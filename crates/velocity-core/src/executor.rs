use std::path::PathBuf;
use std::time::{Duration, Instant};

use velocity_common::{
    Action, Direction, PlatformDriver, Result, Step, StepResult, StepStatus, SuiteConfig, TestCase,
    TestResult, TestStatus, VelocityError,
};

use crate::selector::SelectorEngine;
use crate::sync::AdaptiveSyncEngine;

pub struct TestExecutor<'a> {
    driver: &'a dyn PlatformDriver,
    sync_engine: AdaptiveSyncEngine,
    selector_engine: SelectorEngine,
    config: SuiteConfig,
}

impl<'a> TestExecutor<'a> {
    pub fn new(driver: &'a dyn PlatformDriver, config: SuiteConfig) -> Self {
        let sync_engine = AdaptiveSyncEngine::new(config.sync.clone());
        let selector_engine = SelectorEngine::new();
        Self {
            driver,
            sync_engine,
            selector_engine,
            config,
        }
    }

    pub async fn execute_test(
        &mut self,
        test: &TestCase,
        device_id: &str,
        app_id: &str,
    ) -> Result<TestResult> {
        let test_start = Instant::now();
        let mut step_results = Vec::with_capacity(test.steps.len());
        let mut screenshots = Vec::new();
        let mut test_error: Option<String> = None;

        for (i, step) in test.steps.iter().enumerate() {
            let step_result = self.execute_step(step, device_id, app_id, i).await;

            match step_result {
                Ok(result) => {
                    if result.status == StepStatus::Failed {
                        if let Some(ref path) = result.screenshot {
                            screenshots.push(path.clone());
                        }
                        test_error = result.error_message.clone();
                        step_results.push(result);
                        // Mark remaining steps as skipped
                        for j in (i + 1)..test.steps.len() {
                            step_results.push(StepResult {
                                step_index: j,
                                action_name: action_name(&test.steps[j].action),
                                status: StepStatus::Skipped,
                                duration: Duration::ZERO,
                                screenshot: None,
                                error_message: None,
                            });
                        }
                        break;
                    }
                    step_results.push(result);
                }
                Err(e) => {
                    let error_msg = e.to_string();

                    // Take failure screenshot if configured
                    let screenshot = if self.config.artifacts.on_failure {
                        self.take_failure_screenshot(device_id, &test.name, i).await
                    } else {
                        None
                    };
                    if let Some(ref path) = screenshot {
                        screenshots.push(path.clone());
                    }

                    test_error = Some(error_msg.clone());
                    step_results.push(StepResult {
                        step_index: i,
                        action_name: action_name(&step.action),
                        status: StepStatus::Failed,
                        duration: test_start.elapsed(),
                        screenshot,
                        error_message: Some(error_msg),
                    });

                    // Mark remaining steps as skipped
                    for j in (i + 1)..test.steps.len() {
                        step_results.push(StepResult {
                            step_index: j,
                            action_name: action_name(&test.steps[j].action),
                            status: StepStatus::Skipped,
                            duration: Duration::ZERO,
                            screenshot: None,
                            error_message: None,
                        });
                    }
                    break;
                }
            }
        }

        let all_passed = step_results.iter().all(|r| r.status == StepStatus::Passed);
        let status = if all_passed {
            TestStatus::Passed
        } else {
            TestStatus::Failed
        };

        Ok(TestResult {
            test_name: test.name.clone(),
            status,
            duration: test_start.elapsed(),
            steps: step_results,
            retries: 0,
            error_message: test_error,
            screenshots,
        })
    }

