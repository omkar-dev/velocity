use serde::{Deserialize, Serialize};

/// Report format for test results output.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ReportFormat {
    #[default]
    Junit,
    Json,
}

/// Driver mode — how tests are executed.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum DriverMode {
    /// Execute against a real emulator/simulator/device.
    #[default]
    Device,
    /// CPU-only headless rendering (no emulator needed).
    Headless,
}

/// Detected app framework.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Framework {
    /// Native iOS/Android (UIKit, Storyboard, Android XML).
    #[default]
    Native,
    /// React Native (JS bundle + bridge).
    ReactNative,
    /// Flutter (Dart + RenderObject tree).
    Flutter,
}

impl std::fmt::Display for Framework {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Native => write!(f, "native"),
            Self::ReactNative => write!(f, "react_native"),
            Self::Flutter => write!(f, "flutter"),
        }
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
    pub driver_mode: DriverMode,
    pub framework: Framework,
    pub update_baselines: bool,
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
            driver_mode: DriverMode::default(),
            framework: Framework::default(),
            update_baselines: false,
        }
    }
}
