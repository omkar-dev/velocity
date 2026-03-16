use velocity_common::types::{Element, Selector};

/// Generate the best selector for an element, prioritizing uniqueness and stability.
/// Priority: AccessibilityId > Id > Text (exact) > TextContains > ClassName
pub fn generate_selector(element: &Element) -> Selector {
    // Prefer accessibility label (most stable across UI changes)
    if let Some(ref label) = element.label {
        if !label.is_empty() {
            return Selector::AccessibilityId(label.clone());
        }
    }

    // Then platform ID (resource-id on Android, accessibility identifier on iOS)
    if !element.platform_id.is_empty() {
        return Selector::Id(element.platform_id.clone());
    }

    // Then exact text match
    if let Some(ref text) = element.text {
        if !text.is_empty() {
            return Selector::Text(text.clone());
        }
    }

    // Fallback to class name
    Selector::ClassName(element.element_type.clone())
}
