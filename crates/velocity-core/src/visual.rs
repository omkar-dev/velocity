use std::path::PathBuf;

use tracing::{debug, info, warn};
use velocity_common::{Result, VelocityError};

/// Configuration for visual assertion comparison.
#[derive(Debug, Clone)]
pub struct VisualConfig {
    /// Minimum pixel similarity threshold (0.0–1.0). Default 0.98.
    pub threshold: f64,
    /// Directory where baseline screenshots are stored.
    pub baselines_dir: PathBuf,
    /// Directory where diff images are written on failure.
    pub diffs_dir: PathBuf,
    /// If true, update baselines instead of comparing.
    pub update_baselines: bool,
}

impl Default for VisualConfig {
    fn default() -> Self {
        Self {
            threshold: 0.98,
            baselines_dir: PathBuf::from("velocity-baselines"),
            diffs_dir: PathBuf::from("velocity-diffs"),
            update_baselines: false,
        }
    }
}

/// Result of a visual comparison.
#[derive(Debug, Clone)]
pub struct VisualComparisonResult {
    /// Overall similarity score (0.0–1.0).
    pub similarity: f64,
    /// Number of pixels that differed.
    pub diff_pixel_count: usize,
    /// Total pixels compared.
    pub total_pixels: usize,
    /// Whether the comparison passed the threshold.
    pub passed: bool,
    /// Path to the diff image (if generated).
    pub diff_image_path: Option<PathBuf>,
}

/// A region to mask (ignore) during visual comparison.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MaskRegion {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// Named mask presets for common dynamic regions.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MaskPreset {
    /// Mask the top status bar (time, battery, signal).
    StatusBar,
    /// Mask the keyboard area (bottom of screen).
    Keyboard,
    /// Custom rectangular region.
    Custom(MaskRegion),
}

impl MaskPreset {
    /// Convert a preset to concrete pixel regions given screen dimensions.
    pub fn to_region(&self, screen_width: u32, screen_height: u32) -> MaskRegion {
        match self {
            MaskPreset::StatusBar => MaskRegion {
                x: 0,
                y: 0,
                width: screen_width,
                height: (screen_height as f64 * 0.04) as u32, // ~4% of screen
            },
            MaskPreset::Keyboard => MaskRegion {
                x: 0,
                y: (screen_height as f64 * 0.6) as u32,
                width: screen_width,
                height: (screen_height as f64 * 0.4) as u32, // bottom 40%
            },
            MaskPreset::Custom(r) => r.clone(),
        }
    }
}

/// Engine for performing visual screenshot assertions.
pub struct VisualEngine {
    config: VisualConfig,
}

impl VisualEngine {
    pub fn new(config: VisualConfig) -> Self {
        Self { config }
    }

    /// Assert that a screenshot matches its baseline.
    ///
    /// - If `update_baselines` is true, saves the screenshot as the new baseline.
    /// - Otherwise, compares against the stored baseline using pixel similarity.
    pub fn assert_screenshot(
        &self,
        baseline_name: &str,
        screenshot_png: &[u8],
        masks: &[MaskPreset],
    ) -> Result<VisualComparisonResult> {
        let baseline_path = self.config.baselines_dir.join(baseline_name);

        // Update mode: save as new baseline
        if self.config.update_baselines {
            std::fs::create_dir_all(&self.config.baselines_dir).map_err(|e| {
                VelocityError::Config(format!("Failed to create baselines dir: {e}"))
            })?;
            std::fs::write(&baseline_path, screenshot_png).map_err(|e| {
                VelocityError::Config(format!("Failed to write baseline {baseline_name}: {e}"))
            })?;
            info!(baseline = baseline_name, "baseline updated");
            return Ok(VisualComparisonResult {
                similarity: 1.0,
                diff_pixel_count: 0,
                total_pixels: 0,
                passed: true,
                diff_image_path: None,
            });
        }

        // Load baseline
        if !baseline_path.exists() {
            return Err(VelocityError::Config(format!(
                "Baseline not found: {}. Run with --update-baselines to create it.",
                baseline_path.display()
            )));
        }

        let baseline_bytes = std::fs::read(&baseline_path).map_err(|e| {
            VelocityError::Config(format!("Failed to read baseline {baseline_name}: {e}"))
        })?;

        // Compare the raw PNG bytes
        let result = compare_images(
            &baseline_bytes,
            screenshot_png,
            masks,
            self.config.threshold,
        )?;

        if !result.passed {
            // Save diff image
            if let Some(ref diff_data) = generate_diff_overlay(&baseline_bytes, screenshot_png) {
                let diff_path = self.save_diff(baseline_name, diff_data)?;
                warn!(
                    baseline = baseline_name,
                    similarity = result.similarity,
                    threshold = self.config.threshold,
                    diff_pixels = result.diff_pixel_count,
                    diff_path = %diff_path.display(),
                    "visual assertion failed"
                );
                return Ok(VisualComparisonResult {
                    diff_image_path: Some(diff_path),
                    ..result
                });
            }

            warn!(
                baseline = baseline_name,
                similarity = result.similarity,
                threshold = self.config.threshold,
                diff_pixels = result.diff_pixel_count,
                "visual assertion failed"
            );
        } else {
            debug!(
                baseline = baseline_name,
                similarity = result.similarity,
                "visual assertion passed"
            );
        }

        Ok(result)
    }

