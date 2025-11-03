use velocity_common::{Element, Rect, Selector};

/// Options controlling how selectors are matched.
pub struct MatchOptions {
    pub visible_only: bool,
}

impl Default for MatchOptions {
    fn default() -> Self {
        Self { visible_only: true }
    }
}

/// Check if an element is considered visible.
fn is_visible(element: &Element, screen_bounds: &Rect) -> bool {
    if !element.visible {
        return false;
    }
    if element.bounds.width <= 0 || element.bounds.height <= 0 {
        return false;
    }
    element.bounds.intersects(screen_bounds)
}

/// Match a Selector against an Element, respecting visibility.
fn matches(element: &Element, selector: &Selector, opts: &MatchOptions, screen: &Rect) -> bool {
    if opts.visible_only && !is_visible(element, screen) {
        return false;
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
                .map_or(false, |t| t.contains(substr.as_str()))
                || element
                    .label
                    .as_ref()
                    .map_or(false, |l| l.contains(substr.as_str()))
        }
        Selector::AccessibilityId(aid) => element.label.as_deref() == Some(aid.as_str()),
        Selector::ClassName(cls) => element.element_type == *cls,
        Selector::Index { .. } => false, // resolved at find_elements level
        Selector::Compound(selectors) => {
            selectors.iter().all(|s| matches(element, s, opts, screen))
        }
    }
}

/// Depth-first search for the first matching element.
pub fn find_element<'a>(
    root: &'a Element,
    selector: &Selector,
    opts: &MatchOptions,
    screen: &Rect,
) -> Option<&'a Element> {
    // Handle Index selector: find all matches, return Nth
    if let Selector::Index { selector: inner, index } = selector {
        let all = find_all_elements(root, inner, opts, screen);
        return all.into_iter().nth(*index);
    }

    if matches(root, selector, opts, screen) {
        return Some(root);
    }
    for child in &root.children {
        if let Some(found) = find_element(child, selector, opts, screen) {
            return Some(found);
        }
    }
    None
}

/// Find all matching elements via depth-first traversal.
pub fn find_all_elements<'a>(
    root: &'a Element,
    selector: &Selector,
    opts: &MatchOptions,
    screen: &Rect,
) -> Vec<&'a Element> {
    let mut results = Vec::new();
    find_all_recursive(root, selector, opts, screen, &mut results);
    results
}

fn find_all_recursive<'a>(
    element: &'a Element,
    selector: &Selector,
    opts: &MatchOptions,
    screen: &Rect,
    results: &mut Vec<&'a Element>,
) {
    if matches(element, selector, opts, screen) {
        results.push(element);
    }
    for child in &element.children {
        find_all_recursive(child, selector, opts, screen, results);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use velocity_common::Rect;

    fn make_screen() -> Rect {
        Rect {
            x: 0,
            y: 0,
            width: 1080,
            height: 2400,
        }
    }

    fn make_button() -> Element {
        Element {
            platform_id: "com.app:id/login_button".to_string(),
            label: Some("Login".to_string()),
            text: Some("Log In".to_string()),
            element_type: "Button".to_string(),
            bounds: Rect {
                x: 120,
                y: 640,
                width: 840,
                height: 96,
            },
            enabled: true,
            visible: true,
            children: Vec::new(),
        }
    }

    #[test]
    fn test_find_by_id() {
        let button = make_button();
        let root = Element {
            platform_id: "root".to_string(),
            label: None,
            text: None,
            element_type: "FrameLayout".to_string(),
            bounds: make_screen(),
            enabled: true,
            visible: true,
            children: vec![button],
        };

        let opts = MatchOptions::default();
        let screen = make_screen();
        let selector = Selector::Id("login_button".to_string());
        let found = find_element(&root, &selector, &opts, &screen);
        assert!(found.is_some());
        assert_eq!(found.unwrap().text.as_deref(), Some("Log In"));
    }

    #[test]
    fn test_find_by_text() {
        let button = make_button();
        let root = Element {
            platform_id: "root".to_string(),
            label: None,
            text: None,
            element_type: "FrameLayout".to_string(),
            bounds: make_screen(),
            enabled: true,
            visible: true,
            children: vec![button],
        };

        let opts = MatchOptions::default();
        let screen = make_screen();
        let selector = Selector::Text("Log In".to_string());
        let found = find_element(&root, &selector, &opts, &screen);
        assert!(found.is_some());
    }

    #[test]
    fn test_invisible_element_skipped() {
        let mut button = make_button();
        button.visible = false;

        let root = Element {
            platform_id: "root".to_string(),
            label: None,
            text: None,
            element_type: "FrameLayout".to_string(),
            bounds: make_screen(),
            enabled: true,
            visible: true,
            children: vec![button],
        };

        let opts = MatchOptions { visible_only: true };
        let screen = make_screen();
        let selector = Selector::Id("login_button".to_string());
        let found = find_element(&root, &selector, &opts, &screen);
        assert!(found.is_none());
    }
}
