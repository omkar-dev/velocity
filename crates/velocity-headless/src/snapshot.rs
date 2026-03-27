use std::path::Path;

/// Result of a snapshot comparison.
#[derive(Debug)]
pub struct SnapshotResult {
    /// Whether the snapshots match within threshold.
    pub matches: bool,
    /// Percentage of pixels that differ (0.0 - 1.0).
    pub diff_percentage: f64,
    /// Number of differing pixels.
    pub diff_pixel_count: u64,
    /// Total number of pixels compared.
    pub total_pixels: u64,
    /// Diff image as PNG bytes (if mismatch).
    pub diff_image: Option<Vec<u8>>,
}

/// Compare two PNG images pixel by pixel.
///
/// Returns a SnapshotResult indicating whether they match within the threshold.
pub fn compare_png(actual: &[u8], baseline: &[u8], threshold: f64) -> Result<SnapshotResult, SnapshotError> {
    let actual_img = decode_png(actual)?;
    let baseline_img = decode_png(baseline)?;

    if actual_img.width != baseline_img.width || actual_img.height != baseline_img.height {
        return Ok(SnapshotResult {
            matches: false,
            diff_percentage: 1.0,
            diff_pixel_count: (actual_img.width * actual_img.height) as u64,
            total_pixels: (actual_img.width * actual_img.height) as u64,
            diff_image: None,
        });
    }

    let total_pixels = (actual_img.width * actual_img.height) as u64;
    if total_pixels == 0 {
        return Ok(SnapshotResult {
            matches: true,
            diff_percentage: 0.0,
            diff_pixel_count: 0,
            total_pixels,
            diff_image: None,
        });
    }
    let mut diff_count: u64 = 0;
    let mut diff_pixels = vec![0u8; actual_img.data.len()];

    // Compare pixel by pixel (RGBA)
    for i in (0..actual_img.data.len()).step_by(4) {
        let r_diff = (actual_img.data[i] as i32 - baseline_img.data[i] as i32).unsigned_abs();
        let g_diff = (actual_img.data[i + 1] as i32 - baseline_img.data[i + 1] as i32).unsigned_abs();
        let b_diff = (actual_img.data[i + 2] as i32 - baseline_img.data[i + 2] as i32).unsigned_abs();
        let a_diff = (actual_img.data[i + 3] as i32 - baseline_img.data[i + 3] as i32).unsigned_abs();

        if r_diff > 0 || g_diff > 0 || b_diff > 0 || a_diff > 0 {
            diff_count += 1;
            // Highlight diff in red
            diff_pixels[i] = 255;     // R
            diff_pixels[i + 1] = 0;   // G
            diff_pixels[i + 2] = 0;   // B
            diff_pixels[i + 3] = 255; // A
        } else {
            // Keep original pixel (dimmed)
            diff_pixels[i] = actual_img.data[i] / 3;
            diff_pixels[i + 1] = actual_img.data[i + 1] / 3;
            diff_pixels[i + 2] = actual_img.data[i + 2] / 3;
            diff_pixels[i + 3] = 255;
        }
    }

    let diff_percentage = diff_count as f64 / total_pixels as f64;
    let matches = diff_percentage <= threshold;

    let diff_image = if !matches {
        Some(encode_png(
            &diff_pixels,
            actual_img.width,
            actual_img.height,
        )?)
    } else {
        None
    };

    Ok(SnapshotResult {
        matches,
        diff_percentage,
        diff_pixel_count: diff_count,
        total_pixels,
        diff_image,
    })
}

/// Save a baseline image.
pub fn save_baseline(png_data: &[u8], baseline_dir: &str, test_name: &str) -> Result<(), SnapshotError> {
    let dir = Path::new(baseline_dir);
    std::fs::create_dir_all(dir).map_err(|e| SnapshotError::IoError(e.to_string()))?;

    let path = dir.join(format!("{}.png", test_name));
    std::fs::write(&path, png_data).map_err(|e| SnapshotError::IoError(e.to_string()))?;

    Ok(())
}

