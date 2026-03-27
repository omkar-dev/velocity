use std::path::PathBuf;
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// Resource metrics captured at a point in time.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceSnapshot {
    pub java_heap_kb: u64,
    pub native_heap_kb: u64,
    pub total_pss_kb: u64,
    pub cpu_percent: f32,
    pub timestamp_ms: u64,
}

/// Resource delta comparing before/after metrics for a step.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceDelta {
    pub before: ResourceSnapshot,
    pub after: ResourceSnapshot,
    pub heap_growth_kb: i64,
}

/// The result of executing a single step.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepResult {
    pub step_index: usize,
    pub action_name: String,
    pub status: StepStatus,
    pub duration: Duration,
    pub screenshot: Option<PathBuf>,
    pub error_message: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_delta: Option<ResourceDelta>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_peak: Option<ResourceSnapshot>,
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
