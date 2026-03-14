use std::fs;
use std::path::Path;

use serde_yaml::Value;
use velocity_common::{Result, VelocityError};

use crate::compat::{FileMigrationResult, MigrationIssue, MigrationReport, Severity};

const UNSUPPORTED_CONSTRUCTS: &[&str] = &[
    "runScript",
    "onFlowComplete",
    "onFlowStart",
    "evalScript",
    "defineVariables",
    "repeat",
    "condition",
    "copyTextFrom",
    "eraseText",
    "hideKeyboard",
    "openLink",
    "setLocation",
    "startRecording",
    "stopRecording",
    "waitForAnimationToEnd",
];

pub struct MaestroMigrator;

impl MaestroMigrator {
    pub fn new() -> Self {
        Self
    }

    pub fn migrate_directory(&self, input_dir: &str, output_dir: &str) -> Result<MigrationReport> {
        let input_path = Path::new(input_dir);
        if !input_path.is_dir() {
            return Err(VelocityError::Config(format!(
                "Input directory does not exist: {input_dir}"
            )));
        }

        let output_path = Path::new(output_dir);
        fs::create_dir_all(output_path).map_err(|e| {
            VelocityError::Internal(anyhow::anyhow!("Failed to create output directory: {e}"))
        })?;

        let mut report = MigrationReport::new();

        let entries = fs::read_dir(input_path).map_err(|e| {
            VelocityError::Internal(anyhow::anyhow!("Failed to read directory: {e}"))
        })?;

        for entry in entries {
            let entry = entry.map_err(|e| {
                VelocityError::Internal(anyhow::anyhow!("Failed to read entry: {e}"))
            })?;

            let path = entry.path();
            let ext = path.extension().and_then(|e| e.to_str());
            if !matches!(ext, Some("yaml" | "yml")) {
                continue;
            }

            let file_name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown");
            let output_file = output_path
                .join(format!("{file_name}.velocity.yaml"))
                .to_string_lossy()
                .to_string();
            let input_file = path.to_string_lossy().to_string();

            let result = self.migrate_file(&input_file, &output_file)?;
            report.add_result(result);
        }

        Ok(report)
    }

    pub fn migrate_file(&self, input: &str, output: &str) -> Result<FileMigrationResult> {
        let content = fs::read_to_string(input)
            .map_err(|e| VelocityError::Internal(anyhow::anyhow!("Failed to read {input}: {e}")))?;

        let doc: Value = serde_yaml::from_str(&content).map_err(|e| {
            let location = e.location();
            VelocityError::YamlParse {
                file: input.into(),
                line: location.as_ref().map_or(0, |l| l.line()),
                col: location.as_ref().map_or(0, |l| l.column()),
                message: e.to_string(),
            }
        })?;

        let mut issues = Vec::new();
        let mut velocity_steps = Vec::new();
        let mut steps_skipped = 0;

        let app_id = doc
            .get("appId")
            .and_then(|v| v.as_str())
            .unwrap_or("com.example.app")
            .to_string();

        let maestro_steps = match &doc {
            Value::Mapping(map) => {
                let mut steps = Vec::new();
                for (i, (key, _)) in map.iter().enumerate() {
                    if key.as_str() == Some("appId") {
                        continue;
                    }
                    if let Some(seq) = doc.as_sequence() {
                        steps = seq.clone();
                        break;
                    }
                    if i > 0 {
                        if let Value::Sequence(seq) = &doc[key] {
                            steps = seq.clone();
                            break;
                        }
                    }
                }
                steps
            }
            Value::Sequence(seq) => seq.clone(),
            _ => Vec::new(),
        };

        // Maestro files can also have a top-level list with appId as first mapping entry
        // Try the common format: mapping with appId + unnamed step list
        let steps_to_process = if maestro_steps.is_empty() {
            if let Value::Mapping(map) = &doc {
                let mut collected = Vec::new();
                for (key, value) in map {
                    if key.as_str() == Some("appId") {
                        continue;
                    }
                    // Each remaining key-value is a step
                    let mut step_map = serde_yaml::Mapping::new();
                    step_map.insert(key.clone(), value.clone());
                    collected.push(Value::Mapping(step_map));
                }
                collected
            } else {
                Vec::new()
            }
        } else {
            maestro_steps
        };

        for (line_idx, step) in steps_to_process.iter().enumerate() {
            let line = line_idx + 2; // approximate line number (after appId)
            match self.convert_step(step, line, &mut issues) {
                Some(converted) => velocity_steps.push(converted),
                None => steps_skipped += 1,
            }
        }

        let velocity_doc = build_velocity_yaml(&app_id, &velocity_steps);

        let success = issues.iter().all(|i| i.severity != Severity::Error);
        if success {
            fs::write(output, &velocity_doc).map_err(|e| {
                VelocityError::Internal(anyhow::anyhow!("Failed to write {output}: {e}"))
            })?;
        }

        Ok(FileMigrationResult {
            source_file: input.to_string(),
            output_file: if success {
                Some(output.to_string())
            } else {
                None
            },
            success,
            steps_migrated: velocity_steps.len(),
            steps_skipped,
            issues,
        })
    }

