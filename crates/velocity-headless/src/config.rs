use serde::{Deserialize, Serialize};

/// Configuration for the headless rendering driver.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeadlessConfig {
    /// Viewport width in pixels.
    #[serde(default = "default_width")]
    pub width: u32,
    /// Viewport height in pixels.
    #[serde(default = "default_height")]
    pub height: u32,
    /// Device pixel density (1.0 = mdpi, 2.0 = xhdpi, 3.0 = xxhdpi).
    #[serde(default = "default_density")]
    pub density: f32,
    /// Directory for snapshot baselines.
    #[serde(default = "default_baseline_dir")]
    pub baseline_dir: String,
    /// Pixel difference threshold for snapshot comparison (0.0 - 1.0).
    #[serde(default = "default_diff_threshold")]
    pub diff_threshold: f64,
    /// Path to APK (Android) or .app bundle (iOS).
    #[serde(default)]
    pub app_path: Option<String>,
    /// Initial layout name (e.g., "@layout/activity_main" or "Main.xib").
    #[serde(default)]
    pub initial_layout: Option<String>,
}

fn default_width() -> u32 {
    1080
}
fn default_height() -> u32 {
    1920
}
fn default_density() -> f32 {
    2.0
}
fn default_baseline_dir() -> String {
    "./velocity-baselines".to_string()
}
fn default_diff_threshold() -> f64 {
    0.001 // 0.1% pixel difference
}

impl Default for HeadlessConfig {
    fn default() -> Self {
        Self {
            width: default_width(),
            height: default_height(),
            density: default_density(),
            baseline_dir: default_baseline_dir(),
            diff_threshold: default_diff_threshold(),
            app_path: None,
            initial_layout: None,
        }
    }
}
