use serde::{Deserialize, Serialize};

/// Configuration for the Flutter headless bridge.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FlutterBridgeConfig {
    /// Path to the Flutter project root (containing pubspec.yaml).
    pub project_path: String,
    /// Dart target file (default: lib/main.dart).
    #[serde(default = "default_target")]
    pub target: String,
    /// Screen width for rendering.
    #[serde(default = "default_width")]
    pub width: u32,
    /// Screen height for rendering.
    #[serde(default = "default_height")]
    pub height: u32,
    /// Directory for golden file baselines.
    #[serde(default)]
    pub golden_dir: Option<String>,
    /// TCP port for communication with the Dart test process.
    #[serde(default = "default_port")]
    pub port: u16,
}

fn default_target() -> String { "lib/main.dart".to_string() }
fn default_width() -> u32 { 1080 }
fn default_height() -> u32 { 2340 }
fn default_port() -> u16 { 19600 }

impl Default for FlutterBridgeConfig {
    fn default() -> Self {
        Self {
            project_path: ".".to_string(),
            target: default_target(),
            width: default_width(),
            height: default_height(),
            golden_dir: None,
            port: default_port(),
        }
    }
}