/// Load a baseline image.
pub fn load_baseline(baseline_dir: &str, test_name: &str) -> Result<Option<Vec<u8>>, SnapshotError> {
    let path = Path::new(baseline_dir).join(format!("{}.png", test_name));
    if !path.exists() {
        return Ok(None);
    }
    let data = std::fs::read(&path).map_err(|e| SnapshotError::IoError(e.to_string()))?;
    Ok(Some(data))
}

struct DecodedImage {
    width: u32,
    height: u32,
    data: Vec<u8>, // RGBA
}

fn decode_png(data: &[u8]) -> Result<DecodedImage, SnapshotError> {
    let cursor = std::io::Cursor::new(data);
    let decoder = png::Decoder::new(cursor);
    let mut reader = decoder
        .read_info()
        .map_err(|e: png::DecodingError| SnapshotError::PngDecodeError(e.to_string()))?;

    let mut buf = vec![0; reader.output_buffer_size().unwrap_or(0)];
    let info = reader
        .next_frame(&mut buf)
        .map_err(|e| SnapshotError::PngDecodeError(e.to_string()))?;

    // Convert to RGBA if needed
    let rgba_data = match info.color_type {
        png::ColorType::Rgba => buf[..info.buffer_size()].to_vec(),
        png::ColorType::Rgb => {
            let rgb = &buf[..info.buffer_size()];
            let mut rgba = Vec::with_capacity(rgb.len() / 3 * 4);
            for chunk in rgb.chunks(3) {
                rgba.extend_from_slice(chunk);
                rgba.push(255);
            }
            rgba
        }
        png::ColorType::Grayscale => {
            let grayscale = &buf[..info.buffer_size()];
            let mut rgba = Vec::with_capacity(grayscale.len() * 4);
            for gray in grayscale {
                rgba.extend_from_slice(&[*gray, *gray, *gray, 255]);
            }
            rgba
        }
        png::ColorType::GrayscaleAlpha => {
            let grayscale_alpha = &buf[..info.buffer_size()];
            let mut rgba = Vec::with_capacity(grayscale_alpha.len() * 2);
            for chunk in grayscale_alpha.chunks_exact(2) {
                rgba.extend_from_slice(&[chunk[0], chunk[0], chunk[0], chunk[1]]);
            }
            rgba
        }
        _ => {
            return Err(SnapshotError::PngDecodeError(format!(
                "Unsupported PNG color type: {:?}",
                info.color_type
            )));
        }
    };

    Ok(DecodedImage {
        width: info.width,
        height: info.height,
        data: rgba_data,
    })
}

fn encode_png(rgba_data: &[u8], width: u32, height: u32) -> Result<Vec<u8>, SnapshotError> {
    let mut buf = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut buf, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|e| SnapshotError::PngEncodeError(e.to_string()))?;
        writer
            .write_image_data(rgba_data)
            .map_err(|e| SnapshotError::PngEncodeError(e.to_string()))?;
    }
    Ok(buf)
}

#[derive(Debug, thiserror::Error)]
pub enum SnapshotError {
    #[error("I/O error: {0}")]
    IoError(String),
    #[error("PNG decode error: {0}")]
    PngDecodeError(String),
    #[error("PNG encode error: {0}")]
    PngEncodeError(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identical_images_match() {
        // Create a simple 2x2 red PNG
        let png = create_test_png(2, 2, &[255, 0, 0, 255]);
        let result = compare_png(&png, &png, 0.0).unwrap();
        assert!(result.matches);
        assert_eq!(result.diff_percentage, 0.0);
        assert_eq!(result.diff_pixel_count, 0);
    }

    fn create_test_png(width: u32, height: u32, pixel: &[u8; 4]) -> Vec<u8> {
        let mut data = Vec::new();
        for _ in 0..(width * height) {
            data.extend_from_slice(pixel);
        }
        encode_png(&data, width, height).unwrap()
    }
}
