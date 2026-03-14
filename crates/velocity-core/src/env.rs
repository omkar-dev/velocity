use std::collections::HashMap;

use regex::Regex;
use velocity_common::{Action, Result, Selector, TestSuite, VelocityError};

pub fn interpolate(input: &str, overrides: &HashMap<String, String>) -> Result<String> {
    let re = Regex::new(r"\$\{([^}]+)\}").unwrap();
    let mut missing_vars = Vec::new();
    let mut result = input.to_string();

    // Process all ${...} patterns. Iterate until no more substitutions (handles nested refs).
    loop {
        let prev = result.clone();
        let mut errors = Vec::new();

        result = re
            .replace_all(&prev, |caps: &regex::Captures| {
                let expr = &caps[1];
                resolve_var_expr(expr, overrides, &mut errors)
            })
            .to_string();

        if !errors.is_empty() {
            missing_vars.extend(errors);
            break;
        }

        if result == prev {
            break;
        }
    }

    if !missing_vars.is_empty() {
        return Err(VelocityError::MissingEnvVars { vars: missing_vars });
    }

    Ok(result)
}

fn resolve_var_expr(
    expr: &str,
    overrides: &HashMap<String, String>,
    errors: &mut Vec<String>,
) -> String {
    // ${VAR:?error message}
    if let Some(idx) = expr.find(":?") {
        let var_name = &expr[..idx];
        let error_msg = &expr[idx + 2..];
        if let Some(val) = lookup_var(var_name, overrides) {
            return val;
        }
        errors.push(format!("{var_name}: {error_msg}"));
        return format!("${{{expr}}}");
    }

    // ${VAR:-default}
    if let Some(idx) = expr.find(":-") {
        let var_name = &expr[..idx];
        let default_val = &expr[idx + 2..];
        if let Some(val) = lookup_var(var_name, overrides) {
            return val;
        }
        return default_val.to_string();
    }

    // ${VAR}
    let var_name = expr;
    if let Some(val) = lookup_var(var_name, overrides) {
        return val;
    }

    errors.push(var_name.to_string());
    format!("${{{expr}}}")
}

fn lookup_var(name: &str, overrides: &HashMap<String, String>) -> Option<String> {
    overrides
        .get(name)
        .cloned()
        .or_else(|| std::env::var(name).ok())
}

pub fn interpolate_suite(suite: &mut TestSuite, overrides: &HashMap<String, String>) -> Result<()> {
    suite.app_id = interpolate(&suite.app_id, overrides)?;

    for flow in &mut suite.flows {
        for step in &mut flow.steps {
            interpolate_action(&mut step.action, overrides)?;
        }
    }

    for test in &mut suite.tests {
        for step in &mut test.steps {
            interpolate_action(&mut step.action, overrides)?;
        }
    }

    Ok(())
}

fn interpolate_action(action: &mut Action, overrides: &HashMap<String, String>) -> Result<()> {
    match action {
        Action::LaunchApp { app_id, .. } => {
            *app_id = interpolate(app_id, overrides)?;
        }
        Action::StopApp { app_id } => {
            *app_id = interpolate(app_id, overrides)?;
        }
        Action::Tap { selector }
        | Action::DoubleTap { selector }
        | Action::ClearText { selector }
        | Action::AssertVisible { selector }
        | Action::AssertNotVisible { selector } => {
            interpolate_selector(selector, overrides)?;
        }
        Action::LongPress { selector, .. } => {
            interpolate_selector(selector, overrides)?;
        }
        Action::InputText { selector, text } => {
            interpolate_selector(selector, overrides)?;
            *text = interpolate(text, overrides)?;
        }
        Action::AssertText { selector, expected } => {
            interpolate_selector(selector, overrides)?;
            *expected = interpolate(expected, overrides)?;
        }
        Action::ScrollUntilVisible { selector, .. } => {
            interpolate_selector(selector, overrides)?;
        }
        Action::Screenshot { filename } => {
            if let Some(f) = filename {
                *f = interpolate(f, overrides)?;
            }
        }
        Action::RunFlow { flow_id } => {
            *flow_id = interpolate(flow_id, overrides)?;
        }
        Action::Swipe { .. } | Action::PressKey { .. } | Action::Wait { .. } => {}
    }
    Ok(())
}

