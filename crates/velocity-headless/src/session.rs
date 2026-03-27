use std::sync::Arc;

use velocity_common::Element;

use crate::config::HeadlessConfig;
use crate::element_map::render_tree_to_element;
use crate::layout::LayoutEngine;
use crate::render_tree::RenderNode;
use crate::surface::SoftwareSurface;
use crate::text::TextMeasurer;

/// Per-device headless session state.
///
/// Holds the current render tree, surface, and layout engine
/// for a single virtual device.
pub struct HeadlessSession {
    /// Session configuration.
    pub config: HeadlessConfig,
    /// Current render tree (set after layout inflation).
    pub render_tree: Option<RenderNode>,
    /// Current element tree (derived from render tree after layout).
    pub element_tree: Option<Element>,
    /// Pixel surface for rendering.
    pub surface: SoftwareSurface,
    /// Layout engine instance.
    pub layout_engine: LayoutEngine,
    /// Shared text measurer for glyph rendering.
    pub text_measurer: Arc<TextMeasurer>,
    /// Installed app path.
    pub app_path: Option<String>,
    /// Whether the session has performed initial layout.
    pub layout_computed: bool,
}

// SAFETY: All non-Send/Sync fields (RenderNode, LayoutEngine) have their own unsafe Send/Sync
// impls because taffy 0.9's CompactLength uses `*const ()` as tagged pointers for float values,
// not real heap pointers.
unsafe impl Send for HeadlessSession {}
unsafe impl Sync for HeadlessSession {}

impl HeadlessSession {
    /// Create a new session with the given config and shared text measurer.
    pub fn new(config: HeadlessConfig, text_measurer: Arc<TextMeasurer>) -> Result<Self, SessionError> {
        let surface = SoftwareSurface::new(config.width, config.height)
            .map_err(|e| SessionError::SurfaceCreation(e.to_string()))?;

        Ok(Self {
            config,
            render_tree: None,
            element_tree: None,
            surface,
            layout_engine: LayoutEngine::new(),
            text_measurer,
            app_path: None,
            layout_computed: false,
        })
    }

    /// Set the render tree and compute layout + render.
    pub fn set_render_tree(&mut self, mut tree: RenderNode) -> Result<(), SessionError> {
        // Compute layout
        self.layout_engine
            .compute_layout(
                &mut tree,
                self.config.width as f32,
                self.config.height as f32,
            )
            .map_err(|e| SessionError::LayoutFailed(e.to_string()))?;

        // Render to surface with glyph rendering
        self.surface.render_tree(&tree, Some(&self.text_measurer));

        // Convert to Element tree
        self.element_tree = Some(render_tree_to_element(&tree));

        // Store render tree
        self.render_tree = Some(tree);
        self.layout_computed = true;

        Ok(())
    }

    /// Get the current element hierarchy.
    pub fn get_hierarchy(&self) -> Option<&Element> {
        self.element_tree.as_ref()
    }

    /// Get a screenshot as PNG bytes.
    pub fn screenshot(&self) -> Result<Vec<u8>, SessionError> {
        self.surface
            .encode_png()
            .map_err(|e| SessionError::ScreenshotFailed(e.to_string()))
    }

    /// Get screen dimensions.
    pub fn screen_size(&self) -> (i32, i32) {
        (self.config.width as i32, self.config.height as i32)
    }

    /// Update text content in the render tree and re-render.
    pub fn update_text(
        &mut self,
        element_id: &str,
        new_text: &str,
    ) -> Result<(), SessionError> {
        let tree = self
            .render_tree
            .as_mut()
            .ok_or(SessionError::NoLayout)?;

        // Find and update the node
        if !Self::update_node_text(tree, element_id, new_text) {
            return Err(SessionError::ElementNotFound(element_id.to_string()));
        }

        // Re-layout and re-render
        let mut tree = self.render_tree.take().unwrap();
        self.set_render_tree(tree)?;

        Ok(())
    }

    fn update_node_text(node: &mut RenderNode, target_id: &str, new_text: &str) -> bool {
        if node.id.as_deref() == Some(target_id) {
            node.text = Some(new_text.to_string());
            return true;
        }
        for child in &mut node.children {
            if Self::update_node_text(child, target_id, new_text) {
                return true;
            }
        }
        false
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("Failed to create rendering surface: {0}")]
    SurfaceCreation(String),
    #[error("Layout computation failed: {0}")]
    LayoutFailed(String),
    #[error("Screenshot encoding failed: {0}")]
    ScreenshotFailed(String),
    #[error("No layout has been loaded")]
    NoLayout,
    #[error("Element not found: {0}")]
    ElementNotFound(String),
}