    fn convert_step(
        &self,
        step: &Value,
        line: usize,
        issues: &mut Vec<MigrationIssue>,
    ) -> Option<Value> {
        let mapping = step.as_mapping()?;

        if let Some((key, value)) = mapping.into_iter().next() {
            let key_str = key.as_str()?;

            // Check for unsupported constructs
            if UNSUPPORTED_CONSTRUCTS.contains(&key_str) {
                issues.push(MigrationIssue {
                    line,
                    severity: Severity::Warning,
                    message: format!("Unsupported Maestro construct '{key_str}' was skipped"),
                    maestro_construct: key_str.to_string(),
                });
                return None;
            }

            return match key_str {
                "tapOn" => Some(self.convert_tap_on(value, line, issues)),
                "assertVisible" => Some(self.convert_assert_visible(value)),
                "inputText" | "inputRandomText" => {
                    Some(self.convert_input_text(key_str, value, line, issues))
                }
                "clearState" => Some(self.convert_clear_state(value)),
                "launchApp" => Some(self.convert_launch_app(value)),
                "scrollUntilVisible" => Some(self.convert_scroll_until_visible(value)),
                "back" => Some(self.convert_back()),
                "swipe" => Some(self.convert_swipe(value, line, issues)),
                "assertNotVisible" => Some(self.convert_assert_not_visible(value)),
                "screenshot" => Some(self.convert_screenshot(value)),
                "pressKey" => Some(self.convert_press_key(value)),
                "waitForElement" | "extendedWaitUntil" => {
                    issues.push(MigrationIssue {
                        line,
                        severity: Severity::Info,
                        message: format!(
                            "'{key_str}' mapped to assertVisible (Velocity uses sync engine for waits)"
                        ),
                        maestro_construct: key_str.to_string(),
                    });
                    Some(self.convert_assert_visible(value))
                }
                "runFlow" => Some(self.convert_run_flow(value, line, issues)),
                other => {
                    issues.push(MigrationIssue {
                        line,
                        severity: Severity::Warning,
                        message: format!("Unknown Maestro construct '{other}' was skipped"),
                        maestro_construct: other.to_string(),
                    });
                    None
                }
            };
        }

        None
    }

    fn convert_tap_on(
        &self,
        value: &Value,
        line: usize,
        issues: &mut Vec<MigrationIssue>,
    ) -> Value {
        let selector = extract_selector(value, line, issues);
        let mut step = serde_yaml::Mapping::new();
        let mut action = serde_yaml::Mapping::new();
        let mut tap = serde_yaml::Mapping::new();
        tap.insert(Value::String("selector".to_string()), selector);
        action.insert(Value::String("tap".to_string()), Value::Mapping(tap));
        step.insert(Value::String("action".to_string()), Value::Mapping(action));
        Value::Mapping(step)
    }

    fn convert_assert_visible(&self, value: &Value) -> Value {
        let selector = selector_from_value(value);
        let mut step = serde_yaml::Mapping::new();
        let mut action = serde_yaml::Mapping::new();
        let mut assert_vis = serde_yaml::Mapping::new();
        assert_vis.insert(Value::String("selector".to_string()), selector);
        action.insert(
            Value::String("assertVisible".to_string()),
            Value::Mapping(assert_vis),
        );
        step.insert(Value::String("action".to_string()), Value::Mapping(action));
        Value::Mapping(step)
    }

