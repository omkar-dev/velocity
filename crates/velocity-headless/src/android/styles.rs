use taffy::prelude::*;

use crate::render_tree::{Color, EdgeSizes, NodeStyle, Position};

use super::resources::ResourceTable;

/// Maps Android XML attributes to NodeStyle properties.
pub struct AndroidStyleMapper;

impl AndroidStyleMapper {
    /// Convert Android view attributes to NodeStyle.
    pub fn map_attributes(
        attrs: &std::collections::HashMap<String, String>,
        resources: &ResourceTable,
        parent_orientation: Option<FlexDirection>,
    ) -> NodeStyle {
        let mut style = NodeStyle::default();

        // Layout width
        if let Some(val) = attrs.get("layout_width") {
            style.width = Self::parse_layout_dimension(val, resources);
        }

        // Layout height
        if let Some(val) = attrs.get("layout_height") {
            style.height = Self::parse_layout_dimension(val, resources);
        }

        // Orientation (for LinearLayout)
        if let Some(val) = attrs.get("orientation") {
            style.flex_direction = match val.as_str() {
                "horizontal" | "0" => FlexDirection::Row,
                _ => FlexDirection::Column, // default vertical
            };
        }

        // Gravity / layout_gravity
        if let Some(val) = attrs.get("gravity") {
            Self::apply_gravity(val, &mut style);
        }

        // Layout weight -> flex_grow
        if let Some(val) = attrs.get("layout_weight") {
            if let Ok(weight) = val.parse::<f32>() {
                style.flex_grow = weight;
                // When using weight, the basis dimension should be 0
                if matches!(
                    parent_orientation.unwrap_or(FlexDirection::Column),
                    FlexDirection::Row | FlexDirection::RowReverse
                ) {
                    style.width = Dimension::length(0.0);
                } else {
                    style.height = Dimension::length(0.0);
                }
            }
        }

        // Padding
        if let Some(val) = attrs.get("padding") {
            if let Some(px) = ResourceTable::parse_dimension(val) {
                style.padding = EdgeSizes::uniform(px);
            }
        }
        if let Some(val) = attrs.get("paddingTop") {
            if let Some(px) = ResourceTable::parse_dimension(val) {
                style.padding.top = px;
            }
        }
        if let Some(val) = attrs.get("paddingBottom") {
            if let Some(px) = ResourceTable::parse_dimension(val) {
                style.padding.bottom = px;
            }
        }
        if let Some(val) = attrs.get("paddingLeft").or(attrs.get("paddingStart")) {
            if let Some(px) = ResourceTable::parse_dimension(val) {
                style.padding.left = px;
            }
        }
        if let Some(val) = attrs.get("paddingRight").or(attrs.get("paddingEnd")) {
            if let Some(px) = ResourceTable::parse_dimension(val) {
                style.padding.right = px;
            }
        }

        // Margin
        if let Some(val) = attrs.get("layout_margin") {
            if let Some(px) = ResourceTable::parse_dimension(val) {
                style.margin = EdgeSizes::uniform(px);
            }
        }
        if let Some(val) = attrs.get("layout_marginTop") {
            if let Some(px) = ResourceTable::parse_dimension(val) {
                style.margin.top = px;
            }
        }
        if let Some(val) = attrs.get("layout_marginBottom") {
            if let Some(px) = ResourceTable::parse_dimension(val) {
                style.margin.bottom = px;
            }
        }
        if let Some(val) = attrs.get("layout_marginLeft").or(attrs.get("layout_marginStart")) {
            if let Some(px) = ResourceTable::parse_dimension(val) {
                style.margin.left = px;
            }
        }
        if let Some(val) = attrs.get("layout_marginRight").or(attrs.get("layout_marginEnd")) {
            if let Some(px) = ResourceTable::parse_dimension(val) {
                style.margin.right = px;
            }
        }

        // Text properties
        if let Some(val) = attrs.get("textSize") {
            if let Some(px) = ResourceTable::parse_dimension(val) {
                style.font_size = px;
            }
        }

        if let Some(val) = attrs.get("textColor") {
            let resolved = resources.resolve(val);
            if let Some(argb) = ResourceTable::parse_color(&resolved) {
                style.text_color = Color::from_argb(argb);
            }
        }

        // Background color
        if let Some(val) = attrs.get("background") {
            let resolved = resources.resolve(val);
            if let Some(argb) = ResourceTable::parse_color(&resolved) {
                style.background_color = Color::from_argb(argb);
            }
        }

        // Visibility
        if let Some(val) = attrs.get("visibility") {
            style.visible = match val.as_str() {
                "gone" | "2" | "invisible" | "1" => false,
                _ => true,
            };
        }

        // Alpha/opacity
        if let Some(val) = attrs.get("alpha") {
            if let Ok(alpha) = val.parse::<f32>() {
                style.opacity = alpha;
            }
        }

        style
    }

