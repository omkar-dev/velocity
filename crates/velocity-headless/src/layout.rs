use taffy::prelude::*;

use crate::render_tree::{ComputedLayout, EdgeSizes, NodeStyle, Position, RenderNode};
use crate::text::TextMeasurer;

/// Layout engine that uses Taffy for flexbox computation.
pub struct LayoutEngine {
    taffy: TaffyTree<()>,
    text_measurer: TextMeasurer,
}

// SAFETY: TaffyTree contains taffy 0.9 CompactLength types that use `*const ()` as tagged
// pointers encoding float values — not real heap pointers. They are safe to send/share.
unsafe impl Send for LayoutEngine {}
unsafe impl Sync for LayoutEngine {}

impl LayoutEngine {
    pub fn new() -> Self {
        Self {
            taffy: TaffyTree::new(),
            text_measurer: TextMeasurer::new(),
        }
    }

    /// Compute layout for the entire render tree at the given viewport size.
    pub fn compute_layout(
        &mut self,
        root: &mut RenderNode,
        viewport_width: f32,
        viewport_height: f32,
    ) -> Result<(), LayoutError> {
        // Clear previous layout state
        self.taffy = TaffyTree::new();

        // Build Taffy tree recursively
        let root_id = self.build_taffy_node(root)?;

        // Compute layout
        self.taffy.compute_layout(
            root_id,
            Size {
                width: AvailableSpace::Definite(viewport_width),
                height: AvailableSpace::Definite(viewport_height),
            },
        )?;

        // Write computed positions back to render nodes
        self.apply_layout(root, root_id, 0.0, 0.0)?;

        Ok(())
    }

    /// Recursively build Taffy nodes from the render tree.
    fn build_taffy_node(&mut self, node: &mut RenderNode) -> Result<NodeId, LayoutError> {
        let style = self.convert_style(&node.style);

        if node.is_leaf() {
            // Leaf node: use measure function for text sizing
            let text = node.text.clone().unwrap_or_default();
            let font_size = node.style.font_size;
            let text_measurer = &self.text_measurer;

            let measured_size = text_measurer.measure(&text, font_size, None);

            let leaf_id = self.taffy.new_leaf_with_context(style, ())?;

            // Apply text measurement for auto-sized dimensions
            let needs_width = node.style.width.is_auto();
            let needs_height = node.style.height.is_auto();
            if needs_width || needs_height {
                let mut current_style = self.taffy.style(leaf_id)?.clone();
                if needs_width {
                    current_style.size.width = length(measured_size.0);
                }
                if needs_height {
                    current_style.size.height = length(measured_size.1);
                }
                self.taffy.set_style(leaf_id, current_style)?;
            }

            node.taffy_id = Some(leaf_id);
            Ok(leaf_id)
        } else {
            // Container node: build children first
            let mut child_ids = Vec::with_capacity(node.children.len());
            for child in &mut node.children {
                let child_id = self.build_taffy_node(child)?;
                child_ids.push(child_id);
            }

            let node_id = self.taffy.new_with_children(style, &child_ids)?;
            node.taffy_id = Some(node_id);
            Ok(node_id)
        }
    }

    /// Convert our NodeStyle to Taffy Style.
    fn convert_style(&self, style: &NodeStyle) -> Style {
        Style {
            display: if style.visible { Display::Flex } else { Display::None },
            position: match style.position {
                Position::Relative => taffy::Position::Relative,
                Position::Absolute => taffy::Position::Absolute,
            },
            flex_direction: style.flex_direction,
            flex_wrap: style.flex_wrap,
            flex_grow: style.flex_grow,
            flex_shrink: style.flex_shrink,
            flex_basis: convert_dimension(style.flex_basis),
            justify_content: style.justify_content,
            align_items: style.align_items,
            align_self: style.align_self,
            justify_self: style.justify_self,
            size: Size {
                width: convert_dimension(style.width),
                height: convert_dimension(style.height),
            },
            min_size: Size {
                width: convert_dimension(style.min_width),
                height: convert_dimension(style.min_height),
            },
            max_size: Size {
                width: convert_dimension(style.max_width),
                height: convert_dimension(style.max_height),
            },
            margin: convert_edges_auto(&style.margin),
            padding: convert_edges(&style.padding),
            gap: Size {
                width: length(style.gap),
                height: length(style.gap),
            },
            inset: Rect {
                top: convert_inset(style.inset.top),
                right: convert_inset(style.inset.right),
                bottom: convert_inset(style.inset.bottom),
                left: convert_inset(style.inset.left),
            },
            ..Default::default()
        }
    }