fn interpolate_selector(
    selector: &mut Selector,
    overrides: &HashMap<String, String>,
) -> Result<()> {
    match selector {
        Selector::Id(v)
        | Selector::Text(v)
        | Selector::TextContains(v)
        | Selector::AccessibilityId(v)
        | Selector::ClassName(v) => {
            *v = interpolate(v, overrides)?;
        }
        Selector::Index {
            selector: inner, ..
        } => {
            interpolate_selector(inner, overrides)?;
        }
        Selector::Compound(selectors) => {
            for s in selectors {
                interpolate_selector(s, overrides)?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn overrides(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn interpolate_simple_var() {
        let ovr = overrides(&[("USER", "alice")]);
        assert_eq!(interpolate("Hello ${USER}", &ovr).unwrap(), "Hello alice");
    }

    #[test]
    fn interpolate_default_value() {
        let ovr = HashMap::new();
        assert_eq!(
            interpolate("${HOST:-localhost}", &ovr).unwrap(),
            "localhost"
        );
    }

    #[test]
    fn interpolate_default_not_used_when_present() {
        let ovr = overrides(&[("HOST", "prod.example.com")]);
        assert_eq!(
            interpolate("${HOST:-localhost}", &ovr).unwrap(),
            "prod.example.com"
        );
    }

    #[test]
    fn interpolate_required_var_present() {
        let ovr = overrides(&[("API_KEY", "secret123")]);
        assert_eq!(
            interpolate("${API_KEY:?API key is required}", &ovr).unwrap(),
            "secret123"
        );
    }

    #[test]
    fn interpolate_required_var_missing() {
        let ovr = HashMap::new();
        let result = interpolate("${API_KEY:?API key is required}", &ovr);
        assert!(result.is_err());
        match result.unwrap_err() {
            VelocityError::MissingEnvVars { vars } => {
                assert!(vars[0].contains("API_KEY"));
            }
            other => panic!("expected MissingEnvVars, got {:?}", other),
        }
    }

    #[test]
    fn interpolate_missing_var_no_default() {
        let ovr = HashMap::new();
        let result = interpolate("${MISSING_VAR}", &ovr);
        assert!(result.is_err());
    }

    #[test]
    fn interpolate_multiple_vars() {
        let ovr = overrides(&[("A", "1"), ("B", "2")]);
        assert_eq!(interpolate("${A}-${B}", &ovr).unwrap(), "1-2");
    }

    #[test]
    fn interpolate_no_vars() {
        let ovr = HashMap::new();
        assert_eq!(
            interpolate("no variables here", &ovr).unwrap(),
            "no variables here"
        );
    }

    #[test]
    fn interpolate_from_process_env() {
        // PATH should exist in virtually all environments
        let ovr = HashMap::new();
        let result = interpolate("${PATH}", &ovr);
        assert!(result.is_ok());
        assert!(!result.unwrap().is_empty());
    }

    #[test]
    fn interpolate_override_takes_precedence() {
        let ovr = overrides(&[("PATH", "my_custom_path")]);
        assert_eq!(interpolate("${PATH}", &ovr).unwrap(), "my_custom_path");
    }

    #[test]
    fn interpolate_suite_app_id() {
        use velocity_common::{SuiteConfig, TestCase};

        let ovr = overrides(&[("APP", "com.prod.app")]);
        let mut suite = TestSuite {
            app_id: "${APP}".to_string(),
            config: SuiteConfig::default(),
            flows: vec![],
            tests: vec![TestCase {
                name: "t".to_string(),
                tags: vec![],
                isolated: false,
                steps: vec![],
            }],
        };

        interpolate_suite(&mut suite, &ovr).unwrap();
        assert_eq!(suite.app_id, "com.prod.app");
    }

    #[test]
    fn interpolate_suite_step_text() {
        use velocity_common::{Step, SuiteConfig, TestCase};

        let ovr = overrides(&[("USERNAME", "testuser")]);
        let mut suite = TestSuite {
            app_id: "com.test".to_string(),
            config: SuiteConfig::default(),
            flows: vec![],
            tests: vec![TestCase {
                name: "t".to_string(),
                tags: vec![],
                isolated: false,
                steps: vec![Step {
                    action: Action::InputText {
                        selector: Selector::Id("email".to_string()),
                        text: "${USERNAME}@example.com".to_string(),
                    },
                    timeout_ms: None,
                }],
            }],
        };

        interpolate_suite(&mut suite, &ovr).unwrap();
        match &suite.tests[0].steps[0].action {
            Action::InputText { text, .. } => {
                assert_eq!(text, "testuser@example.com");
            }
            other => panic!("expected InputText, got {:?}", other),
        }
    }
}
