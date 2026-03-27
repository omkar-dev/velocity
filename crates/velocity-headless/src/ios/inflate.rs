use taffy::FlexDirection;

use crate::render_tree::{Color, NodeStyle, RenderNode, SafeAreaInsets};

use super::constraints::SimpleConstraintSolver;
use super::resources::IosResourceLoader;
use super::xib::{XibDocument, XibFrame, XibObject};

/// Inflates iOS XIB/Storyboard views into RenderNode trees.
pub struct IosInflater {
    resources: IosResourceLoader,
    safe_area_insets: SafeAreaInsets,
}

impl IosInflater {
    pub fn new(resources: IosResourceLoader) -> Self {
        Self {
            resources,
            safe_area_insets: SafeAreaInsets::default(),
        }
    }

    pub fn with_safe_area_insets(mut self, safe_area_insets: SafeAreaInsets) -> Self {
        self.safe_area_insets = safe_area_insets;
        self
    }

    /// Inflate a XIB document into a RenderNode tree.
    pub fn inflate(&self, doc: &XibDocument) -> Result<RenderNode, IosInflateError> {
        let root_obj = doc
            .objects
            .first()
            .ok_or(IosInflateError::NoViews)?;
        Ok(self.inflate_object(root_obj, None, FlexDirection::Column))
    }

    /// Inflate a single XIB object recursively.
    fn inflate_object(
        &self,
        obj: &XibObject,
        parent_id: Option<&str>,
        parent_flex_direction: FlexDirection,
    ) -> RenderNode {
        let view_name = &obj.class;
        let mut style = self.map_uikit_style(obj);

        // Apply constraints
        SimpleConstraintSolver::apply_constraints(
            &obj.constraints,
            &obj.id,
            parent_id,
            parent_flex_direction,
            self.safe_area_insets,
            &mut style,
        );

        // Apply frame-based layout if present and no explicit size
        if let Some(frame) = &obj.frame {
            if style.width.is_auto() {
                style.width = taffy::Dimension::length(frame.width);
            }
            if style.height.is_auto() {
                style.height = taffy::Dimension::length(frame.height);
            }
        }

        // Apply UIStackView properties
        if view_name == "UIStackView" {
            SimpleConstraintSolver::apply_stack_view_properties(&obj.properties, &mut style);
        }

        // Extract text content
        let text = obj
            .properties
            .get("text")
            .or_else(|| obj.properties.get("title"))
            .or_else(|| obj.properties.get("placeholder"))
            .cloned();

        let mut node = if text.is_some() {
            RenderNode::text_node(text.clone().unwrap(), view_name)
        } else {
            RenderNode::container(view_name)
        };

        // Set properties
        node.id = obj
            .properties
            .get("accessibilityIdentifier")
            .cloned()
            .or_else(|| {
                if !obj.id.is_empty() {
                    Some(obj.id.clone())
                } else {
                    None
                }
            });
        node.label = obj
            .properties
            .get("accessibilityLabel")
            .cloned()
            .or(text.clone());
        node.style = style;
        node.enabled = obj
            .properties
            .get("userInteractionEnabled")
            .map(|v| v != "NO")
            .unwrap_or(true);

        // Inflate subviews
        let is_fill_equally = obj
            .properties
            .get("distribution")
            .map(|d| d == "fillEqually")
            .unwrap_or(false);

        for subview in &obj.subviews {
            let mut child = self.inflate_object(subview, Some(&obj.id), node.style.flex_direction);

            // For fillEqually stack views, set flex_grow on children
            if is_fill_equally {
                child.style.flex_grow = 1.0;
            }

            node.children.push(child);
        }

        node
    }

