use velocity_common::test_types::{TestCase, TestSuite};
use velocity_common::types::{Action, Selector};

/// Severity of a lint warning.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LintSeverity {
    Warning,
    Error,
}

/// A lint diagnostic about a selector.
#[derive(Debug, Clone)]
pub struct LintDiagnostic {
    pub severity: LintSeverity,
    pub test_name: String,
    pub step_index: usize,
    pub selector: String,
    pub rule: String,
    pub message: String,
}

/// Lint all selectors in a test suite.
pub fn lint_suite(suite: &TestSuite) -> Vec<LintDiagnostic> {
    let mut diagnostics = Vec::new();
    for test in &suite.tests {
        lint_test(test, &mut diagnostics);
    }
    diagnostics
}

fn lint_test(test: &TestCase, diagnostics: &mut Vec<LintDiagnostic>) {
    for (i, step) in test.steps.iter().enumerate() {
        if let Some(selector) = extract_selector(&step.action) {
            lint_selector(selector, &test.name, i, diagnostics);
        }
    }
}

/// Extract the selector from an action, if any.
fn extract_selector(action: &Action) -> Option<&Selector> {
    match action {
        Action::Tap { selector } => Some(selector),
        Action::DoubleTap { selector } => Some(selector),
        Action::LongPress { selector, .. } => Some(selector),
        Action::InputText { selector, .. } => Some(selector),
        Action::ClearText { selector } => Some(selector),
        Action::AssertVisible { selector } => Some(selector),
        Action::AssertNotVisible { selector } => Some(selector),
        Action::AssertText { selector, .. } => Some(selector),
        Action::ScrollUntilVisible { selector, .. } => Some(selector),
        _ => None,
    }
}

fn lint_selector(
    selector: &Selector,
    test_name: &str,
    step_index: usize,
    diagnostics: &mut Vec<LintDiagnostic>,
) {
    let sel_str = selector.to_string();

    // Rule 1: Platform-specific IDs
    check_platform_specific_id(selector, test_name, step_index, &sel_str, diagnostics);

    // Rule 2: Fragile text selectors with exact match
    check_fragile_text(selector, test_name, step_index, &sel_str, diagnostics);

    // Rule 3: Index-based selectors (fragile)
    check_index_selector(selector, test_name, step_index, &sel_str, diagnostics);

    // Rule 4: Empty or overly broad selectors
    check_broad_selector(selector, test_name, step_index, &sel_str, diagnostics);
}

fn check_platform_specific_id(
    selector: &Selector,
    test_name: &str,
    step_index: usize,
    sel_str: &str,
    diagnostics: &mut Vec<LintDiagnostic>,
) {
    if let Selector::Id(id) = selector {
        // Android resource IDs contain colons/slashes
        if id.contains(':') || id.contains('/') {
            diagnostics.push(LintDiagnostic {
                severity: LintSeverity::Warning,
                test_name: test_name.to_string(),
                step_index,
                selector: sel_str.to_string(),
                rule: "platform-specific-id".to_string(),
                message: format!(
                    "ID '{id}' appears platform-specific (contains ':' or '/'). \
                     Use accessibilityId for cross-platform compatibility."
                ),
            });
        }
    }
}

fn check_fragile_text(
    selector: &Selector,
    test_name: &str,
    step_index: usize,
    sel_str: &str,
    diagnostics: &mut Vec<LintDiagnostic>,
) {
    if let Selector::Text(text) = selector {
        if text.len() > 50 {
            diagnostics.push(LintDiagnostic {
                severity: LintSeverity::Warning,
                test_name: test_name.to_string(),
                step_index,
                selector: sel_str.to_string(),
                rule: "long-text-selector".to_string(),
                message: "Text selector is very long (>50 chars). Consider using \
                          textContains or accessibilityId for stability."
                    .to_string(),
            });
        }
    }
}

fn check_index_selector(
    selector: &Selector,
    test_name: &str,
    step_index: usize,
    sel_str: &str,
    diagnostics: &mut Vec<LintDiagnostic>,
) {
    if let Selector::Index { .. } = selector {
        diagnostics.push(LintDiagnostic {
            severity: LintSeverity::Warning,
            test_name: test_name.to_string(),
            step_index,
            selector: sel_str.to_string(),
            rule: "index-selector".to_string(),
            message: "Index-based selectors are fragile \u{2014} they break when element \
                      order changes. Prefer id or accessibilityId."
                .to_string(),
        });
    }
}

fn check_broad_selector(
    selector: &Selector,
    test_name: &str,
    step_index: usize,
    sel_str: &str,
    diagnostics: &mut Vec<LintDiagnostic>,
) {
    if let Selector::ClassName(cls) = selector {
        let broad_types = ["View", "UIView", "FrameLayout", "LinearLayout", "div"];
        if broad_types.iter().any(|b| cls == *b) {
            diagnostics.push(LintDiagnostic {
                severity: LintSeverity::Warning,
                test_name: test_name.to_string(),
                step_index,
                selector: sel_str.to_string(),
                rule: "broad-class-selector".to_string(),
                message: format!(
                    "Class selector '{cls}' is very broad and will match many elements. \
                     Consider adding an id or combining with other selectors."
                ),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use velocity_common::test_types::Step;

    fn make_step(action: Action) -> Step {
        Step {
            action,
            timeout_ms: None,
        }
    }

    #[test]
    fn test_platform_specific_id() {
        let selector = Selector::Id("com.example:id/button".to_string());
        let mut diags = Vec::new();
        lint_selector(&selector, "test1", 0, &mut diags);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule, "platform-specific-id");
    }

    #[test]
    fn test_long_text_selector() {
        let long_text = "a".repeat(51);
        let selector = Selector::Text(long_text);
        let mut diags = Vec::new();
        lint_selector(&selector, "test1", 0, &mut diags);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule, "long-text-selector");
    }

    #[test]
    fn test_index_selector() {
        let selector = Selector::Index {
            selector: Box::new(Selector::ClassName("Button".to_string())),
            index: 2,
        };
        let mut diags = Vec::new();
        lint_selector(&selector, "test1", 0, &mut diags);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule, "index-selector");
    }

    #[test]
    fn test_broad_class_selector() {
        let selector = Selector::ClassName("UIView".to_string());
        let mut diags = Vec::new();
        lint_selector(&selector, "test1", 0, &mut diags);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].rule, "broad-class-selector");
    }

    #[test]
    fn test_good_selector_no_warnings() {
        let selector = Selector::AccessibilityId("login_button".to_string());
        let mut diags = Vec::new();
        lint_selector(&selector, "test1", 0, &mut diags);
        assert!(diags.is_empty());
    }
}
