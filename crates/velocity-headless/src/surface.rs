use std::sync::Arc;

use tiny_skia::{Color, FillRule, Paint, PathBuilder, Pixmap, Rect, Transform};

use crate::render_tree::{self, ComputedLayout, RenderNode};
use crate::text::TextMeasurer;

/// CPU-only software rendering surface using tiny-skia.
///
/// Renders a RenderTree to an RGBA pixel buffer and encodes to PNG.
/// Deterministic: no GPU, no threading, no system-dependent state.
pub struct SoftwareSurface {
    pixmap: Pixmap,
}

impl SoftwareSurface {
    /// Create a new surface with the given dimensions.
    pub fn new(width: u32, height: u32) -> Result<Self, SurfaceError> {
        let pixmap = Pixmap::new(width, height).ok_or(SurfaceError::CreationFailed {
            width,
            height,
        })?;
        Ok(Self { pixmap })
    }

    /// Get surface dimensions.
    pub fn size(&self) -> (u32, u32) {
        (self.pixmap.width(), self.pixmap.height())
    }

    /// Clear the surface to a solid color.
    pub fn clear(&mut self, color: render_tree::Color) {
        self.pixmap.fill(Color::from_rgba8(color.r, color.g, color.b, color.a));
    }

    /// Render a complete render tree to the surface.
    pub fn render_tree(&mut self, root: &RenderNode, text_measurer: Option<&Arc<TextMeasurer>>) {
        // Clear to white background
        self.pixmap.fill(Color::from_rgba8(255, 255, 255, 255));
        self.render_node(root, text_measurer);
    }

    /// Render a single node and its children recursively.
    fn render_node(&mut self, node: &RenderNode, text_measurer: Option<&Arc<TextMeasurer>>) {
        if !node.style.visible {
            return;
        }

        let layout = match node.layout {
            Some(l) => l,
            None => return,
        };

        // Draw background if non-transparent
        let bg = &node.style.background_color;
        if bg.a > 0 {
            self.draw_rect(layout, bg);
        }

        // Draw text if present
        if let Some(text) = &node.text {
            if let Some(measurer) = text_measurer {
                self.draw_text_glyphs(text, layout, &node.style, measurer);
            } else {
                self.draw_text_fallback(text, layout, &node.style);
            }
        }

        // Draw children
        for child in &node.children {
            self.render_node(child, text_measurer);
        }
    }