    fn convert_assert_not_visible(&self, value: &Value) -> Value {
        let selector = selector_from_value(value);
        let mut step = serde_yaml::Mapping::new();
        let mut action = serde_yaml::Mapping::new();
        let mut assert_nv = serde_yaml::Mapping::new();
        assert_nv.insert(Value::String("selector".to_string()), selector);
        action.insert(
            Value::String("assertNotVisible".to_string()),
            Value::Mapping(assert_nv),
        );
        step.insert(Value::String("action".to_string()), Value::Mapping(action));
        Value::Mapping(step)
    }

    fn convert_input_text(
        &self,
        key: &str,
        value: &Value,
        line: usize,
        issues: &mut Vec<MigrationIssue>,
    ) -> Value {
        if key == "inputRandomText" {
            issues.push(MigrationIssue {
                line,
                severity: Severity::Info,
                message: "inputRandomText converted to inputText with placeholder".to_string(),
                maestro_construct: key.to_string(),
            });
        }

        let text = value.as_str().unwrap_or("TODO: set text");
        let mut step = serde_yaml::Mapping::new();
        let mut action = serde_yaml::Mapping::new();
        let mut input = serde_yaml::Mapping::new();
        // Maestro inputText targets the focused element; Velocity needs a selector
        let mut selector = serde_yaml::Mapping::new();
        selector.insert(
            Value::String("className".to_string()),
            Value::String("TextField".to_string()),
        );
        input.insert(
            Value::String("selector".to_string()),
            Value::Mapping(selector),
        );
        input.insert(
            Value::String("text".to_string()),
            Value::String(text.to_string()),
        );
        action.insert(
            Value::String("inputText".to_string()),
            Value::Mapping(input),
        );
        step.insert(Value::String("action".to_string()), Value::Mapping(action));
        Value::Mapping(step)
    }

    fn convert_clear_state(&self, value: &Value) -> Value {
        let app_id = value.as_str().unwrap_or("com.example.app");
        let mut step = serde_yaml::Mapping::new();
        let mut action = serde_yaml::Mapping::new();
        let mut launch = serde_yaml::Mapping::new();
        launch.insert(
            Value::String("appId".to_string()),
            Value::String(app_id.to_string()),
        );
        launch.insert(Value::String("clearState".to_string()), Value::Bool(true));
        action.insert(
            Value::String("launchApp".to_string()),
            Value::Mapping(launch),
        );
        step.insert(Value::String("action".to_string()), Value::Mapping(action));
        Value::Mapping(step)
    }

    fn convert_launch_app(&self, value: &Value) -> Value {
        let app_id = match value {
            Value::String(s) => s.clone(),
            Value::Mapping(m) => m
                .get(Value::String("appId".to_string()))
                .and_then(|v| v.as_str())
                .unwrap_or("com.example.app")
                .to_string(),
            _ => "com.example.app".to_string(),
        };
        let mut step = serde_yaml::Mapping::new();
        let mut action = serde_yaml::Mapping::new();
        let mut launch = serde_yaml::Mapping::new();
        launch.insert(Value::String("appId".to_string()), Value::String(app_id));
        launch.insert(Value::String("clearState".to_string()), Value::Bool(false));
        action.insert(
            Value::String("launchApp".to_string()),
            Value::Mapping(launch),
        );
        step.insert(Value::String("action".to_string()), Value::Mapping(action));
        Value::Mapping(step)
    }

    fn convert_scroll_until_visible(&self, value: &Value) -> Value {
        let mut step = serde_yaml::Mapping::new();
        let mut action = serde_yaml::Mapping::new();
        let mut scroll = serde_yaml::Mapping::new();

        if let Value::Mapping(m) = value {
            if let Some(element) = m.get(Value::String("element".to_string())) {
                let selector = selector_from_value(element);
                scroll.insert(Value::String("selector".to_string()), selector);
            }
            let dir = m
                .get(Value::String("direction".to_string()))
                .and_then(|v| v.as_str())
                .unwrap_or("down");
            scroll.insert(
                Value::String("direction".to_string()),
                Value::String(dir.to_string()),
            );
            let max = m
                .get(Value::String("maxScrolls".to_string()))
                .and_then(|v| v.as_u64())
                .unwrap_or(10);
            scroll.insert(
                Value::String("maxScrolls".to_string()),
                Value::Number(serde_yaml::Number::from(max)),
            );
        }

        action.insert(
            Value::String("scrollUntilVisible".to_string()),
            Value::Mapping(scroll),
        );
        step.insert(Value::String("action".to_string()), Value::Mapping(action));
        Value::Mapping(step)
    }