    pub async fn execute_step(
        &mut self,
        step: &Step,
        device_id: &str,
        app_id: &str,
        step_index: usize,
    ) -> Result<StepResult> {
        let step_start = Instant::now();
        let name = action_name(&step.action);

        // Pre-action sync (skip for wait and screenshot actions)
        let skip_sync = matches!(step.action, Action::Wait { .. } | Action::Screenshot { .. });
        if !skip_sync {
            self.sync_engine
                .wait_for_idle(self.driver, device_id)
                .await?;
        }

        let result = self.execute_action(&step.action, device_id, app_id).await;

        // Post-action sync for actions that modify UI state
        let mutating = matches!(
            step.action,
            Action::Tap { .. }
                | Action::DoubleTap { .. }
                | Action::LongPress { .. }
                | Action::InputText { .. }
                | Action::ClearText { .. }
                | Action::Swipe { .. }
                | Action::ScrollUntilVisible { .. }
                | Action::PressKey { .. }
                | Action::LaunchApp { .. }
                | Action::StopApp { .. }
        );

        match result {
            Ok(()) => {
                if mutating {
                    self.selector_engine.invalidate_cache();
                    // Best-effort post-action sync; ignore timeout since the UI may still be settling
                    let _ = self.sync_engine.wait_for_idle(self.driver, device_id).await;
                }
                Ok(StepResult {
                    step_index,
                    action_name: name,
                    status: StepStatus::Passed,
                    duration: step_start.elapsed(),
                    screenshot: None,
                    error_message: None,
                })
            }
            Err(e) => {
                let screenshot = if self.config.artifacts.on_failure {
                    self.take_failure_screenshot(device_id, &name, step_index)
                        .await
                } else {
                    None
                };
                Ok(StepResult {
                    step_index,
                    action_name: name,
                    status: StepStatus::Failed,
                    duration: step_start.elapsed(),
                    screenshot,
                    error_message: Some(e.to_string()),
                })
            }
        }
    }

    async fn execute_action(
        &mut self,
        action: &Action,
        device_id: &str,
        app_id: &str,
    ) -> Result<()> {
        match action {
            Action::LaunchApp {
                app_id: override_id,
                clear_state,
            } => {
                let id = if override_id.is_empty() {
                    app_id
                } else {
                    override_id
                };
                self.driver.launch_app(device_id, id, *clear_state).await
            }
            Action::StopApp {
                app_id: override_id,
            } => {
                let id = if override_id.is_empty() {
                    app_id
                } else {
                    override_id
                };
                self.driver.stop_app(device_id, id).await
            }
            Action::Tap { selector } => {
                let element = self
                    .selector_engine
                    .find_element(self.driver, device_id, selector)
                    .await?;
                self.driver.tap(device_id, &element).await
            }
            Action::DoubleTap { selector } => {
                let element = self
                    .selector_engine
                    .find_element(self.driver, device_id, selector)
                    .await?;
                self.driver.double_tap(device_id, &element).await
            }
            Action::LongPress {
                selector,
                duration_ms,
            } => {
                let element = self
                    .selector_engine
                    .find_element(self.driver, device_id, selector)
                    .await?;
                let ms = duration_ms.unwrap_or(1000);
                self.driver.long_press(device_id, &element, ms).await
            }
            Action::InputText { selector, text } => {
                let element = self
                    .selector_engine
                    .find_element(self.driver, device_id, selector)
                    .await?;
                self.driver.input_text(device_id, &element, text).await
            }
            Action::ClearText { selector } => {
                let element = self
                    .selector_engine
                    .find_element(self.driver, device_id, selector)
                    .await?;
                self.driver.clear_text(device_id, &element).await
            }
            Action::AssertVisible { selector } => {
                let element = self
                    .selector_engine
                    .find_element(self.driver, device_id, selector)
                    .await?;
                let visible = self.driver.is_element_visible(device_id, &element).await?;
                if !visible {
                    return Err(VelocityError::AssertionFailed {
                        expected: "visible".to_string(),
                        actual: "not visible".to_string(),
                        selector: format!("{selector}"),
                        screenshot: None,
                    });
                }
                Ok(())
            }
            Action::AssertNotVisible { selector } => {
                let result = self
                    .selector_engine
                    .find_element(self.driver, device_id, selector)
                    .await;
                match result {
                    Err(_) => Ok(()), // Element not found = not visible, pass
                    Ok(element) => {
                        let visible = self.driver.is_element_visible(device_id, &element).await?;
                        if visible {
                            return Err(VelocityError::AssertionFailed {
                                expected: "not visible".to_string(),
                                actual: "visible".to_string(),
                                selector: format!("{selector}"),
                                screenshot: None,
                            });
                        }
                        Ok(())
                    }
                }
            }
            Action::AssertText { selector, expected } => {
                let element = self
                    .selector_engine
                    .find_element(self.driver, device_id, selector)
                    .await?;
                let actual = self.driver.get_element_text(device_id, &element).await?;
                if actual != *expected {
                    return Err(VelocityError::AssertionFailed {
                        expected: expected.clone(),
                        actual,
                        selector: format!("{selector}"),
                        screenshot: None,
                    });
                }
                Ok(())
            }
            Action::ScrollUntilVisible {
                selector,
                direction,
                max_scrolls,
            } => {
                for _ in 0..*max_scrolls {
                    let found = self
                        .selector_engine
                        .find_element(self.driver, device_id, selector)
                        .await;
                    if let Ok(element) = found {
                        if self
                            .driver
                            .is_element_visible(device_id, &element)
                            .await
                            .unwrap_or(false)
                        {
                            return Ok(());
                        }
                    }
                    self.selector_engine.invalidate_cache();
                    self.driver.swipe(device_id, *direction).await?;
                    let _ = self.sync_engine.wait_for_idle(self.driver, device_id).await;
                }
                Err(VelocityError::ElementNotFound {
                    selector: format!("{selector}"),
                    timeout_ms: 0,
                    screenshot: None,
                    hierarchy_snapshot: None,
                })
            }
            Action::Swipe {
                direction,
                from,
                to,
            } => {
                if let (Some(from), Some(to)) = (from, to) {
                    self.driver.swipe_coords(device_id, *from, *to).await
                } else if let Some(dir) = direction {
                    self.driver.swipe(device_id, *dir).await
                } else {
                    self.driver.swipe(device_id, Direction::Down).await
                }
            }
            Action::Screenshot { filename } => {
                let data = self.driver.screenshot(device_id).await?;
                let fname = filename
                    .clone()
                    .unwrap_or_else(|| format!("screenshot_{}.png", chrono_millis()));
                let path = PathBuf::from(&self.config.artifacts.output_dir).join(&fname);
                // Best-effort write; don't fail the step if artifacts dir doesn't exist
                let _ = std::fs::create_dir_all(path.parent().unwrap_or(&PathBuf::from(".")));
                let _ = std::fs::write(&path, &data);
                Ok(())
            }
            Action::PressKey { key } => self.driver.press_key(device_id, key.clone()).await,
            Action::Wait { ms } => {
                tokio::time::sleep(Duration::from_millis(*ms)).await;
                Ok(())
            }
            Action::RunFlow { .. } => {
                // Flows should be resolved before execution
                Err(VelocityError::Config(
                    "runFlow encountered during execution; flows must be resolved first"
                        .to_string(),
                ))
            }
        }
    }

