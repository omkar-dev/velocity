use serde::{Deserialize, Serialize};
use taffy::prelude::*;

/// A node in the headless render tree.
///
/// Each node represents a view element (container, text, image, or placeholder)
/// with styling properties that map to Taffy flexbox layout.
#[derive(Debug, Clone)]
pub struct RenderNode {
    /// Resource ID or accessibility identifier (e.g., "login_button").
    pub id: Option<String>,
    /// Content description / accessibility label.
    pub label: Option<String>,
    /// Text content (for TextView / UILabel).
    pub text: Option<String>,
    /// View type name (e.g., "TextView", "UILabel", "LinearLayout").
    pub node_type: String,
    /// Layout and visual style properties.
    pub style: NodeStyle,
    /// Computed layout result after Taffy pass.
    pub layout: Option<ComputedLayout>,
    /// Taffy node ID (set during layout computation).
    pub taffy_id: Option<taffy::NodeId>,
    /// Whether this node is enabled (tappable).
    pub enabled: bool,
    /// Child nodes.
    pub children: Vec<RenderNode>,
}

/// Computed absolute layout position and size.
#[derive(Debug, Clone, Copy, Default)]
pub struct ComputedLayout {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// Visual and layout style properties for a render node.
#[derive(Debug, Clone)]
pub struct NodeStyle {
    // Dimensions
    pub width: Dimension,
    pub height: Dimension,
    pub min_width: Dimension,
    pub min_height: Dimension,
    pub max_width: Dimension,
    pub max_height: Dimension,

    // Flexbox
    pub flex_direction: FlexDirection,
    pub flex_wrap: FlexWrap,
    pub flex_grow: f32,
    pub flex_shrink: f32,
    pub flex_basis: Dimension,
    pub justify_content: Option<JustifyContent>,
    pub align_items: Option<AlignItems>,
    pub align_self: Option<AlignSelf>,
    pub justify_self: Option<JustifySelf>,
    pub gap: f32,

    // Spacing
    pub margin: EdgeSizes,
    pub padding: EdgeSizes,

    // Visual
    pub background_color: Color,
    pub text_color: Color,
    pub font_size: f32,
    pub border_radius: f32,

    // Visibility
    pub visible: bool,
    pub opacity: f32,

    // Position
    pub position: Position,
    pub inset: SafeAreaInsets,
}

/// RGBA color.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const TRANSPARENT: Self = Self { r: 0, g: 0, b: 0, a: 0 };
    pub const BLACK: Self = Self { r: 0, g: 0, b: 0, a: 255 };
    pub const WHITE: Self = Self { r: 255, g: 255, b: 255, a: 255 };

    pub fn from_argb(argb: u32) -> Self {
        Self {
            a: ((argb >> 24) & 0xFF) as u8,
            r: ((argb >> 16) & 0xFF) as u8,
            g: ((argb >> 8) & 0xFF) as u8,
            b: (argb & 0xFF) as u8,
        }
    }

    pub fn from_rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }
}

/// Edge sizes for margin, padding, and inset.
#[derive(Debug, Clone, Copy, Default)]
pub struct EdgeSizes {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

/// Absolute inset values. `None` means `auto`; `Some(0.0)` is an explicit zero inset.
#[derive(Debug, Clone, Copy, Default)]
pub struct SafeAreaInsets {
    pub top: Option<f32>,
    pub right: Option<f32>,
    pub bottom: Option<f32>,
    pub left: Option<f32>,
}

impl SafeAreaInsets {
    pub const AUTO: Self = Self {
        top: None,
        right: None,
        bottom: None,
        left: None,
    };

    pub fn uniform(value: f32) -> Self {
        Self {
            top: Some(value),
            right: Some(value),
            bottom: Some(value),
            left: Some(value),
        }
    }
}

impl EdgeSizes {
    pub const ZERO: Self = Self { top: 0.0, right: 0.0, bottom: 0.0, left: 0.0 };

