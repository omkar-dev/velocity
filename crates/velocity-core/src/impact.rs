use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::process::Command;

use tracing::{debug, info, warn};
use velocity_common::{Result, VelocityError};

/// Configuration for test impact analysis.
#[derive(Debug, Clone)]
pub struct ImpactConfig {
    /// Whether impact analysis is enabled.
    pub enabled: bool,
    /// Git base ref to diff against (e.g., "main", "origin/main", "HEAD~5").
    pub base_ref: String,
}

impl Default for ImpactConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            base_ref: "main".to_string(),
        }
    }
}

/// Mapping from source files to test flow IDs that exercise them.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ImpactMapping {
    /// Map of flow ID -> list of source file patterns that the flow exercises.
    pub flows: HashMap<String, Vec<String>>,
}

impl ImpactMapping {
    /// Load impact mapping from a YAML file.
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path).map_err(|e| {
            VelocityError::Config(format!(
                "Failed to read impact mapping at {}: {e}",
                path.display()
            ))
        })?;

        serde_yaml::from_str(&content)
            .map_err(|e| VelocityError::Config(format!("Failed to parse impact mapping: {e}")))
    }

    /// Create an empty mapping.
    pub fn empty() -> Self {
        Self {
            flows: HashMap::new(),
        }
    }

    /// Build a reverse index: source file pattern -> set of flow IDs.
    fn reverse_index(&self) -> HashMap<String, HashSet<String>> {
        let mut index: HashMap<String, HashSet<String>> = HashMap::new();
        for (flow_id, patterns) in &self.flows {
            for pattern in patterns {
                index
                    .entry(pattern.clone())
                    .or_default()
                    .insert(flow_id.clone());
            }
        }
        index
    }
}

/// Test impact analyzer that determines which test flows are affected by code changes.
pub struct ImpactAnalyzer {
    config: ImpactConfig,
    mapping: ImpactMapping,
}

impl ImpactAnalyzer {
    pub fn new(config: ImpactConfig, mapping: ImpactMapping) -> Self {
        Self { config, mapping }
    }