    /// Map UIKit view properties to NodeStyle.
    fn map_uikit_style(&self, obj: &XibObject) -> NodeStyle {
        let mut style = NodeStyle::default();

        // Hidden property
        if let Some(hidden) = obj.properties.get("hidden") {
            style.visible = hidden != "YES";
        }

        // Alpha
        if let Some(alpha) = obj.properties.get("alpha") {
            if let Ok(a) = alpha.parse::<f32>() {
                style.opacity = a;
                if a == 0.0 {
                    style.visible = false;
                }
            }
        }

        // Background color
        if let Some(hex) = obj.properties.get("color_backgroundColor") {
            if let Some(argb) = crate::android::resources::ResourceTable::parse_color(hex) {
                style.background_color = Color::from_argb(argb);
            }
        }

        // Text color
        if let Some(hex) = obj.properties.get("color_textColor") {
            if let Some(argb) = crate::android::resources::ResourceTable::parse_color(hex) {
                style.text_color = Color::from_argb(argb);
            }
        }

        // Font size
        if let Some(size) = obj.properties.get("fontSize") {
            if let Ok(s) = size.parse::<f32>() {
                style.font_size = s;
            }
        }

        // Number of lines (affects text measurement constraints)
        if let Some(lines) = obj.properties.get("numberOfLines") {
            if lines == "1" {
                // Single line: no wrapping
            }
        }

        // Content mode (for images)
        if let Some(mode) = obj.properties.get("contentMode") {
            match mode.as_str() {
                "scaleAspectFit" | "scaleAspectFill" => {
                    // Image scaling modes — keep aspect ratio
                }
                "scaleToFill" => {
                    // Stretch to fill
                }
                _ => {}
            }
        }

        style
    }
}

#[derive(Debug, thiserror::Error)]
pub enum IosInflateError {
    #[error("No views found in XIB document")]
    NoViews,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ios::xib::{XibDocument, XibFrame, XibObject};
    use std::collections::HashMap;

    fn make_label(id: &str, text: &str) -> XibObject {
        let mut props = HashMap::new();
        props.insert("text".to_string(), text.to_string());

        XibObject {
            class: "UILabel".to_string(),
            id: id.to_string(),
            properties: props,
            subviews: Vec::new(),
            constraints: Vec::new(),
            frame: Some(XibFrame {
                x: 20.0,
                y: 100.0,
                width: 335.0,
                height: 21.0,
            }),
        }
    }

    #[test]
    fn test_inflate_simple_hierarchy() {
        let doc = XibDocument {
            objects: vec![XibObject {
                class: "UIView".to_string(),
                id: "root".to_string(),
                properties: HashMap::new(),
                subviews: vec![
                    make_label("title", "Hello"),
                    make_label("subtitle", "World"),
                ],
                constraints: Vec::new(),
                frame: Some(XibFrame {
                    x: 0.0,
                    y: 0.0,
                    width: 375.0,
                    height: 812.0,
                }),
            }],
            connections: Vec::new(),
        };

        let inflater = IosInflater::new(IosResourceLoader::empty());
        let root = inflater.inflate(&doc).unwrap();

        assert_eq!(root.node_type, "UIView");
        assert_eq!(root.children.len(), 2);
        assert_eq!(root.children[0].text.as_deref(), Some("Hello"));
        assert_eq!(root.children[1].text.as_deref(), Some("World"));
    }

    #[test]
    fn test_inflate_stack_view() {
        let mut stack_props = HashMap::new();
        stack_props.insert("axis".to_string(), "horizontal".to_string());
        stack_props.insert("spacing".to_string(), "8".to_string());
        stack_props.insert("distribution".to_string(), "fillEqually".to_string());

        let doc = XibDocument {
            objects: vec![XibObject {
                class: "UIStackView".to_string(),
                id: "stack".to_string(),
                properties: stack_props,
                subviews: vec![
                    make_label("l1", "Left"),
                    make_label("l2", "Right"),
                ],
                constraints: Vec::new(),
                frame: None,
            }],
            connections: Vec::new(),
        };

        let inflater = IosInflater::new(IosResourceLoader::empty());
        let root = inflater.inflate(&doc).unwrap();

        assert_eq!(root.node_type, "UIStackView");
        assert_eq!(root.style.flex_direction, taffy::FlexDirection::Row);
        assert_eq!(root.style.gap, 8.0);
        // Children should have flex_grow from fillEqually
        assert_eq!(root.children[0].style.flex_grow, 1.0);
        assert_eq!(root.children[1].style.flex_grow, 1.0);
    }
}
