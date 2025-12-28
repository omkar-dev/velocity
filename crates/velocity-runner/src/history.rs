use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use velocity_common::{Result, TestResult, VelocityError};

const HISTORY_FILENAME: &str = "velocity-history.json";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TestHistory {
    pub durations: HashMap<String, u64>,
}

/// Load test history from a JSON file at the given directory path.
///
/// The file is expected at `{path}/velocity-history.json`. Returns an empty
/// history if the file does not exist.
pub fn load(path: &str) -> Result<TestHistory> {
    let file_path = std::path::Path::new(path).join(HISTORY_FILENAME);
    if !file_path.exists() {
        return Ok(TestHistory::default());
    }

    let contents = std::fs::read_to_string(&file_path).map_err(|e| {
        VelocityError::Config(format!(
            "failed to read history file {}: {e}",
            file_path.display()
        ))
    })?;

    let history: TestHistory = serde_json::from_str(&contents).map_err(|e| {
        VelocityError::Config(format!(
            "failed to parse history file {}: {e}",
            file_path.display()
        ))
    })?;

    Ok(history)
}

/// Save test history to a JSON file at the given directory path.
pub fn save(path: &str, history: &TestHistory) -> Result<()> {
    let dir = std::path::Path::new(path);
    std::fs::create_dir_all(dir).map_err(|e| {
        VelocityError::Config(format!(
            "failed to create history directory {}: {e}",
            dir.display()
        ))
    })?;

    let file_path = dir.join(HISTORY_FILENAME);
    let json = serde_json::to_string_pretty(history).map_err(|e| {
        VelocityError::Config(format!("failed to serialize history: {e}"))
    })?;

    std::fs::write(&file_path, json).map_err(|e| {
        VelocityError::Config(format!(
            "failed to write history file {}: {e}",
            file_path.display()
        ))
    })?;

    Ok(())
}

/// Update the history with durations from the latest test results.
pub fn update(history: &mut TestHistory, results: &[TestResult]) {
    for result in results {
        let duration_ms = result.duration.as_millis() as u64;
        history
            .durations
            .insert(result.test_name.clone(), duration_ms);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::Duration;
    use velocity_common::TestStatus;

    fn make_result(name: &str, duration_ms: u64) -> TestResult {
        TestResult {
            test_name: name.to_string(),
            status: TestStatus::Passed,
            duration: Duration::from_millis(duration_ms),
            steps: vec![],
            retries: 0,
            error_message: None,
            screenshots: vec![],
        }
    }

    #[test]
    fn test_update_inserts_new_entries() {
        let mut history = TestHistory::default();
        let results = vec![make_result("test_a", 1500), make_result("test_b", 3200)];

        update(&mut history, &results);

        assert_eq!(history.durations["test_a"], 1500);
        assert_eq!(history.durations["test_b"], 3200);
    }

    #[test]
    fn test_update_overwrites_existing() {
        let mut history = TestHistory::default();
        history.durations.insert("test_a".to_string(), 1000);

        let results = vec![make_result("test_a", 2000)];
        update(&mut history, &results);

        assert_eq!(history.durations["test_a"], 2000);
    }

    #[test]
    fn test_load_nonexistent_returns_empty() {
        let history = load("/tmp/velocity_test_nonexistent_dir_12345").unwrap();
        assert!(history.durations.is_empty());
    }

    #[test]
    fn test_save_and_load_roundtrip() {
        let dir = std::env::temp_dir().join("velocity_history_test");
        let _ = std::fs::remove_dir_all(&dir);

        let mut history = TestHistory::default();
        history.durations.insert("login_test".to_string(), 4500);
        history.durations.insert("signup_test".to_string(), 6200);

        let dir_str = dir.to_str().unwrap();
        save(dir_str, &history).unwrap();

        let loaded = load(dir_str).unwrap();
        assert_eq!(loaded.durations["login_test"], 4500);
        assert_eq!(loaded.durations["signup_test"], 6200);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
