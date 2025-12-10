use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use velocity_common::TestCase;

use crate::history::TestHistory;

/// Keep only tests that have at least one tag matching the provided list.
pub fn filter_by_tags(tests: &[TestCase], tags: &[String]) -> Vec<TestCase> {
    if tags.is_empty() {
        return tests.to_vec();
    }
    tests
        .iter()
        .filter(|t| t.tags.iter().any(|tag| tags.contains(tag)))
        .cloned()
        .collect()
}

/// Keep only tests whose name matches a glob-like pattern.
///
/// Supports `*` as a wildcard matching any sequence of characters. The match
/// is case-insensitive.
pub fn filter_by_name(tests: &[TestCase], pattern: &str) -> Vec<TestCase> {
    if pattern.is_empty() {
        return tests.to_vec();
    }
    tests
        .iter()
        .filter(|t| glob_match(pattern, &t.name))
        .cloned()
        .collect()
}

fn glob_match(pattern: &str, value: &str) -> bool {
    let pattern = pattern.to_lowercase();
    let value = value.to_lowercase();

    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return value == pattern;
    }

    let mut pos = 0usize;
    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        match value[pos..].find(part) {
            Some(found) => {
                // The first segment must anchor to the start if the pattern
                // doesn't begin with '*'.
                if i == 0 && found != 0 {
                    return false;
                }
                pos += found + part.len();
            }
            None => return false,
        }
    }

    // If the pattern doesn't end with '*', the value must end at exactly pos.
    if !pattern.ends_with('*') && pos != value.len() {
        return false;
    }

    true
}

/// Distribute tests across shards.
///
/// When `history` is provided, tests are distributed using duration-balanced
/// greedy bin-packing so that each shard has roughly equal total duration.
/// Without history, tests are assigned deterministically via hash of the test
/// name.
pub fn shard_tests(
    tests: Vec<TestCase>,
    shard_index: usize,
    shard_total: usize,
    history: Option<&TestHistory>,
) -> Vec<TestCase> {
    if shard_total <= 1 {
        return tests;
    }

    match history {
        Some(hist) if !hist.durations.is_empty() => {
            shard_by_duration(tests, shard_index, shard_total, hist)
        }
        _ => shard_by_hash(tests, shard_index, shard_total),
    }
}

fn shard_by_hash(tests: Vec<TestCase>, shard_index: usize, shard_total: usize) -> Vec<TestCase> {
    tests
        .into_iter()
        .filter(|t| {
            let mut hasher = DefaultHasher::new();
            t.name.hash(&mut hasher);
            (hasher.finish() as usize) % shard_total == shard_index
        })
        .collect()
}

