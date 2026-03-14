use std::collections::HashMap;

use velocity_common::{Action, Flow, Result, Step, TestCase, TestSuite, VelocityError};

pub fn resolve_flows(suite: &TestSuite) -> Result<Vec<TestCase>> {
    let flow_map: HashMap<&str, &Flow> = suite.flows.iter().map(|f| (f.id.as_str(), f)).collect();

    suite
        .tests
        .iter()
        .map(|test| resolve_test_case(test, &flow_map))
        .collect()
}

fn resolve_test_case(test: &TestCase, flow_map: &HashMap<&str, &Flow>) -> Result<TestCase> {
    let resolved_steps = resolve_steps(&test.steps, flow_map, &test.name)?;
    Ok(TestCase {
        name: test.name.clone(),
        tags: test.tags.clone(),
        isolated: test.isolated,
        steps: resolved_steps,
    })
}

fn resolve_steps(
    steps: &[Step],
    flow_map: &HashMap<&str, &Flow>,
    test_name: &str,
) -> Result<Vec<Step>> {
    let mut resolved = Vec::new();

    for step in steps {
        match &step.action {
            Action::RunFlow { flow_id } => {
                let flow = flow_map.get(flow_id.as_str()).ok_or_else(|| {
                    VelocityError::UnknownFlowRef {
                        flow_id: flow_id.clone(),
                        test_name: test_name.to_string(),
                    }
                })?;
                // Recursively resolve in case flows reference other flows
                let inner = resolve_steps(&flow.steps, flow_map, test_name)?;
                resolved.extend(inner);
            }
            _ => {
                resolved.push(step.clone());
            }
        }
    }

    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use velocity_common::{Selector, SuiteConfig};

    fn step(action: Action) -> Step {
        Step {
            action,
            timeout_ms: None,
        }
    }

    #[test]
    fn resolve_inlines_flow_steps() {
        let suite = TestSuite {
            app_id: "com.test".to_string(),
            config: SuiteConfig::default(),
            flows: vec![Flow {
                id: "login".to_string(),
                steps: vec![
                    step(Action::Tap {
                        selector: Selector::Id("username".to_string()),
                    }),
                    step(Action::InputText {
                        selector: Selector::Id("username".to_string()),
                        text: "user".to_string(),
                    }),
                    step(Action::Tap {
                        selector: Selector::Id("submit".to_string()),
                    }),
                ],
            }],
            tests: vec![TestCase {
                name: "test1".to_string(),
                tags: vec![],
                isolated: false,
                steps: vec![
                    step(Action::LaunchApp {
                        app_id: "com.test".to_string(),
                        clear_state: true,
                    }),
                    step(Action::RunFlow {
                        flow_id: "login".to_string(),
                    }),
                    step(Action::AssertVisible {
                        selector: Selector::Text("Dashboard".to_string()),
                    }),
                ],
            }],
        };

        let resolved = resolve_flows(&suite).unwrap();
        assert_eq!(resolved.len(), 1);
        // 1 launchApp + 3 login steps + 1 assertVisible = 5
        assert_eq!(resolved[0].steps.len(), 5);
        // No RunFlow actions remain
        for step in &resolved[0].steps {
            assert!(!matches!(step.action, Action::RunFlow { .. }));
        }
    }

    #[test]
    fn resolve_nested_flows() {
        let suite = TestSuite {
            app_id: "com.test".to_string(),
            config: SuiteConfig::default(),
            flows: vec![
                Flow {
                    id: "setup".to_string(),
                    steps: vec![step(Action::LaunchApp {
                        app_id: "com.test".to_string(),
                        clear_state: true,
                    })],
                },
                Flow {
                    id: "login".to_string(),
                    steps: vec![
                        step(Action::RunFlow {
                            flow_id: "setup".to_string(),
                        }),
                        step(Action::Tap {
                            selector: Selector::Id("login".to_string()),
                        }),
                    ],
                },
            ],
            tests: vec![TestCase {
                name: "test1".to_string(),
                tags: vec![],
                isolated: false,
                steps: vec![step(Action::RunFlow {
                    flow_id: "login".to_string(),
                })],
            }],
        };

        let resolved = resolve_flows(&suite).unwrap();
        assert_eq!(resolved[0].steps.len(), 2);
        assert!(matches!(
            resolved[0].steps[0].action,
            Action::LaunchApp { .. }
        ));
        assert!(matches!(resolved[0].steps[1].action, Action::Tap { .. }));
    }

    #[test]
    fn resolve_unknown_flow_fails() {
        let suite = TestSuite {
            app_id: "com.test".to_string(),
            config: SuiteConfig::default(),
            flows: vec![],
            tests: vec![TestCase {
                name: "test1".to_string(),
                tags: vec![],
                isolated: false,
                steps: vec![step(Action::RunFlow {
                    flow_id: "missing".to_string(),
                })],
            }],
        };

        let err = resolve_flows(&suite).unwrap_err();
        assert!(matches!(err, VelocityError::UnknownFlowRef { .. }));
    }

    #[test]
    fn resolve_preserves_non_flow_steps() {
        let suite = TestSuite {
            app_id: "com.test".to_string(),
            config: SuiteConfig::default(),
            flows: vec![],
            tests: vec![TestCase {
                name: "test1".to_string(),
                tags: vec![],
                isolated: false,
                steps: vec![
                    step(Action::Wait { ms: 100 }),
                    step(Action::Screenshot { filename: None }),
                ],
            }],
        };

        let resolved = resolve_flows(&suite).unwrap();
        assert_eq!(resolved[0].steps.len(), 2);
    }

    #[test]
    fn resolve_preserves_metadata() {
        let suite = TestSuite {
            app_id: "com.test".to_string(),
            config: SuiteConfig::default(),
            flows: vec![],
            tests: vec![TestCase {
                name: "my test".to_string(),
                tags: vec!["smoke".to_string()],
                isolated: true,
                steps: vec![step(Action::Wait { ms: 100 })],
            }],
        };

        let resolved = resolve_flows(&suite).unwrap();
        assert_eq!(resolved[0].name, "my test");
        assert_eq!(resolved[0].tags, vec!["smoke"]);
        assert!(resolved[0].isolated);
    }
}
