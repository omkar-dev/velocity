use std::collections::HashSet;

use velocity_common::{Action, Result, Selector, Step, TestSuite, VelocityError};

pub fn validate_suite(suite: &TestSuite) -> Result<()> {
    let flow_ids: HashSet<&str> = suite.flows.iter().map(|f| f.id.as_str()).collect();

    for test in &suite.tests {
        for (i, step) in test.steps.iter().enumerate() {
            validate_step(step, &test.name, i, &flow_ids)?;
        }
    }

    for flow in &suite.flows {
        for (i, step) in flow.steps.iter().enumerate() {
            validate_step(step, &flow.id, i, &flow_ids)?;
        }
    }

    Ok(())
}

fn validate_step(
    step: &Step,
    context_name: &str,
    step_index: usize,
    flow_ids: &HashSet<&str>,
) -> Result<()> {
    match &step.action {
        Action::Tap { selector }
        | Action::DoubleTap { selector }
        | Action::ClearText { selector }
        | Action::AssertVisible { selector }
        | Action::AssertNotVisible { selector } => {
            validate_selector(selector, context_name, step_index)?;
        }
        Action::LongPress { selector, .. } => {
            validate_selector(selector, context_name, step_index)?;
        }
        Action::InputText { selector, .. } => {
            validate_selector(selector, context_name, step_index)?;
        }
        Action::AssertText { selector, .. } => {
            validate_selector(selector, context_name, step_index)?;
        }
        Action::ScrollUntilVisible { selector, .. } => {
            validate_selector(selector, context_name, step_index)?;
        }
        Action::RunFlow { flow_id } => {
            if !flow_ids.contains(flow_id.as_str()) {
                return Err(VelocityError::UnknownFlowRef {
                    flow_id: flow_id.clone(),
                    test_name: context_name.to_string(),
                });
            }
        }
        Action::LaunchApp { .. }
        | Action::StopApp { .. }
        | Action::Swipe { .. }
        | Action::Screenshot { .. }
        | Action::PressKey { .. }
        | Action::Wait { .. } => {}
    }
    Ok(())
}

fn validate_selector(selector: &Selector, test_name: &str, step_index: usize) -> Result<()> {
    match selector {
        Selector::Id(v)
        | Selector::Text(v)
        | Selector::TextContains(v)
        | Selector::AccessibilityId(v)
        | Selector::ClassName(v) => {
            if v.trim().is_empty() {
                return Err(VelocityError::InvalidSelector {
                    test_name: test_name.to_string(),
                    step_index,
                    reason: "selector value is empty".to_string(),
                });
            }
        }
        Selector::Index {
            selector: inner, ..
        } => {
            validate_selector(inner, test_name, step_index)?;
        }
        Selector::Compound(selectors) => {
            if selectors.is_empty() {
                return Err(VelocityError::InvalidSelector {
                    test_name: test_name.to_string(),
                    step_index,
                    reason: "compound selector has no children".to_string(),
                });
            }
            for s in selectors {
                validate_selector(s, test_name, step_index)?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use velocity_common::{Flow, SuiteConfig, TestCase};

    fn make_suite(flows: Vec<Flow>, tests: Vec<TestCase>) -> TestSuite {
        TestSuite {
            app_id: "com.test".to_string(),
            config: SuiteConfig::default(),
            flows,
            tests,
        }
    }

    fn step(action: Action) -> Step {
        Step {
            action,
            timeout_ms: None,
        }
    }

    #[test]
    fn valid_suite_passes() {
        let suite = make_suite(
            vec![Flow {
                id: "login".to_string(),
                steps: vec![step(Action::Tap {
                    selector: Selector::Id("btn".to_string()),
                })],
            }],
            vec![TestCase {
                name: "test1".to_string(),
                tags: vec![],
                isolated: false,
                steps: vec![
                    step(Action::RunFlow {
                        flow_id: "login".to_string(),
                    }),
                    step(Action::AssertVisible {
                        selector: Selector::Text("Welcome".to_string()),
                    }),
                ],
            }],
        );
        assert!(validate_suite(&suite).is_ok());
    }

    #[test]
    fn unknown_flow_ref_fails() {
        let suite = make_suite(
            vec![],
            vec![TestCase {
                name: "test1".to_string(),
                tags: vec![],
                isolated: false,
                steps: vec![step(Action::RunFlow {
                    flow_id: "nonexistent".to_string(),
                })],
            }],
        );
        let err = validate_suite(&suite).unwrap_err();
        match err {
            VelocityError::UnknownFlowRef { flow_id, .. } => {
                assert_eq!(flow_id, "nonexistent");
            }
            other => panic!("expected UnknownFlowRef, got {:?}", other),
        }
    }

    #[test]
    fn empty_selector_fails() {
        let suite = make_suite(
            vec![],
            vec![TestCase {
                name: "test1".to_string(),
                tags: vec![],
                isolated: false,
                steps: vec![step(Action::Tap {
                    selector: Selector::Id("".to_string()),
                })],
            }],
        );
        let err = validate_suite(&suite).unwrap_err();
        match err {
            VelocityError::InvalidSelector { reason, .. } => {
                assert!(reason.contains("empty"));
            }
            other => panic!("expected InvalidSelector, got {:?}", other),
        }
    }

    #[test]
    fn empty_compound_selector_fails() {
        let suite = make_suite(
            vec![],
            vec![TestCase {
                name: "test1".to_string(),
                tags: vec![],
                isolated: false,
                steps: vec![step(Action::Tap {
                    selector: Selector::Compound(vec![]),
                })],
            }],
        );
        let err = validate_suite(&suite).unwrap_err();
        match err {
            VelocityError::InvalidSelector { reason, .. } => {
                assert!(reason.contains("compound"));
            }
            other => panic!("expected InvalidSelector, got {:?}", other),
        }
    }

    #[test]
    fn flow_steps_are_validated() {
        let suite = make_suite(
            vec![Flow {
                id: "bad_flow".to_string(),
                steps: vec![step(Action::Tap {
                    selector: Selector::Id("  ".to_string()),
                })],
            }],
            vec![TestCase {
                name: "test1".to_string(),
                tags: vec![],
                isolated: false,
                steps: vec![step(Action::Wait { ms: 100 })],
            }],
        );
        assert!(validate_suite(&suite).is_err());
    }

    #[test]
    fn non_selector_actions_pass() {
        let suite = make_suite(
            vec![],
            vec![TestCase {
                name: "test1".to_string(),
                tags: vec![],
                isolated: false,
                steps: vec![
                    step(Action::LaunchApp {
                        app_id: "com.test".to_string(),
                        clear_state: false,
                    }),
                    step(Action::Wait { ms: 500 }),
                    step(Action::Screenshot { filename: None }),
                    step(Action::PressKey {
                        key: velocity_common::Key::Back,
                    }),
                ],
            }],
        );
        assert!(validate_suite(&suite).is_ok());
    }
}
