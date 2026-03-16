use velocity_common::types::Selector;

/// Generate a YAML test step snippet for a given action and selector.
pub fn tap_yaml(selector: &Selector) -> String {
    format!("- tap:\n    {}", selector_to_yaml(selector))
}

pub fn type_text_yaml(selector: &Selector, text: &str) -> String {
    format!(
        "- inputText:\n    selector:\n      {}\n    text: {:?}",
        selector_to_yaml(selector),
        text
    )
}

pub fn swipe_yaml(direction: &str) -> String {
    format!("- swipe:\n    direction: {direction}")
}

pub fn assert_visible_yaml(selector: &Selector) -> String {
    format!("- assertVisible:\n    {}", selector_to_yaml(selector))
}

fn selector_to_yaml(selector: &Selector) -> String {
    match selector {
        Selector::Id(id) => format!("id: {id:?}"),
        Selector::Text(text) => format!("text: {text:?}"),
        Selector::TextContains(sub) => format!("textContains: {sub:?}"),
        Selector::AccessibilityId(aid) => format!("accessibilityId: {aid:?}"),
        Selector::ClassName(cls) => format!("className: {cls:?}"),
        Selector::Index { selector, index } => {
            format!(
                "index:\n      selector:\n        {}\n      index: {index}",
                selector_to_yaml(selector)
            )
        }
        Selector::Compound(selectors) => {
            let parts: Vec<String> = selectors.iter().map(selector_to_yaml).collect();
            format!("compound:\n      {}", parts.join("\n      "))
        }
    }
}