    fn convert_back(&self) -> Value {
        let mut step = serde_yaml::Mapping::new();
        let mut action = serde_yaml::Mapping::new();
        let mut press = serde_yaml::Mapping::new();
        press.insert(
            Value::String("key".to_string()),
            Value::String("back".to_string()),
        );
        action.insert(Value::String("pressKey".to_string()), Value::Mapping(press));
        step.insert(Value::String("action".to_string()), Value::Mapping(action));
        Value::Mapping(step)
    }

    fn convert_swipe(&self, value: &Value, line: usize, issues: &mut Vec<MigrationIssue>) -> Value {
        let mut step = serde_yaml::Mapping::new();
        let mut action = serde_yaml::Mapping::new();
        let mut swipe = serde_yaml::Mapping::new();

        match value {
            Value::String(dir) => {
                swipe.insert(
                    Value::String("direction".to_string()),
                    Value::String(dir.to_lowercase()),
                );
            }
            Value::Mapping(m) => {
                if let Some(dir) = m.get(Value::String("direction".to_string())) {
                    swipe.insert(Value::String("direction".to_string()), dir.clone());
                }
                // Maestro coordinate-based swipes use start/end
                if m.get(Value::String("start".to_string())).is_some()
                    || m.get(Value::String("end".to_string())).is_some()
                {
                    issues.push(MigrationIssue {
                        line,
                        severity: Severity::Warning,
                        message: "Coordinate-based swipe simplified to direction-based swipe"
                            .to_string(),
                        maestro_construct: "swipe".to_string(),
                    });
                    if swipe.get(Value::String("direction".to_string())).is_none() {
                        swipe.insert(
                            Value::String("direction".to_string()),
                            Value::String("up".to_string()),
                        );
                    }
                }
            }
            _ => {
                swipe.insert(
                    Value::String("direction".to_string()),
                    Value::String("up".to_string()),
                );
            }
        }

        action.insert(Value::String("swipe".to_string()), Value::Mapping(swipe));
        step.insert(Value::String("action".to_string()), Value::Mapping(action));
        Value::Mapping(step)
    }

    fn convert_screenshot(&self, value: &Value) -> Value {
        let filename = value.as_str().map(|s| s.to_string());
        let mut step = serde_yaml::Mapping::new();
        let mut action = serde_yaml::Mapping::new();
        let mut ss = serde_yaml::Mapping::new();
        if let Some(f) = filename {
            ss.insert(Value::String("filename".to_string()), Value::String(f));
        }
        action.insert(Value::String("screenshot".to_string()), Value::Mapping(ss));
        step.insert(Value::String("action".to_string()), Value::Mapping(action));
        Value::Mapping(step)
    }