    /// Parse layout_width/layout_height values.
    fn parse_layout_dimension(value: &str, resources: &ResourceTable) -> Dimension {
        match value {
            "match_parent" | "fill_parent" | "-1" => Dimension::percent(1.0),
            "wrap_content" | "-2" => Dimension::auto(),
            _ => {
                let resolved = resources.resolve(value);
                ResourceTable::parse_dimension(&resolved)
                    .map(Dimension::length)
                    .unwrap_or(Dimension::auto())
            }
        }
    }

    /// Apply gravity to justify_content and align_items.
    fn apply_gravity(gravity: &str, style: &mut NodeStyle) {
        let gravity = gravity.to_lowercase();

        let is_row = matches!(style.flex_direction, FlexDirection::Row | FlexDirection::RowReverse);
        let main_start = match style.flex_direction {
            FlexDirection::RowReverse | FlexDirection::ColumnReverse => JustifyContent::FlexEnd,
            _ => JustifyContent::FlexStart,
        };
        let main_end = match style.flex_direction {
            FlexDirection::RowReverse | FlexDirection::ColumnReverse => JustifyContent::FlexStart,
            _ => JustifyContent::FlexEnd,
        };

        let set_horizontal = |style: &mut NodeStyle, align_main: bool, value: &str| {
            if value.contains("center_horizontal") {
                if align_main {
                    style.justify_content = Some(JustifyContent::Center);
                } else {
                    style.align_items = Some(AlignItems::Center);
                }
            }
            if value.contains("left") || value.contains("start") {
                if align_main {
                    style.justify_content = Some(main_start);
                } else {
                    style.align_items = Some(AlignItems::FlexStart);
                }
            }
            if value.contains("right") || value.contains("end") {
                if align_main {
                    style.justify_content = Some(main_end);
                } else {
                    style.align_items = Some(AlignItems::FlexEnd);
                }
            }
        };

        let set_vertical = |style: &mut NodeStyle, align_main: bool, value: &str| {
            if value.contains("center_vertical") {
                if align_main {
                    style.justify_content = Some(JustifyContent::Center);
                } else {
                    style.align_items = Some(AlignItems::Center);
                }
            }
            if value.contains("top") {
                if align_main {
                    style.justify_content = Some(main_start);
                } else {
                    style.align_items = Some(AlignItems::FlexStart);
                }
            }
            if value.contains("bottom") {
                if align_main {
                    style.justify_content = Some(main_end);
                } else {
                    style.align_items = Some(AlignItems::FlexEnd);
                }
            }
        };

        if gravity == "center" {
            style.justify_content = Some(JustifyContent::Center);
            style.align_items = Some(AlignItems::Center);
            return;
        }

        if is_row {
            set_horizontal(style, true, &gravity);
            set_vertical(style, false, &gravity);
        } else {
            set_vertical(style, true, &gravity);
            set_horizontal(style, false, &gravity);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn make_attrs(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn test_match_parent() {
        let attrs = make_attrs(&[("layout_width", "match_parent"), ("layout_height", "100dp")]);
        let resources = ResourceTable::empty();
        let style = AndroidStyleMapper::map_attributes(&attrs, &resources, None);

        assert_eq!(style.width, Dimension::percent(1.0));
        assert_eq!(style.height, Dimension::length(200.0)); // 100dp * 2.0
    }

    #[test]
    fn test_wrap_content() {
        let attrs = make_attrs(&[("layout_width", "wrap_content")]);
        let resources = ResourceTable::empty();
        let style = AndroidStyleMapper::map_attributes(&attrs, &resources, None);

        assert!(style.width.is_auto());
    }

    #[test]
    fn test_orientation() {
        let attrs = make_attrs(&[("orientation", "horizontal")]);
        let resources = ResourceTable::empty();
        let style = AndroidStyleMapper::map_attributes(&attrs, &resources, None);

        assert_eq!(style.flex_direction, FlexDirection::Row);
    }

    #[test]
    fn test_padding_and_margin() {
        let attrs = make_attrs(&[("padding", "8dp"), ("layout_margin", "16dp")]);
        let resources = ResourceTable::empty();
        let style = AndroidStyleMapper::map_attributes(&attrs, &resources, None);

        assert_eq!(style.padding.top, 16.0); // 8dp * 2.0
        assert_eq!(style.margin.left, 32.0); // 16dp * 2.0
    }
}