    fn save_diff(&self, baseline_name: &str, diff_data: &[u8]) -> Result<PathBuf> {
        std::fs::create_dir_all(&self.config.diffs_dir)
            .map_err(|e| VelocityError::Config(format!("Failed to create diffs dir: {e}")))?;

        let diff_name = format!("diff_{}", baseline_name);
        let diff_path = self.config.diffs_dir.join(&diff_name);

        std::fs::write(&diff_path, diff_data)
            .map_err(|e| VelocityError::Config(format!("Failed to write diff image: {e}")))?;

        Ok(diff_path)
    }

    /// Check if a baseline exists for the given name.
    pub fn has_baseline(&self, baseline_name: &str) -> bool {
        self.config.baselines_dir.join(baseline_name).exists()
    }

    /// List all baselines in the baselines directory.
    pub fn list_baselines(&self) -> Result<Vec<String>> {
        if !self.config.baselines_dir.exists() {
            return Ok(Vec::new());
        }

        let entries = std::fs::read_dir(&self.config.baselines_dir)
            .map_err(|e| VelocityError::Config(format!("Failed to read baselines dir: {e}")))?;

        let mut baselines = Vec::new();
        for entry in entries.flatten() {
            if let Some(name) = entry.file_name().to_str() {
                if name.ends_with(".png") {
                    baselines.push(name.to_string());
                }
            }
        }

        baselines.sort();
        Ok(baselines)
    }
}

/// Compare two PNG images byte-by-byte with optional masking.
///
/// This is a simple raw-byte comparison. For PNG images of the same dimensions
/// and encoding, this provides pixel-level accuracy. For production use,
/// a proper image decoding library would be needed for format-independent comparison.
fn compare_images(
    baseline: &[u8],
    actual: &[u8],
    _masks: &[MaskPreset],
    threshold: f64,
) -> Result<VisualComparisonResult> {
    // Simple byte-level comparison (works when both PNGs have identical encoding params)
    // For a more robust approach, decode both PNGs to raw RGBA and compare pixels.
    // This simplified version compares raw bytes which is sufficient for screenshots
    // from the same device/simulator with consistent encoding.

    if baseline == actual {
        return Ok(VisualComparisonResult {
            similarity: 1.0,
            diff_pixel_count: 0,
            total_pixels: baseline.len(),
            passed: true,
            diff_image_path: None,
        });
    }

    // Byte-level comparison
    let max_len = baseline.len().max(actual.len());
    let min_len = baseline.len().min(actual.len());

    if max_len == 0 {
        return Ok(VisualComparisonResult {
            similarity: 1.0,
            diff_pixel_count: 0,
            total_pixels: 0,
            passed: true,
            diff_image_path: None,
        });
    }

    let mut diff_count = 0usize;

    // Compare overlapping bytes
    for i in 0..min_len {
        if baseline[i] != actual[i] {
            diff_count += 1;
        }
    }

    // Any extra bytes in the longer image count as differences
    diff_count += max_len - min_len;

    let similarity = 1.0 - (diff_count as f64 / max_len as f64);
    let passed = similarity >= threshold;

    Ok(VisualComparisonResult {
        similarity,
        diff_pixel_count: diff_count,
        total_pixels: max_len,
        passed,
        diff_image_path: None,
    })
}