    pub fn uniform(value: f32) -> Self {
        Self { top: value, right: value, bottom: value, left: value }
    }

    pub fn symmetric(horizontal: f32, vertical: f32) -> Self {
        Self { top: vertical, right: horizontal, bottom: vertical, left: horizontal }
    }
}

/// Positioning mode.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub enum Position {
    #[default]
    Relative,
    Absolute,
}

// SAFETY: taffy 0.9's `Dimension` (and related types) contain `*const ()` internally
// as a tagged-pointer representation of compact float values — they are not real pointers
// to heap allocations. The values are purely numeric and safe to send/share across threads.
unsafe impl Send for NodeStyle {}
unsafe impl Sync for NodeStyle {}
unsafe impl Send for RenderNode {}
unsafe impl Sync for RenderNode {}

impl Default for NodeStyle {
    fn default() -> Self {
        Self {
            width: Dimension::auto(),
            height: Dimension::auto(),
            min_width: Dimension::auto(),
            min_height: Dimension::auto(),
            max_width: Dimension::auto(),
            max_height: Dimension::auto(),
            flex_direction: FlexDirection::Column,
            flex_wrap: FlexWrap::NoWrap,
            flex_grow: 0.0,
            flex_shrink: 1.0,
            flex_basis: Dimension::auto(),
            justify_content: None,
            align_items: None,
            align_self: None,
            justify_self: None,
            gap: 0.0,
            margin: EdgeSizes::ZERO,
            padding: EdgeSizes::ZERO,
            background_color: Color::TRANSPARENT,
            text_color: Color::BLACK,
            font_size: 14.0,
            border_radius: 0.0,
            visible: true,
            opacity: 1.0,
            position: Position::Relative,
            inset: SafeAreaInsets::AUTO,
        }
    }
}

impl RenderNode {
    /// Create a new container node.
    pub fn container(node_type: impl Into<String>) -> Self {
        Self {
            id: None,
            label: None,
            text: None,
            node_type: node_type.into(),
            style: NodeStyle::default(),
            layout: None,
            taffy_id: None,
            enabled: true,
            children: Vec::new(),
        }
    }

    /// Create a new text leaf node.
    pub fn text_node(text: impl Into<String>, node_type: impl Into<String>) -> Self {
        Self {
            id: None,
            label: None,
            text: Some(text.into()),
            node_type: node_type.into(),
            style: NodeStyle::default(),
            layout: None,
            taffy_id: None,
            enabled: true,
            children: Vec::new(),
        }
    }

    /// Builder: set ID.
    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// Builder: set label.
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Builder: set style.
    pub fn with_style(mut self, style: NodeStyle) -> Self {
        self.style = style;
        self
    }

    /// Builder: add child.
    pub fn with_child(mut self, child: RenderNode) -> Self {
        self.children.push(child);
        self
    }

    /// Builder: set enabled.
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Returns true if this is a leaf node (text or image with no children).
    pub fn is_leaf(&self) -> bool {
        self.children.is_empty() && self.text.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_container() {
        let node = RenderNode::container("LinearLayout")
            .with_id("root")
            .with_child(RenderNode::text_node("Hello", "TextView").with_id("greeting"));

        assert_eq!(node.node_type, "LinearLayout");
        assert_eq!(node.id.as_deref(), Some("root"));
        assert_eq!(node.children.len(), 1);
        assert_eq!(node.children[0].text.as_deref(), Some("Hello"));
    }

    #[test]
    fn test_color_from_argb() {
        let color = Color::from_argb(0xFF_FF_00_00); // opaque red
        assert_eq!(color.r, 255);
        assert_eq!(color.g, 0);
        assert_eq!(color.b, 0);
        assert_eq!(color.a, 255);
    }

    #[test]
    fn test_default_style() {
        let style = NodeStyle::default();
        assert_eq!(style.flex_direction, FlexDirection::Column);
        assert!(style.visible);
        assert_eq!(style.font_size, 14.0);
    }
}
