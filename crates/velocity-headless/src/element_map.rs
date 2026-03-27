use velocity_common::{Element, Rect};

use crate::render_tree::{ComputedLayout, RenderNode};

/// Convert a render tree to a velocity-common Element tree.
///
/// Maps RenderNode properties to Element fields:
/// - RenderNode.id → Element.platform_id (prefixed with "headless::")
/// - RenderNode.label → Element.label
/// - RenderNode.text → Element.text
/// - RenderNode.node_type → Element.element_type
/// - RenderNode.layout → Element.bounds
/// - RenderNode.style.visible → Element.visible
/// - RenderNode.enabled → Element.enabled
pub fn render_tree_to_element(node: &RenderNode) -> Element {
    render_node_to_element(node, 0, &mut 0)
}

fn render_node_to_element(node: &RenderNode, depth: usize, counter: &mut usize) -> Element {
    let layout = node.layout.unwrap_or_default();

    // Generate platform_id: use node id if available, otherwise use counter
    let platform_id = match &node.id {
        Some(id) => format!("headless::{}", id),
        None => {
            *counter += 1;
            format!("headless::node_{}", counter)
        }
    };

    let bounds = layout_to_rect(&layout);
    let visible = node.style.visible && bounds.width > 0 && bounds.height > 0;

    let children: Vec<Element> = node
        .children
        .iter()
        .map(|child| render_node_to_element(child, depth + 1, counter))
        .collect();

    Element {
        platform_id,
        label: node.label.clone(),
        text: node.text.clone(),
        element_type: node.node_type.clone(),
        bounds,
        enabled: node.enabled,
        visible,
        children,
    }
}

/// Convert a ComputedLayout to a velocity-common Rect (f32 → i32).
fn layout_to_rect(layout: &ComputedLayout) -> Rect {
    Rect {
        x: layout.x.round() as i32,
        y: layout.y.round() as i32,
        width: layout.width.round() as i32,
        height: layout.height.round() as i32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render_tree::{ComputedLayout, NodeStyle, RenderNode};
    use taffy::prelude::*;

    #[test]
    fn test_simple_conversion() {
        let root = RenderNode {
            id: Some("root".to_string()),
            label: Some("Root View".to_string()),
            text: None,
            node_type: "FrameLayout".to_string(),
            style: NodeStyle::default(),
            layout: Some(ComputedLayout {
                x: 0.0,
                y: 0.0,
                width: 1080.0,
                height: 1920.0,
            }),
            taffy_id: None,
            enabled: true,
            children: vec![RenderNode {
                id: Some("greeting".to_string()),
                label: Some("Greeting Label".to_string()),
                text: Some("Hello World".to_string()),
                node_type: "TextView".to_string(),
                style: NodeStyle::default(),
                layout: Some(ComputedLayout {
                    x: 100.0,
                    y: 200.0,
                    width: 400.0,
                    height: 50.0,
                }),
                taffy_id: None,
                enabled: true,
                children: Vec::new(),
            }],
        };

        let element = render_tree_to_element(&root);

        assert_eq!(element.platform_id, "headless::root");
        assert_eq!(element.label.as_deref(), Some("Root View"));
        assert_eq!(element.element_type, "FrameLayout");
        assert_eq!(element.bounds.width, 1080);
        assert_eq!(element.bounds.height, 1920);
        assert!(element.visible);
        assert_eq!(element.children.len(), 1);

        let child = &element.children[0];
        assert_eq!(child.platform_id, "headless::greeting");
        assert_eq!(child.text.as_deref(), Some("Hello World"));
        assert_eq!(child.bounds.x, 100);
        assert_eq!(child.bounds.y, 200);
    }

    #[test]
    fn test_invisible_node() {
        let node = RenderNode {
            id: Some("hidden".to_string()),
            label: None,
            text: None,
            node_type: "View".to_string(),
            style: NodeStyle {
                visible: false,
                ..Default::default()
            },
            layout: Some(ComputedLayout {
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 100.0,
            }),
            taffy_id: None,
            enabled: true,
            children: Vec::new(),
        };

        let element = render_tree_to_element(&node);
        assert!(!element.visible);
    }

    #[test]
    fn test_auto_generated_ids() {
        let root = RenderNode::container("View")
            .with_child(RenderNode::container("View"))
            .with_child(RenderNode::container("View"));

        // Set layout for all nodes
        let mut root = root;
        root.layout = Some(ComputedLayout {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 100.0,
        });
        for child in &mut root.children {
            child.layout = Some(ComputedLayout::default());
        }

        let element = render_tree_to_element(&root);

        // Root has no id, so gets auto-generated
        assert!(element.platform_id.starts_with("headless::node_"));
        // Children get unique auto-generated ids
        assert_ne!(
            element.children[0].platform_id,
            element.children[1].platform_id
        );
    }
}
