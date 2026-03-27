use taffy::prelude::*;

use crate::render_tree::{NodeStyle, SafeAreaInsets};

use super::xib::XibConstraint;

/// Simplified Auto Layout constraint solver.
///
/// Supports:
/// - Fixed width/height
/// - Center X/Y in superview
/// - Pin to superview edges (leading/trailing/top/bottom)
/// - Safe area insets
///
/// Does NOT support:
/// - Complex constraint chains (A→B→C)
/// - Priority-based constraint resolution
/// - Full Cassowary algorithm
pub struct SimpleConstraintSolver;

impl SimpleConstraintSolver {
    /// Apply constraints to a node style.
    ///
    /// Converts a set of XIB constraints into Taffy-compatible style properties.
    pub fn apply_constraints(
        constraints: &[XibConstraint],
        node_id: &str,
        parent_id: Option<&str>,
        parent_flex_direction: FlexDirection,
        safe_area_insets: SafeAreaInsets,
        style: &mut NodeStyle,
    ) {
        for constraint in constraints {
            let is_self = constraint
                .first_item
                .as_deref()
                .map(|id| id == node_id)
                .unwrap_or(true);
            let is_to_parent = constraint
                .second_item
                .as_deref()
                .map(|id| parent_id.map(|p| p == id).unwrap_or(false))
                .unwrap_or(true);

            if !is_self && !is_to_parent {
                continue; // Skip constraints between siblings (unsupported)
            }

            match constraint.first_attribute.as_str() {
                // Fixed width
                "width" if constraint.second_item.is_none() => {
                    style.width = Dimension::length(constraint.constant);
                }
                // Fixed height
                "height" if constraint.second_item.is_none() => {
                    style.height = Dimension::length(constraint.constant);
                }
                // Proportional width
                "width" if constraint.second_item.is_some() => {
                    style.width =
                        Dimension::percent(constraint.multiplier);
                }
                // Proportional height
                "height" if constraint.second_item.is_some() => {
                    style.height =
                        Dimension::percent(constraint.multiplier);
                }
                // Center X
                "centerX" => {
                    if matches!(
                        parent_flex_direction,
                        FlexDirection::Column | FlexDirection::ColumnReverse
                    ) {
                        style.align_self = Some(AlignSelf::Center);
                    } else {
                        style.justify_self = Some(JustifySelf::Center);
                    }
                }
                // Center Y
                "centerY" => {
                    if matches!(
                        parent_flex_direction,
                        FlexDirection::Column | FlexDirection::ColumnReverse
                    ) {
                        style.justify_self = Some(JustifySelf::Center);
                    } else {
                        style.align_self = Some(AlignSelf::Center);
                    }
                }
                // Pin to top
                "top" | "topMargin" => {
                    style.margin.top = constraint.constant;
                }
                // Pin to bottom
                "bottom" | "bottomMargin" => {
                    style.margin.bottom = constraint.constant;
                }
                // Pin to leading/left
                "leading" | "leadingMargin" | "left" | "leftMargin" => {
                    style.margin.left = constraint.constant;
                }
                // Pin to trailing/right
                "trailing" | "trailingMargin" | "right" | "rightMargin" => {
                    style.margin.right = constraint.constant;
                }
                // Safe area top
                "topAnchor" => {
                    style.margin.top = constraint.constant.max(safe_area_insets.top.unwrap_or(0.0));
                }
                // Safe area bottom
                "bottomAnchor" => {
                    style.margin.bottom = constraint.constant.max(safe_area_insets.bottom.unwrap_or(0.0));
                }
                _ => {
                    tracing::debug!(
                        "Unsupported constraint attribute: {}",
                        constraint.first_attribute
                    );
                }
            }
        }
    }