fn shard_by_duration(
    tests: Vec<TestCase>,
    shard_index: usize,
    shard_total: usize,
    history: &TestHistory,
) -> Vec<TestCase> {
    // Sort tests by duration descending so the largest jobs get placed first
    // (greedy bin-packing heuristic).
    let default_duration = 5000u64;
    let mut indexed: Vec<(usize, &TestCase, u64)> = tests
        .iter()
        .enumerate()
        .map(|(i, t)| {
            let dur = history
                .durations
                .get(&t.name)
                .copied()
                .unwrap_or(default_duration);
            (i, t, dur)
        })
        .collect();

    indexed.sort_by(|a, b| b.2.cmp(&a.2));

    // Greedy assignment: always place the next test into the shard with the
    // smallest accumulated duration.
    let mut shard_loads = vec![0u64; shard_total];
    let mut assignments = vec![0usize; tests.len()];

    for &(orig_idx, _, dur) in &indexed {
        let target = shard_loads
            .iter()
            .enumerate()
            .min_by_key(|(_, &load)| load)
            .map(|(i, _)| i)
            .unwrap_or(0);
        assignments[orig_idx] = target;
        shard_loads[target] += dur;
    }

    tests
        .into_iter()
        .enumerate()
        .filter(|(i, _)| assignments[*i] == shard_index)
        .map(|(_, t)| t)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use velocity_common::{Action, Selector, Step};

    fn make_test(name: &str, tags: Vec<&str>) -> TestCase {
        TestCase {
            name: name.to_string(),
            tags: tags.into_iter().map(String::from).collect(),
            isolated: false,
            steps: vec![Step {
                action: Action::Wait { ms: 100 },
                timeout_ms: None,
            }],
        }
    }

    #[test]
    fn test_filter_by_tags_empty_tags_returns_all() {
        let tests = vec![make_test("a", vec!["smoke"]), make_test("b", vec!["reg"])];
        let result = filter_by_tags(&tests, &[]);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_filter_by_tags_matches() {
        let tests = vec![
            make_test("a", vec!["smoke", "fast"]),
            make_test("b", vec!["regression"]),
            make_test("c", vec!["smoke"]),
        ];
        let result = filter_by_tags(&tests, &["smoke".to_string()]);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].name, "a");
        assert_eq!(result[1].name, "c");
    }

    #[test]
    fn test_filter_by_tags_no_match() {
        let tests = vec![make_test("a", vec!["smoke"])];
        let result = filter_by_tags(&tests, &["nightly".to_string()]);
        assert!(result.is_empty());
    }

    #[test]
    fn test_filter_by_name_wildcard() {
        let tests = vec![
            make_test("Login flow", vec![]),
            make_test("Login error handling", vec![]),
            make_test("Dashboard view", vec![]),
        ];
        let result = filter_by_name(&tests, "Login*");
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_filter_by_name_case_insensitive() {
        let tests = vec![make_test("Login Flow", vec![])];
        let result = filter_by_name(&tests, "login*");
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_filter_by_name_middle_wildcard() {
        let tests = vec![
            make_test("User login test", vec![]),
            make_test("User signup test", vec![]),
            make_test("Admin login test", vec![]),
        ];
        let result = filter_by_name(&tests, "User*test");
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_shard_by_hash_deterministic() {
        let tests: Vec<TestCase> = (0..20).map(|i| make_test(&format!("test_{i}"), vec![])).collect();

        let shard0 = shard_tests(tests.clone(), 0, 3, None);
        let shard1 = shard_tests(tests.clone(), 1, 3, None);
        let shard2 = shard_tests(tests.clone(), 2, 3, None);

        // Every test should be in exactly one shard
        let total = shard0.len() + shard1.len() + shard2.len();
        assert_eq!(total, 20);

        // Running again should produce the same assignment
        let shard0_again = shard_tests(tests.clone(), 0, 3, None);
        assert_eq!(
            shard0.iter().map(|t| &t.name).collect::<Vec<_>>(),
            shard0_again.iter().map(|t| &t.name).collect::<Vec<_>>()
        );
    }

    #[test]
    fn test_shard_by_hash_no_overlap() {
        let tests: Vec<TestCase> = (0..10).map(|i| make_test(&format!("test_{i}"), vec![])).collect();
        let shard0 = shard_tests(tests.clone(), 0, 2, None);
        let shard1 = shard_tests(tests.clone(), 1, 2, None);

        let names0: Vec<&str> = shard0.iter().map(|t| t.name.as_str()).collect();
        let names1: Vec<&str> = shard1.iter().map(|t| t.name.as_str()).collect();

        for name in &names0 {
            assert!(!names1.contains(name), "test {name} appears in both shards");
        }
    }

    #[test]
    fn test_shard_by_duration_balanced() {
        use std::collections::HashMap;

        let tests = vec![
            make_test("slow", vec![]),
            make_test("medium", vec![]),
            make_test("fast1", vec![]),
            make_test("fast2", vec![]),
        ];

        let mut durations = HashMap::new();
        durations.insert("slow".to_string(), 10000u64);
        durations.insert("medium".to_string(), 5000);
        durations.insert("fast1".to_string(), 1000);
        durations.insert("fast2".to_string(), 1000);

        let history = TestHistory { durations };

        let shard0 = shard_tests(tests.clone(), 0, 2, Some(&history));
        let shard1 = shard_tests(tests.clone(), 1, 2, Some(&history));

        assert_eq!(shard0.len() + shard1.len(), 4);

        // The slow test (10s) should be alone or paired with the smallest
        // tests, while medium gets the other fast tests. This checks that
        // the bin-packing doesn't put slow + medium in the same shard.
        let shard0_names: Vec<&str> = shard0.iter().map(|t| t.name.as_str()).collect();
        let shard1_names: Vec<&str> = shard1.iter().map(|t| t.name.as_str()).collect();

        let has_slow_and_medium_together = (shard0_names.contains(&"slow")
            && shard0_names.contains(&"medium"))
            || (shard1_names.contains(&"slow") && shard1_names.contains(&"medium"));
        assert!(
            !has_slow_and_medium_together,
            "slow and medium should be in different shards for balance"
        );
    }

    #[test]
    fn test_shard_single_shard_returns_all() {
        let tests = vec![make_test("a", vec![]), make_test("b", vec![])];
        let result = shard_tests(tests.clone(), 0, 1, None);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_glob_exact_match() {
        assert!(glob_match("hello", "hello"));
        assert!(!glob_match("hello", "world"));
    }

    #[test]
    fn test_glob_trailing_wildcard() {
        assert!(glob_match("hello*", "hello world"));
        assert!(glob_match("hello*", "hello"));
        assert!(!glob_match("hello*", "hi hello"));
    }

    #[test]
    fn test_glob_leading_wildcard() {
        assert!(glob_match("*world", "hello world"));
        assert!(!glob_match("*world", "world!"));
    }
}