    /// Get the list of files changed since the base ref.
    pub fn changed_files(&self) -> Result<Vec<String>> {
        let output = Command::new("git")
            .args([
                "diff",
                "--name-only",
                &format!("{}..HEAD", self.config.base_ref),
            ])
            .output()
            .map_err(|e| VelocityError::Config(format!("Failed to run git diff: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(VelocityError::Config(format!("git diff failed: {stderr}")));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let files: Vec<String> = stdout
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect();

        debug!(count = files.len(), base = %self.config.base_ref, "detected changed files");
        Ok(files)
    }

    /// Determine which flow IDs are affected by the current changes.
    ///
    /// Returns `None` if no mapping exists or impact analysis is disabled,
    /// meaning "run all tests" (safe fallback).
    pub fn affected_flows(&self) -> Result<Option<Vec<String>>> {
        if !self.config.enabled {
            return Ok(None);
        }

        if self.mapping.flows.is_empty() {
            info!("no impact mapping configured, running all tests");
            return Ok(None);
        }

        let changed_files = self.changed_files()?;
        if changed_files.is_empty() {
            info!(
                "no files changed since {}, skipping all tests",
                self.config.base_ref
            );
            return Ok(Some(Vec::new()));
        }

        let reverse_index = self.mapping.reverse_index();
        let mut affected: HashSet<String> = HashSet::new();

        for changed_file in &changed_files {
            for (pattern, flow_ids) in &reverse_index {
                if file_matches_pattern(changed_file, pattern) {
                    for flow_id in flow_ids {
                        affected.insert(flow_id.clone());
                    }
                }
            }
        }

        if affected.is_empty() {
            warn!(
                changed_files = changed_files.len(),
                "no flows matched changed files — consider running all tests"
            );
            // Safe fallback: run all tests if no mappings matched
            return Ok(None);
        }

        let mut result: Vec<String> = affected.into_iter().collect();
        result.sort();

        info!(
            affected = result.len(),
            total_changed = changed_files.len(),
            "impact analysis complete"
        );

        Ok(Some(result))
    }

    /// Filter a list of flow IDs to only those affected by changes.
    /// Returns the original list if impact analysis is disabled or no mapping exists.
    pub fn filter_flows(&self, all_flows: &[String]) -> Result<Vec<String>> {
        match self.affected_flows()? {
            Some(affected) => {
                let affected_set: HashSet<&str> = affected.iter().map(|s| s.as_str()).collect();
                Ok(all_flows
                    .iter()
                    .filter(|f| affected_set.contains(f.as_str()))
                    .cloned()
                    .collect())
            }
            None => Ok(all_flows.to_vec()),
        }
    }
}

/// Check if a file path matches a pattern (supports simple glob-like matching).
fn file_matches_pattern(file: &str, pattern: &str) -> bool {
    // Exact match
    if file == pattern {
        return true;
    }

    // Directory prefix match (pattern "src/screens/" matches "src/screens/Login.tsx")
    if pattern.ends_with('/') && file.starts_with(pattern) {
        return true;
    }

    // Simple wildcard matching
    if pattern.contains('*') {
        return glob_match(pattern, file);
    }

    // Substring match for simple patterns
    if !pattern.contains('/') {
        // Pattern is just a filename — match against file's basename
        if let Some(basename) = file.rsplit('/').next() {
            return basename == pattern;
        }
    }

    false
}

/// Simple glob matching supporting `*` as wildcard and `**` for directory traversal.
fn glob_match(pattern: &str, text: &str) -> bool {
    if pattern == "**" {
        return true;
    }

    // Split on ** for recursive directory matching
    if pattern.contains("**") {
        let parts: Vec<&str> = pattern.split("**").collect();
        if parts.len() == 2 {
            let prefix = parts[0].trim_end_matches('/');
            let suffix = parts[1].trim_start_matches('/');

            let prefix_ok = prefix.is_empty() || text.starts_with(prefix);
            let suffix_ok = suffix.is_empty() || {
                // Check if any suffix of text matches the suffix pattern
                text.ends_with(suffix)
                    || simple_glob_match(suffix, text.rsplit('/').next().unwrap_or(text))
            };

            return prefix_ok && suffix_ok;
        }
    }

    simple_glob_match(pattern, text)
}

/// Simple glob matching with single `*` wildcards only.
fn simple_glob_match(pattern: &str, text: &str) -> bool {
    if !pattern.contains('*') {
        return pattern == text;
    }

    let parts: Vec<&str> = pattern.split('*').collect();
    let mut pos = 0;

    for (i, part) in parts.iter().enumerate() {
        if part.is_empty() {
            continue;
        }
        match text[pos..].find(part) {
            Some(idx) => {
                if i == 0 && idx != 0 {
                    return false;
                }
                pos += idx + part.len();
            }
            None => return false,
        }
    }

    if !pattern.ends_with('*') {
        return pos == text.len();
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_file_matches_exact() {
        assert!(file_matches_pattern("src/Login.tsx", "src/Login.tsx"));
    }

    #[test]
    fn test_file_matches_directory_prefix() {
        assert!(file_matches_pattern(
            "src/screens/Login.tsx",
            "src/screens/"
        ));
        assert!(!file_matches_pattern("src/api/auth.ts", "src/screens/"));
    }

    #[test]
    fn test_file_matches_wildcard() {
        assert!(file_matches_pattern(
            "src/screens/Login.tsx",
            "src/screens/*.tsx"
        ));
        assert!(!file_matches_pattern(
            "src/screens/Login.ts",
            "src/screens/*.tsx"
        ));
    }

    #[test]
    fn test_file_matches_double_star() {
        assert!(file_matches_pattern(
            "src/screens/Login.tsx",
            "src/**/*.tsx"
        ));
        assert!(file_matches_pattern(
            "src/deep/nested/File.tsx",
            "src/**/*.tsx"
        ));
    }

    #[test]
    fn test_file_matches_basename() {
        assert!(file_matches_pattern("src/utils/auth.ts", "auth.ts"));
    }

    #[test]
    fn test_reverse_index() {
        let mapping = ImpactMapping {
            flows: HashMap::from([
                (
                    "login_test".to_string(),
                    vec!["src/screens/Login.tsx".to_string()],
                ),
                (
                    "checkout_test".to_string(),
                    vec![
                        "src/screens/Login.tsx".to_string(),
                        "src/api/cart.ts".to_string(),
                    ],
                ),
            ]),
        };

        let index = mapping.reverse_index();
        let login_flows = index.get("src/screens/Login.tsx").unwrap();
        assert!(login_flows.contains("login_test"));
        assert!(login_flows.contains("checkout_test"));

        let cart_flows = index.get("src/api/cart.ts").unwrap();
        assert!(cart_flows.contains("checkout_test"));
        assert!(!cart_flows.contains("login_test"));
    }

    #[test]
    fn test_filter_flows_disabled() {
        let config = ImpactConfig {
            enabled: false,
            base_ref: "main".to_string(),
        };
        let analyzer = ImpactAnalyzer::new(config, ImpactMapping::empty());
        let all = vec!["flow1".to_string(), "flow2".to_string()];
        let result = analyzer.filter_flows(&all).unwrap();
        assert_eq!(result, all); // all flows returned when disabled
    }

    #[test]
    fn test_filter_flows_empty_mapping_returns_all() {
        let config = ImpactConfig {
            enabled: true,
            base_ref: "main".to_string(),
        };
        let analyzer = ImpactAnalyzer::new(config, ImpactMapping::empty());
        let all = vec!["flow1".to_string(), "flow2".to_string()];
        let result = analyzer.filter_flows(&all).unwrap();
        assert_eq!(result, all);
    }

    #[test]
    fn test_glob_match_double_star() {
        assert!(glob_match("src/**/*.tsx", "src/screens/Login.tsx"));
        assert!(glob_match("**/*.ts", "src/api/auth.ts"));
        assert!(!glob_match("src/**/*.tsx", "lib/other.tsx"));
    }

    #[test]
    fn test_simple_glob_match() {
        assert!(simple_glob_match("*.tsx", "Login.tsx"));
        assert!(!simple_glob_match("*.tsx", "Login.ts"));
        assert!(simple_glob_match("Login*", "LoginScreen.tsx"));
    }
}