    /// Write computed layout positions back to render nodes (absolute coordinates).
    fn apply_layout(
        &self,
        node: &mut RenderNode,
        taffy_id: NodeId,
        parent_x: f32,
        parent_y: f32,
    ) -> Result<(), LayoutError> {
        let layout = self.taffy.layout(taffy_id)?;

        let abs_x = parent_x + layout.location.x;
        let abs_y = parent_y + layout.location.y;

        node.layout = Some(ComputedLayout {
            x: abs_x,
            y: abs_y,
            width: layout.size.width,
            height: layout.size.height,
        });

        // Apply to children
        let child_ids: Vec<NodeId> = self.taffy.children(taffy_id)?.to_vec();
        for (i, child) in node.children.iter_mut().enumerate() {
            if let Some(&child_taffy_id) = child_ids.get(i) {
                self.apply_layout(child, child_taffy_id, abs_x, abs_y)?;
            }
        }

        Ok(())
    }
}

/// Convert our Dimension to Taffy Dimension.
fn convert_dimension(dim: Dimension) -> taffy::Dimension {
    dim
}

/// Convert EdgeSizes to Taffy Rect<LengthPercentage>.
fn convert_edges(edges: &EdgeSizes) -> Rect<LengthPercentage> {
    Rect {
        top: length(edges.top),
        right: length(edges.right),
        bottom: length(edges.bottom),
        left: length(edges.left),
    }
}

/// Convert EdgeSizes to Taffy Rect<LengthPercentageAuto>.
fn convert_edges_auto(edges: &EdgeSizes) -> Rect<LengthPercentageAuto> {
    Rect {
        top: LengthPercentageAuto::length(edges.top),
        right: LengthPercentageAuto::length(edges.right),
        bottom: LengthPercentageAuto::length(edges.bottom),
        left: LengthPercentageAuto::length(edges.left),
    }
}

fn convert_inset(value: Option<f32>) -> LengthPercentageAuto {
    match value {
        Some(value) => LengthPercentageAuto::length(value),
        None => LengthPercentageAuto::auto(),
    }
}

#[derive(Debug, thiserror::Error)]
pub enum LayoutError {
    #[error("Taffy layout error: {0}")]
    Taffy(#[from] taffy::TaffyError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render_tree::RenderNode;

    #[test]
    fn test_simple_vertical_layout() {
        let mut root = RenderNode::container("LinearLayout")
            .with_style(NodeStyle {
                width: Dimension::length(400.0),
                height: Dimension::length(800.0),
                flex_direction: FlexDirection::Column,
                ..Default::default()
            })
            .with_child(
                RenderNode::text_node("Hello", "TextView")
                    .with_id("text1")
                    .with_style(NodeStyle {
                        width: Dimension::length(400.0),
                        height: Dimension::length(50.0),
                        ..Default::default()
                    }),
            )
            .with_child(
                RenderNode::text_node("World", "TextView")
                    .with_id("text2")
                    .with_style(NodeStyle {
                        width: Dimension::length(400.0),
                        height: Dimension::length(50.0),
                        ..Default::default()
                    }),
            );

        let mut engine = LayoutEngine::new();
        engine.compute_layout(&mut root, 400.0, 800.0).unwrap();

        // Root at (0,0) with full size
        let root_layout = root.layout.unwrap();
        assert_eq!(root_layout.x, 0.0);
        assert_eq!(root_layout.y, 0.0);
        assert_eq!(root_layout.width, 400.0);

        // First child at (0,0)
        let child1 = root.children[0].layout.unwrap();
        assert_eq!(child1.x, 0.0);
        assert_eq!(child1.y, 0.0);
        assert_eq!(child1.height, 50.0);

        // Second child below first at (0,50)
        let child2 = root.children[1].layout.unwrap();
        assert_eq!(child2.x, 0.0);
        assert_eq!(child2.y, 50.0);
        assert_eq!(child2.height, 50.0);
    }

    #[test]
    fn test_horizontal_layout_with_flex_grow() {
        let mut root = RenderNode::container("LinearLayout")
            .with_style(NodeStyle {
                width: Dimension::length(300.0),
                height: Dimension::length(100.0),
                flex_direction: FlexDirection::Row,
                ..Default::default()
            })
            .with_child(RenderNode::container("View").with_style(NodeStyle {
                flex_grow: 1.0,
                height: Dimension::length(100.0),
                ..Default::default()
            }))
            .with_child(RenderNode::container("View").with_style(NodeStyle {
                flex_grow: 2.0,
                height: Dimension::length(100.0),
                ..Default::default()
            }));

        let mut engine = LayoutEngine::new();
        engine.compute_layout(&mut root, 300.0, 100.0).unwrap();

        // flex_grow 1:2 ratio means 100px and 200px
        let child1 = root.children[0].layout.unwrap();
        let child2 = root.children[1].layout.unwrap();
        assert_eq!(child1.width, 100.0);
        assert_eq!(child2.width, 200.0);
        assert_eq!(child2.x, 100.0);
    }
}
