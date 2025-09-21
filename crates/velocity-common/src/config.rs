use serde::{Deserialize, Serialize};

/// Report format for test results output.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ReportFormat {
    Junit,
    Json,
}

impl Default for ReportFormat {
    fn default() -> Self {
        Self::Junit
    }
}

/// Runtime configuration merged from CLI args and config file.
#[derive(Debug, Clone)]
pub struct RuntimeConfig {
    pub config_path: String,
    pub platform: Option<crate::types::Platform>,
    pub device_id: Option<String>,
    pub tags: Vec<String>,
    pub test_filter: Option<String>,
    pub shard_index: Option<usize>,
    pub shard_total: Option<usize>,
    pub retry_count: Option<u32>,
    pub report_format: ReportFormat,
    pub artifacts_dir: String,
    pub suite_timeout_ms: Option<u64>,
    pub fail_fast: bool,
    pub env_overrides: Vec<(String, String)>,
}

impl Default for RuntimeConfig {
    fn default() -> Self {
        Self {
            config_path: "velocity.yaml".to_string(),
            platform: None,
            device_id: None,
            tags: Vec::new(),
            test_filter: None,
            shard_index: None,
            shard_total: None,
            retry_count: None,
            report_format: ReportFormat::default(),
            artifacts_dir: "./velocity-results".to_string(),
            suite_timeout_ms: None,
            fail_fast: false,
            env_overrides: Vec::new(),
        }
    }
}