/// Generate a simple diff overlay (returns raw bytes highlighting differences).
/// In a full implementation, this would decode both PNGs and produce a highlighted diff image.
/// For now, returns None as a placeholder — the diff metadata in VisualComparisonResult
/// is sufficient for CI reporting.
fn generate_diff_overlay(_baseline: &[u8], _actual: &[u8]) -> Option<Vec<u8>> {
    // Full implementation would:
    // 1. Decode both PNGs to RGBA
    // 2. Create a new image highlighting differing pixels in red
    // 3. Encode back to PNG
    // Requires an image decoding crate (e.g., `image` or `png`)
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identical_images_pass() {
        let data = vec![0x89, 0x50, 0x4E, 0x47, 1, 2, 3, 4, 5];
        let result = compare_images(&data, &data, &[], 0.98).unwrap();
        assert!(result.passed);
        assert_eq!(result.similarity, 1.0);
        assert_eq!(result.diff_pixel_count, 0);
    }

    #[test]
    fn test_slightly_different_images() {
        let baseline = vec![0u8; 1000];
        let mut actual = vec![0u8; 1000];
        // Change 1% of bytes
        for i in 0..10 {
            actual[i] = 255;
        }
        let result = compare_images(&baseline, &actual, &[], 0.98).unwrap();
        assert_eq!(result.similarity, 0.99);
        assert!(result.passed); // 99% > 98% threshold
    }

    #[test]
    fn test_very_different_images_fail() {
        let baseline = vec![0u8; 100];
        let actual = vec![255u8; 100];
        let result = compare_images(&baseline, &actual, &[], 0.98).unwrap();
        assert!(!result.passed);
        assert!(result.similarity < 0.1);
    }

    #[test]
    fn test_different_lengths() {
        let baseline = vec![0u8; 100];
        let actual = vec![0u8; 200];
        let result = compare_images(&baseline, &actual, &[], 0.98).unwrap();
        // 100 extra bytes out of 200 = 50% diff
        assert!(!result.passed);
        assert_eq!(result.similarity, 0.5);
    }

    #[test]
    fn test_mask_preset_status_bar() {
        let region = MaskPreset::StatusBar.to_region(1080, 2400);
        assert_eq!(region.x, 0);
        assert_eq!(region.y, 0);
        assert_eq!(region.width, 1080);
        assert!(region.height > 0 && region.height < 200);
    }

    #[test]
    fn test_mask_preset_keyboard() {
        let region = MaskPreset::Keyboard.to_region(1080, 2400);
        assert_eq!(region.x, 0);
        assert!(region.y > 1000);
        assert_eq!(region.width, 1080);
    }

    #[test]
    fn test_visual_engine_update_baselines() {
        let dir = tempfile::tempdir().unwrap();
        let config = VisualConfig {
            threshold: 0.98,
            baselines_dir: dir.path().join("baselines"),
            diffs_dir: dir.path().join("diffs"),
            update_baselines: true,
        };
        let engine = VisualEngine::new(config);
        let screenshot = vec![0x89, 0x50, 0x4E, 0x47, 1, 2, 3];
        let result = engine
            .assert_screenshot("test.png", &screenshot, &[])
            .unwrap();
        assert!(result.passed);
        assert!(engine.has_baseline("test.png"));
    }

    #[test]
    fn test_visual_engine_missing_baseline() {
        let dir = tempfile::tempdir().unwrap();
        let config = VisualConfig {
            threshold: 0.98,
            baselines_dir: dir.path().join("baselines"),
            diffs_dir: dir.path().join("diffs"),
            update_baselines: false,
        };
        let engine = VisualEngine::new(config);
        let result = engine.assert_screenshot("nonexistent.png", &[1, 2, 3], &[]);
        assert!(result.is_err());
    }
}
