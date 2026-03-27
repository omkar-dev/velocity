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
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum SyncMode {
    /// v1 polling-based sync (default).
    Polling,
    /// v2 native probe sync (requires probe embedded in app).
    Native,
    /// Try native first, fallback to polling if probe unavailable.
    #[default]
    Auto,
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
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RetryConfig {
    #[serde(default)]
    pub count: u32,
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

/// Configuration for automatic system dialog dismissal.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DialogConfig {
    /// Whether auto-dismiss is enabled (default: true).
    #[serde(default = "default_dialog_enabled")]
    pub enabled: bool,
    /// Maximum number of dismiss attempts per sync cycle (default: 3).
    #[serde(default = "default_max_dismissals")]
    pub max_dismissals: u32,
    /// Additional button labels to treat as dismiss targets.
    #[serde(default)]
    pub custom_dismiss_labels: Vec<String>,
}

fn default_dialog_enabled() -> bool {
    true
}
fn default_max_dismissals() -> u32 {
    3
}

impl Default for DialogConfig {
    fn default() -> Self {
        Self {
            enabled: default_dialog_enabled(),
            max_dismissals: default_max_dismissals(),
            custom_dismiss_labels: Vec::new(),
        }
    }
}

/// Configuration for self-healing selectors.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealingConfig {
    /// Whether self-healing is enabled (default: true).
    #[serde(default = "default_healing_enabled")]
    pub enabled: bool,
    /// Minimum confidence (0.0–1.0) required to accept a healed match (default: 0.7).
    #[serde(default = "default_confidence_threshold")]
    pub confidence_threshold: f64,
    /// Whether to persist healed selector mappings (default: true).
    #[serde(default = "default_persist_healed")]
    pub persist_healed: bool,
}

fn default_healing_enabled() -> bool {
    true
}
fn default_confidence_threshold() -> f64 {
    0.7
}
fn default_persist_healed() -> bool {
    true
}

impl Default for HealingConfig {
    fn default() -> Self {
        Self {
            enabled: default_healing_enabled(),
            confidence_threshold: default_confidence_threshold(),
            persist_healed: default_persist_healed(),
        }
    }
}

/// Configuration for opt-in resource profiling (heap, PSS, CPU).
/// Disabled by default — zero overhead when off.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PerformanceConfig {
    /// Whether resource profiling is enabled (default: false).
    #[serde(default)]
    pub enabled: bool,
    /// Sampling interval in milliseconds during long-running actions (default: 500).
    #[serde(default = "default_perf_interval")]
    pub interval_ms: u64,
    /// Percentage threshold for heap growth to flag as regression (default: 15).
    #[serde(default = "default_heap_growth_threshold")]
    pub heap_growth_threshold_pct: u32,
}

fn default_perf_interval() -> u64 {
    500
}
fn default_heap_growth_threshold() -> u32 {
    15
}

impl Default for PerformanceConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            interval_ms: default_perf_interval(),
            heap_growth_threshold_pct: default_heap_growth_threshold(),
        }
    }
}

/// Headless driver YAML configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeadlessYamlConfig {
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
    #[serde(default)]
    pub density: Option<f32>,
    #[serde(default)]
    pub baseline_dir: Option<String>,
    #[serde(default)]
    pub app_path: Option<String>,
    #[serde(default)]
    pub initial_layout: Option<String>,
}

/// React Native headless configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReactNativeYamlConfig {
    /// Path to the JS bundle (e.g., index.android.bundle).
    #[serde(default)]
    pub bundle_path: Option<String>,
    /// Root component name.
    #[serde(default)]
    pub component: Option<String>,
    /// Sidecar TCP port (default: 19500).
    #[serde(default)]
    pub port: Option<u16>,
    /// Native module mocks (module_name -> { method: return_value }).
    #[serde(default)]
    pub native_mocks: std::collections::HashMap<String, serde_json::Value>,
}

/// Flutter headless configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlutterYamlConfig {
    /// Path to the Flutter project root.
    #[serde(default)]
    pub project_path: Option<String>,
    /// Dart target file (default: lib/main.dart).
    #[serde(default)]
    pub target: Option<String>,
    /// Directory for golden file baselines.
    #[serde(default)]
    pub golden_dir: Option<String>,
    /// TCP port for Dart test process (default: 19600).
    #[serde(default)]
    pub port: Option<u16>,
}

/// Suite-level configuration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SuiteConfig {
    #[serde(default)]
    pub platform: Option<Platform>,
    /// Driver mode: "device" (default) or "headless".
    #[serde(default)]
    pub driver: Option<String>,
    /// App framework: auto-detected if omitted. Values: "native", "react_native", "flutter".
    #[serde(default)]
    pub framework: Option<String>,
    /// Headless-specific configuration.
    #[serde(default)]
    pub headless: Option<HeadlessYamlConfig>,
    /// React Native bridge configuration.
    #[serde(default)]
    pub react_native: Option<ReactNativeYamlConfig>,
    /// Flutter bridge configuration.
    #[serde(default)]
    pub flutter: Option<FlutterYamlConfig>,
    #[serde(default)]
    pub sync: SyncConfig,
    #[serde(default)]
    pub retry: RetryConfig,
    #[serde(default)]
    pub artifacts: ArtifactsConfig,
    #[serde(default)]
    pub dialog: DialogConfig,
    #[serde(default)]
    pub healing: HealingConfig,
    #[serde(default)]
    pub performance: PerformanceConfig,
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
