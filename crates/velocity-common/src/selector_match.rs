use crate::types::{Element, Rect, Selector};

/// Check if an element matches a selector.
///
/// When `screen` is `Some`, elements that are not visible, have zero-area bounds,
/// or fall outside the screen rect are rejected before matching. When `screen` is
/// `None` these visibility/bounds checks are skipped (useful for bridge drivers
/// that do not track screen geometry).
pub fn matches_selector(element: &Element, selector: &Selector, screen: Option<&Rect>) -> bool {
    if let Some(screen) = screen {
        if !element.visible || element.bounds.width <= 0 || element.bounds.height <= 0 {
            return false;
        }
        if !element.bounds.intersects(screen) {
            return false;
        }
    }

    match selector {
        Selector::Id(id) => {
            element.platform_id.contains(id.as_str())
                || element.label.as_deref() == Some(id.as_str())
        }
        Selector::Text(text) => {
            element.text.as_deref() == Some(text.as_str())
                || element.label.as_deref() == Some(text.as_str())
        }
        Selector::TextContains(substr) => {
            element
                .text
                .as_ref()
                .is_some_and(|t| t.contains(substr.as_str()))
                || element
                    .label
                    .as_ref()
                    .is_some_and(|l| l.contains(substr.as_str()))
        }
        Selector::AccessibilityId(aid) => {
            element.label.as_deref() == Some(aid.as_str())
                || element.platform_id == *aid
        }
        Selector::ClassName(cls) => element.element_type == *cls,
        Selector::Index { .. } => false, // Handled at find level
        Selector::Compound(selectors) => {
            selectors.iter().all(|s| matches_selector(element, s, screen))
        }
    }
}

/// Find the first element matching `selector` in the tree rooted at `root`.
///
/// When `screen` is `Some`, visibility/bounds filtering is applied.
pub fn find_in_tree(root: &Element, selector: &Selector, screen: Option<&Rect>) -> Option<Element> {
    if let Selector::Index { selector: inner, index } = selector {
        let mut results = Vec::new();
        find_all_in_tree(root, inner, screen, &mut results);
        return results.into_iter().nth(*index);
    }

    if matches_selector(root, selector, screen) {
        return Some(root.clone());
    }
    for child in &root.children {
        if let Some(found) = find_in_tree(child, selector, screen) {
            return Some(found);
        }
    }
    None
}

/// Collect all elements matching `selector` in the tree rooted at `root`.
///
/// When `screen` is `Some`, visibility/bounds filtering is applied.
pub fn find_all_in_tree(
    root: &Element,
    selector: &Selector,
    screen: Option<&Rect>,
    results: &mut Vec<Element>,
) {
    if matches_selector(root, selector, screen) {
        results.push(root.clone());
    }
    for child in &root.children {
        find_all_in_tree(child, selector, screen, results);
    }
}
