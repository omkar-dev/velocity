use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// The result of executing a single step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepResult {
    pub step_index: usize,
    pub action_name: String,
    pub status: StepStatus,
    pub duration: Duration,
    pub screenshot: Option<PathBuf>,
    pub error_message: Option<String>,
}

/// Status of a single step.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum StepStatus {
    Passed,
    Failed,
    Skipped,
}

/// The result of executing a single test case.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestResult {
    pub test_name: String,
    pub status: TestStatus,
    pub duration: Duration,
    pub steps: Vec<StepResult>,
    pub retries: u32,
    pub error_message: Option<String>,
    pub screenshots: Vec<PathBuf>,
}

/// Status of a test case.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum TestStatus {
    Passed,
    Failed,
    Skipped,
    Retried,
}

/// The result of executing the entire test suite.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuiteResult {
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub skipped: usize,
    pub retried: usize,
    pub duration: Duration,
    pub tests: Vec<TestResult>,
    pub shard_index: Option<usize>,
    pub shard_total: Option<usize>,
}

impl SuiteResult {
    pub fn exit_code(&self) -> i32 {
        if self.failed > 0 {
            1
        } else {
            0
        }
    }
}
