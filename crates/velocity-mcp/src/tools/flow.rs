use std::sync::Arc;

use serde_json::Value;
use velocity_common::{PlatformDriver, Result, TestSuite, VelocityError};
use velocity_core::{parse_suite, TestExecutor};

fn load_suite(config_path: &Option<String>) -> Result<TestSuite> {
    let path = config_path.as_deref().unwrap_or("velocity.yaml");
    parse_suite(path)
}

pub fn list_flows(config_path: &Option<String>, _args: &Value) -> Result<Value> {
    let suite = load_suite(config_path)?;
    let flows: Vec<Value> = suite
        .flows
        .iter()
        .map(|f| {
            serde_json::json!({
                "id": f.id,
                "step_count": f.steps.len()
            })
        })
        .collect();

    Ok(serde_json::json!({
        "count": flows.len(),
        "flows": flows
    }))
}

pub async fn run_flow(
    driver: &Arc<dyn PlatformDriver>,
    device_id: &str,
    config_path: &Option<String>,
    args: &Value,
) -> Result<Value> {
    let flow_id = args
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| VelocityError::Config("Missing 'id' argument".to_string()))?;

    let suite = load_suite(config_path)?;
    let flow = suite
        .flows
        .iter()
        .find(|f| f.id == flow_id)
        .ok_or_else(|| VelocityError::UnknownFlowRef {
            flow_id: flow_id.to_string(),
            test_name: "<mcp>".to_string(),
        })?;

    let app_id = &suite.app_id;
    let mut executor = TestExecutor::new(driver.as_ref(), suite.config.clone(), &suite.app_id);
    let mut results = Vec::new();
    for (i, step) in flow.steps.iter().enumerate() {
        match executor.execute_step(step, device_id, app_id, i).await {
            Ok(result) => {
                let passed = result.status == velocity_common::StepStatus::Passed;
                results.push(serde_json::json!({
                    "step": i,
                    "action": result.action_name,
                    "status": if passed { "passed" } else { "failed" },
                    "duration_ms": result.duration.as_millis()
                }));
                if !passed {
                    break;
                }
            }
            Err(e) => {
                results.push(serde_json::json!({
                    "step": i,
                    "error": e.to_string()
                }));
                break;
            }
        }
    }

    Ok(serde_json::json!({
        "flow_id": flow_id,
        "steps_executed": results.len(),
        "results": results
    }))
}

pub fn list_tests(config_path: &Option<String>, _args: &Value) -> Result<Value> {
    let suite = load_suite(config_path)?;
    let tests: Vec<Value> = suite
        .tests
        .iter()
        .map(|t| {
            serde_json::json!({
                "name": t.name,
                "tags": t.tags,
                "step_count": t.steps.len(),
                "isolated": t.isolated
            })
        })
        .collect();

    Ok(serde_json::json!({
        "count": tests.len(),
        "tests": tests
    }))
}

pub async fn run_test(
    driver: &Arc<dyn PlatformDriver>,
    device_id: &str,
    config_path: &Option<String>,
    args: &Value,
) -> Result<Value> {
    let test_name = args
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| VelocityError::Config("Missing 'name' argument".to_string()))?;

    let suite = load_suite(config_path)?;
    let test = suite
        .tests
        .iter()
        .find(|t| t.name == test_name)
        .ok_or_else(|| VelocityError::Config(format!("Test '{test_name}' not found in suite")))?;

    let app_id = &suite.app_id;
    let mut executor = TestExecutor::new(driver.as_ref(), suite.config.clone(), &suite.app_id);
    let result = executor.execute_test(test, device_id, app_id).await?;

    Ok(serde_json::json!({
        "test_name": result.test_name,
        "status": format!("{:?}", result.status).to_lowercase(),
        "duration_ms": result.duration.as_millis(),
        "steps": result.steps.len(),
        "retries": result.retries,
        "error": result.error_message
    }))
}

pub fn generate_test_skeleton(args: &Value) -> Result<Value> {
    let description = args
        .get("description")
        .and_then(|v| v.as_str())
        .unwrap_or("New test");

    let app_id = args
        .get("app_id")
        .and_then(|v| v.as_str())
        .unwrap_or("com.example.app");

    let sanitized_name = description
        .to_lowercase()
        .replace(|c: char| !c.is_alphanumeric() && c != ' ', "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("_");

    let skeleton = format!(
        r#"# Auto-generated test skeleton for: {description}
appId: {app_id}

config:
  sync:
    interval_ms: 200
    stability_count: 3

tests:
  - name: {sanitized_name}
    tags: [generated]
    steps:
      - action:
          launchApp:
            appId: {app_id}
            clearState: true
      # TODO: Add test steps based on: {description}
      - action:
          assertVisible:
            selector:
              text: "Expected Element"
"#
    );

    Ok(serde_json::json!({
        "description": description,
        "app_id": app_id,
        "test_name": sanitized_name,
        "yaml": skeleton
    }))
}