    async fn take_failure_screenshot(
        &self,
        device_id: &str,
        context: &str,
        step_index: usize,
    ) -> Option<PathBuf> {
        let data = self.driver.screenshot(device_id).await.ok()?;
        let sanitized = context.replace(|c: char| !c.is_alphanumeric(), "_");
        let fname = format!("failure_{sanitized}_step{step_index}.png");
        let path = PathBuf::from(&self.config.artifacts.output_dir).join(&fname);
        let _ = std::fs::create_dir_all(path.parent()?);
        std::fs::write(&path, &data).ok()?;
        Some(path)
    }
}

fn action_name(action: &Action) -> String {
    match action {
        Action::LaunchApp { .. } => "launchApp".to_string(),
        Action::StopApp { .. } => "stopApp".to_string(),
        Action::Tap { .. } => "tap".to_string(),
        Action::DoubleTap { .. } => "doubleTap".to_string(),
        Action::LongPress { .. } => "longPress".to_string(),
        Action::InputText { .. } => "inputText".to_string(),
        Action::ClearText { .. } => "clearText".to_string(),
        Action::AssertVisible { .. } => "assertVisible".to_string(),
        Action::AssertNotVisible { .. } => "assertNotVisible".to_string(),
        Action::AssertText { .. } => "assertText".to_string(),
        Action::ScrollUntilVisible { .. } => "scrollUntilVisible".to_string(),
        Action::Swipe { .. } => "swipe".to_string(),
        Action::Screenshot { .. } => "screenshot".to_string(),
        Action::PressKey { .. } => "pressKey".to_string(),
        Action::Wait { .. } => "wait".to_string(),
        Action::RunFlow { .. } => "runFlow".to_string(),
    }
}

fn chrono_millis() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}