    fn convert_run_flow(
        &self,
        value: &Value,
        line: usize,
        issues: &mut Vec<MigrationIssue>,
    ) -> Value {
        // Maestro runFlow references a YAML file path. Extract a flow ID from
        // the filename so the migrated output references a Velocity flow that
        // the user can define in their suite's `flows:` section.
        let file_path = match value {
            Value::String(s) => s.clone(),
            Value::Mapping(m) => m
                .get(Value::String("file".to_string()))
                .or_else(|| m.get(Value::String("path".to_string())))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string(),
            _ => "unknown".to_string(),
        };

        let flow_id = Path::new(&file_path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown_flow")
            .to_string();

        issues.push(MigrationIssue {
            line,
            severity: Severity::Info,
            message: format!(
                "runFlow '{file_path}' converted to RunFlow reference '{flow_id}'. \
                 Ensure a matching flow is defined in the suite's flows section."
            ),
            maestro_construct: "runFlow".to_string(),
        });

        let mut step = serde_yaml::Mapping::new();
        let mut action = serde_yaml::Mapping::new();
        let mut run_flow = serde_yaml::Mapping::new();
        run_flow.insert(Value::String("flow_id".to_string()), Value::String(flow_id));
        action.insert(
            Value::String("runFlow".to_string()),
            Value::Mapping(run_flow),
        );
        step.insert(Value::String("action".to_string()), Value::Mapping(action));
        Value::Mapping(step)
    }

    fn convert_press_key(&self, value: &Value) -> Value {
        let key = match value {
            Value::String(s) => s.clone(),
            Value::Mapping(m) => m
                .get(Value::String("key".to_string()))
                .and_then(|v| v.as_str())
                .unwrap_or("enter")
                .to_string(),
            _ => "enter".to_string(),
        };
        let mut step = serde_yaml::Mapping::new();
        let mut action = serde_yaml::Mapping::new();
        let mut press = serde_yaml::Mapping::new();
        press.insert(Value::String("key".to_string()), Value::String(key));
        action.insert(Value::String("pressKey".to_string()), Value::Mapping(press));
        step.insert(Value::String("action".to_string()), Value::Mapping(action));
        Value::Mapping(step)
    }
}

impl Default for MaestroMigrator {
    fn default() -> Self {
        Self::new()
    }
}

fn extract_selector(value: &Value, line: usize, issues: &mut Vec<MigrationIssue>) -> Value {
    match value {
        Value::String(text) => {
            let mut sel = serde_yaml::Mapping::new();
            sel.insert(
                Value::String("text".to_string()),
                Value::String(text.clone()),
            );
            Value::Mapping(sel)
        }
        Value::Mapping(m) => {
            let mut sel = serde_yaml::Mapping::new();
            if let Some(id) = m.get(Value::String("id".to_string())) {
                sel.insert(Value::String("id".to_string()), id.clone());
            } else if let Some(text) = m.get(Value::String("text".to_string())) {
                sel.insert(Value::String("text".to_string()), text.clone());
            } else if let Some(aid) = m.get(Value::String("accessibilityId".to_string())) {
                sel.insert(Value::String("accessibilityId".to_string()), aid.clone());
            } else {
                issues.push(MigrationIssue {
                    line,
                    severity: Severity::Warning,
                    message: "Could not determine selector type; using first key as text"
                        .to_string(),
                    maestro_construct: "tapOn".to_string(),
                });
                if let Some((_, v)) = m.iter().next() {
                    sel.insert(Value::String("text".to_string()), v.clone());
                }
            }
            Value::Mapping(sel)
        }
        _ => {
            let mut sel = serde_yaml::Mapping::new();
            sel.insert(
                Value::String("text".to_string()),
                Value::String("TODO".to_string()),
            );
            Value::Mapping(sel)
        }
    }
}

fn selector_from_value(value: &Value) -> Value {
    match value {
        Value::String(text) => {
            let mut sel = serde_yaml::Mapping::new();
            sel.insert(
                Value::String("text".to_string()),
                Value::String(text.clone()),
            );
            Value::Mapping(sel)
        }
        Value::Mapping(m) => {
            let mut sel = serde_yaml::Mapping::new();
            if let Some(id) = m.get(Value::String("id".to_string())) {
                sel.insert(Value::String("id".to_string()), id.clone());
            } else if let Some(text) = m.get(Value::String("text".to_string())) {
                sel.insert(Value::String("text".to_string()), text.clone());
            } else if let Some(aid) = m.get(Value::String("accessibilityId".to_string())) {
                sel.insert(Value::String("accessibilityId".to_string()), aid.clone());
            } else if let Some((k, v)) = m.iter().next() {
                sel.insert(k.clone(), v.clone());
            }
            Value::Mapping(sel)
        }
        other => other.clone(),
    }
}

fn build_velocity_yaml(app_id: &str, steps: &[Value]) -> String {
    let mut doc = serde_yaml::Mapping::new();
    doc.insert(
        Value::String("appId".to_string()),
        Value::String(app_id.to_string()),
    );

    let mut config = serde_yaml::Mapping::new();
    let mut sync = serde_yaml::Mapping::new();
    sync.insert(
        Value::String("interval_ms".to_string()),
        Value::Number(serde_yaml::Number::from(200u64)),
    );
    sync.insert(
        Value::String("stability_count".to_string()),
        Value::Number(serde_yaml::Number::from(3u64)),
    );
    config.insert(Value::String("sync".to_string()), Value::Mapping(sync));
    doc.insert(Value::String("config".to_string()), Value::Mapping(config));

    let mut test = serde_yaml::Mapping::new();
    test.insert(
        Value::String("name".to_string()),
        Value::String("migrated_test".to_string()),
    );
    test.insert(
        Value::String("tags".to_string()),
        Value::Sequence(vec![Value::String("migrated".to_string())]),
    );
    test.insert(
        Value::String("steps".to_string()),
        Value::Sequence(steps.to_vec()),
    );
    doc.insert(
        Value::String("tests".to_string()),
        Value::Sequence(vec![Value::Mapping(test)]),
    );

    serde_yaml::to_string(&Value::Mapping(doc)).unwrap_or_default()
}
