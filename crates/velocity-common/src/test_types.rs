use serde::{Deserialize, Serialize};

use crate::types::{Action, Platform};

/// A single step in a test or flow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Step {
    pub action: Action,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

/// A reusable flow (sequence of steps).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Flow {
    pub id: String,
    pub steps: Vec<Step>,
}

/// A test case containing steps to execute.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestCase {
    pub name: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub isolated: bool,
    pub steps: Vec<Step>,
}

/// Sync engine mode selection.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum SyncMode {
    /// v1 polling-based sync (default).
    Polling,
    /// v2 native probe sync (requires probe embedded in app).
    Native,
    /// Try native first, fallback to polling if probe unavailable.
    Auto,
}

impl Default for SyncMode {
    fn default() -> Self {
        Self::Auto
    }
}

/// Sync engine configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncConfig {
    #[serde(default)]
    pub mode: SyncMode,
    #[serde(default = "default_sync_interval")]
    pub interval_ms: u64,
    #[serde(default = "default_stability_count")]
    pub stability_count: u32,
    #[serde(default = "default_sync_timeout")]
    pub timeout_ms: u64,
    #[serde(default = "default_adaptive")]
    pub adaptive: bool,
    #[serde(default = "default_native_port_ios")]
    pub native_port_ios: u16,
    #[serde(default = "default_native_port_android")]
    pub native_port_android: u16,
    #[serde(default = "default_probe_connect_timeout")]
    pub probe_connect_timeout_ms: u64,
}

fn default_sync_interval() -> u64 {
    200
}
fn default_stability_count() -> u32 {
    3
}
fn default_sync_timeout() -> u64 {
    10000
}
fn default_adaptive() -> bool {
    true
}
fn default_native_port_ios() -> u16 {
    19400
}
fn default_native_port_android() -> u16 {
    19401
}
fn default_probe_connect_timeout() -> u64 {
    2000
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            mode: SyncMode::default(),
            interval_ms: default_sync_interval(),
            stability_count: default_stability_count(),
            timeout_ms: default_sync_timeout(),
            adaptive: default_adaptive(),
            native_port_ios: default_native_port_ios(),
            native_port_android: default_native_port_android(),
            probe_connect_timeout_ms: default_probe_connect_timeout(),
        }
    }
}

/// Retry configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryConfig {
    #[serde(default)]
    pub count: u32,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self { count: 0 }
    }
}

/// Artifact collection configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactsConfig {
    #[serde(default = "default_on_failure")]
    pub on_failure: bool,
    #[serde(default = "default_output_dir")]
    pub output_dir: String,
}

fn default_on_failure() -> bool {
    true
}
fn default_output_dir() -> String {
    "./velocity-results".to_string()
}

impl Default for ArtifactsConfig {
    fn default() -> Self {
        Self {
            on_failure: default_on_failure(),
            output_dir: default_output_dir(),
        }
    }
}

/// Suite-level configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuiteConfig {
    #[serde(default)]
    pub platform: Option<Platform>,
    #[serde(default)]
    pub sync: SyncConfig,
    #[serde(default)]
    pub retry: RetryConfig,
    #[serde(default)]
    pub artifacts: ArtifactsConfig,
}

impl Default for SuiteConfig {
    fn default() -> Self {
        Self {
            platform: None,
            sync: SyncConfig::default(),
            retry: RetryConfig::default(),
            artifacts: ArtifactsConfig::default(),
        }
    }
}

/// A complete test suite parsed from a velocity.yaml file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestSuite {
    #[serde(rename = "appId")]
    pub app_id: String,
    #[serde(default)]
    pub config: SuiteConfig,
    #[serde(default)]
    pub flows: Vec<Flow>,
    pub tests: Vec<TestCase>,
}