    /// Draw a filled rectangle.
    fn draw_rect(&mut self, layout: ComputedLayout, color: &render_tree::Color) {
        let rect = match Rect::from_xywh(layout.x, layout.y, layout.width, layout.height) {
            Some(r) => r,
            None => return,
        };

        let mut paint = Paint::default();
        paint.set_color_rgba8(color.r, color.g, color.b, color.a);
        paint.anti_alias = true;

        // Draw rounded rect if border_radius > 0, otherwise plain rect
        let path = {
            let mut pb = PathBuilder::new();
            pb.push_rect(rect);
            match pb.finish() {
                Some(p) => p,
                None => return,
            }
        };

        self.pixmap.fill_path(
            &path,
            &paint,
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }

    /// Draw text using real glyph rasterization via cosmic-text + swash.
    fn draw_text_glyphs(
        &mut self,
        text: &str,
        layout: ComputedLayout,
        style: &render_tree::NodeStyle,
        text_measurer: &TextMeasurer,
    ) {
        if text.is_empty() {
            return;
        }

        let max_width = Some(layout.width - style.padding.left - style.padding.right);
        let origin_x = layout.x + style.padding.left;
        let origin_y = layout.y + style.padding.top;

        let glyphs = text_measurer.render_glyphs(text, style.font_size, max_width, origin_x, origin_y);

        let text_color = &style.text_color;
        let surface_w = self.pixmap.width() as i32;
        let surface_h = self.pixmap.height() as i32;
        let pixels = self.pixmap.data_mut();

        for glyph in &glyphs {
            for gy in 0..glyph.height as i32 {
                for gx in 0..glyph.width as i32 {
                    let px = glyph.x + gx;
                    let py = glyph.y + gy;

                    // Clip to surface bounds
                    if px < 0 || py < 0 || px >= surface_w || py >= surface_h {
                        continue;
                    }

                    let alpha_idx = (gy as usize) * (glyph.width as usize) + (gx as usize);
                    let alpha = glyph.alpha[alpha_idx];
                    if alpha == 0 {
                        continue;
                    }

                    let pixel_idx = ((py * surface_w + px) * 4) as usize;
                    if pixel_idx + 3 >= pixels.len() {
                        continue;
                    }

                    // Alpha-blend text color onto existing pixel
                    let a = alpha as f32 / 255.0;
                    let inv_a = 1.0 - a;

                    pixels[pixel_idx] = (text_color.r as f32 * a + pixels[pixel_idx] as f32 * inv_a) as u8;
                    pixels[pixel_idx + 1] = (text_color.g as f32 * a + pixels[pixel_idx + 1] as f32 * inv_a) as u8;
                    pixels[pixel_idx + 2] = (text_color.b as f32 * a + pixels[pixel_idx + 2] as f32 * inv_a) as u8;
                    pixels[pixel_idx + 3] = (alpha.max(pixels[pixel_idx + 3])) as u8;
                }
            }
        }
    }

    /// Fallback: draw text as colored block (used when no TextMeasurer available).
    fn draw_text_fallback(&mut self, text: &str, layout: ComputedLayout, style: &render_tree::NodeStyle) {
        if text.is_empty() {
            return;
        }

        let text_color = &style.text_color;
        let mut paint = Paint::default();
        paint.set_color_rgba8(text_color.r, text_color.g, text_color.b, text_color.a);
        paint.anti_alias = true;

        // Render text as a thin horizontal bar to indicate text presence
        let text_height = style.font_size.min(layout.height);
        let y_offset = (layout.height - text_height) / 2.0;

        // Use character count to determine text block width
        let char_width = style.font_size * 0.6;
        let char_count = text.chars().count() as f32;
        let text_width = (char_count * char_width).min(layout.width);

        let rect = match Rect::from_xywh(
            layout.x + style.padding.left,
            layout.y + y_offset,
            text_width,
            text_height,
        ) {
            Some(r) => r,
            None => return,
        };

        let path = {
            let mut pb = PathBuilder::new();
            pb.push_rect(rect);
            match pb.finish() {
                Some(p) => p,
                None => return,
            }
        };

        self.pixmap.fill_path(
            &path,
            &paint,
            FillRule::Winding,
            Transform::identity(),
            None,
        );
    }

    /// Encode the current surface to PNG bytes.
    pub fn encode_png(&self) -> Result<Vec<u8>, SurfaceError> {
        self.pixmap
            .encode_png()
            .map_err(|e| SurfaceError::EncodingFailed(e.to_string()))
    }

    /// Get raw RGBA pixel data.
    pub fn pixels(&self) -> &[u8] {
        self.pixmap.data()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SurfaceError {
    #[error("Failed to create surface with dimensions {width}x{height}")]
    CreationFailed { width: u32, height: u32 },
    #[error("PNG encoding failed: {0}")]
    EncodingFailed(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render_tree::{Color as RColor, NodeStyle};

    #[test]
    fn test_create_surface() {
        let surface = SoftwareSurface::new(1080, 1920).unwrap();
        assert_eq!(surface.size(), (1080, 1920));
    }

    #[test]
    fn test_encode_png() {
        let mut surface = SoftwareSurface::new(100, 100).unwrap();
        surface.clear(RColor::WHITE);
        let png = surface.encode_png().unwrap();
        // PNG magic bytes
        assert_eq!(&png[0..4], &[0x89, b'P', b'N', b'G']);
        assert!(png.len() > 8);
    }

    #[test]
    fn test_render_colored_rect() {
        let mut surface = SoftwareSurface::new(100, 100).unwrap();

        let mut node = RenderNode::container("View");
        node.style.background_color = RColor::from_rgb(255, 0, 0);
        node.layout = Some(ComputedLayout {
            x: 10.0,
            y: 10.0,
            width: 50.0,
            height: 50.0,
        });

        surface.render_tree(&node, None);
        let png = surface.encode_png().unwrap();
        assert!(!png.is_empty());

        // Verify the red pixel is present in the buffer
        let pixels = surface.pixels();
        // Check pixel at (35, 35) — center of the 50x50 rect at (10,10)
        let idx = ((35 * 100 + 35) * 4) as usize;
        assert_eq!(pixels[idx], 255, "red channel");
        assert_eq!(pixels[idx + 1], 0, "green channel");
        assert_eq!(pixels[idx + 2], 0, "blue channel");
    }

    #[test]
    fn test_glyph_rendering_produces_varied_pixels() {
        let text_measurer = Arc::new(TextMeasurer::new());
        let mut surface = SoftwareSurface::new(200, 50).unwrap();

        let mut node = RenderNode::container("TextView");
        node.text = Some("Hello World".to_string());
        node.style.text_color = RColor::from_rgb(0, 0, 0);
        node.style.font_size = 16.0;
        node.layout = Some(ComputedLayout {
            x: 0.0,
            y: 0.0,
            width: 200.0,
            height: 50.0,
        });

        surface.render_tree(&node, Some(&text_measurer));
        let pixels = surface.pixels();

        // Count non-white pixels — glyph rendering should produce anti-aliased edges
        // with varied alpha values, not just solid blocks
        let mut non_white = 0u32;
        let mut unique_values = std::collections::HashSet::new();
        for chunk in pixels.chunks(4) {
            let r = chunk[0];
            let g = chunk[1];
            let b = chunk[2];
            if r != 255 || g != 255 || b != 255 {
                non_white += 1;
                unique_values.insert((r, g, b));
            }
        }

        assert!(non_white > 10, "should have rendered text pixels, got {non_white}");
        // Real glyph rendering produces many unique gray values from anti-aliasing
        assert!(unique_values.len() > 3, "expected varied pixel values from anti-aliasing, got {} unique", unique_values.len());
    }
}