    /// Convert UIStackView properties to flexbox style.
    pub fn apply_stack_view_properties(
        properties: &std::collections::HashMap<String, String>,
        style: &mut NodeStyle,
    ) {
        // Axis -> flex_direction
        if let Some(axis) = properties.get("axis") {
            style.flex_direction = match axis.as_str() {
                "horizontal" => FlexDirection::Row,
                _ => FlexDirection::Column,
            };
        }

        // Spacing -> gap
        if let Some(spacing) = properties.get("spacing") {
            if let Ok(val) = spacing.parse::<f32>() {
                style.gap = val;
            }
        }

        // Distribution -> justify_content
        if let Some(distribution) = properties.get("distribution") {
            style.justify_content = match distribution.as_str() {
                "fillEqually" => Some(JustifyContent::SpaceEvenly),
                "fillProportionally" => Some(JustifyContent::Start),
                "equalSpacing" => Some(JustifyContent::SpaceBetween),
                "equalCentering" => Some(JustifyContent::SpaceAround),
                "fill" => Some(JustifyContent::Start),
                _ => None,
            };

            // fillEqually means all children get equal flex
            if distribution == "fillEqually" {
                // Children will need flex_grow = 1.0 (set during inflation)
            }
        }

        // Alignment -> align_items
        if let Some(alignment) = properties.get("alignment") {
            style.align_items = match alignment.as_str() {
                "center" => Some(AlignItems::Center),
                "leading" | "top" => Some(AlignItems::FlexStart),
                "trailing" | "bottom" => Some(AlignItems::FlexEnd),
                "fill" => Some(AlignItems::Stretch),
                _ => None,
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fixed_size_constraints() {
        let constraints = vec![
            XibConstraint {
                id: "c1".to_string(),
                first_item: Some("label1".to_string()),
                first_attribute: "width".to_string(),
                second_item: None,
                second_attribute: None,
                relation: "equal".to_string(),
                constant: 200.0,
                multiplier: 1.0,
            },
            XibConstraint {
                id: "c2".to_string(),
                first_item: Some("label1".to_string()),
                first_attribute: "height".to_string(),
                second_item: None,
                second_attribute: None,
                relation: "equal".to_string(),
                constant: 44.0,
                multiplier: 1.0,
            },
        ];

        let mut style = NodeStyle::default();
        SimpleConstraintSolver::apply_constraints(&constraints, "label1", Some("root"), FlexDirection::Column, SafeAreaInsets::default(), &mut style);

        assert_eq!(style.width, Dimension::length(200.0));
        assert_eq!(style.height, Dimension::length(44.0));
    }

    #[test]
    fn test_center_constraints() {
        let constraints = vec![XibConstraint {
            id: "c1".to_string(),
            first_item: Some("label1".to_string()),
            first_attribute: "centerX".to_string(),
            second_item: Some("root".to_string()),
            second_attribute: Some("centerX".to_string()),
            relation: "equal".to_string(),
            constant: 0.0,
            multiplier: 1.0,
        }];

        let mut style = NodeStyle::default();
        SimpleConstraintSolver::apply_constraints(&constraints, "label1", Some("root"), FlexDirection::Column, SafeAreaInsets::default(), &mut style);

        assert_eq!(style.align_self, Some(AlignSelf::Center));
    }

    #[test]
    fn test_stack_view_horizontal() {
        let mut props = std::collections::HashMap::new();
        props.insert("axis".to_string(), "horizontal".to_string());
        props.insert("spacing".to_string(), "8".to_string());
        props.insert("distribution".to_string(), "fillEqually".to_string());
        props.insert("alignment".to_string(), "center".to_string());

        let mut style = NodeStyle::default();
        SimpleConstraintSolver::apply_stack_view_properties(&props, &mut style);

        assert_eq!(style.flex_direction, FlexDirection::Row);
        assert_eq!(style.gap, 8.0);
        assert_eq!(style.justify_content, Some(JustifyContent::SpaceEvenly));
        assert_eq!(style.align_items, Some(AlignItems::Center));
    }
}
