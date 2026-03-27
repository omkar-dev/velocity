use serde::{Deserialize, Serialize};

/// Configuration for the React Native bridge.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RnBridgeConfig {
    /// Path to the JS bundle (e.g., index.android.bundle or main.jsbundle)
    pub bundle_path: String,
    /// Root component name (e.g., "App" or "MainScreen")
    #[serde(default = "default_component")]
    pub component: String,
    /// Sidecar TCP port
    #[serde(default = "default_port")]
    pub port: u16,
    /// Screen width for rendering
    #[serde(default = "default_width")]
    pub width: u32,
    /// Screen height for rendering
    #[serde(default = "default_height")]
    pub height: u32,
    /// Native module mocks (module_name -> { method: return_value })
    #[serde(default)]
    pub native_mocks: std::collections::HashMap<String, serde_json::Value>,
}

fn default_component() -> String {
    "App".to_string()
}
fn default_port() -> u16 {
    19500
}
fn default_width() -> u32 {
    1080
}
fn default_height() -> u32 {
    2340
}

impl Default for RnBridgeConfig {
    fn default() -> Self {
        Self {
            bundle_path: String::new(),
            component: default_component(),
            port: default_port(),
            width: default_width(),
            height: default_height(),
            native_mocks: Default::default(),
        }
    }
}
