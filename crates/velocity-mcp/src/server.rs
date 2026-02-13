use std::io::{self, BufRead, Write};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use tracing::debug;
use velocity_common::{PlatformDriver, Result, VelocityError};

use crate::session::McpSession;
use crate::tool_registry::ToolDefinition;
use crate::tools::{device, flow, interaction, query};

#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    jsonrpc: String,
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Debug, Serialize)]
struct JsonRpcResponse {
    jsonrpc: String,
    id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
struct JsonRpcError {
    code: i32,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
}

pub struct McpServer {
    driver: Arc<dyn PlatformDriver>,
    session: McpSession,
}

impl McpServer {
    pub fn new(
        driver: Arc<dyn PlatformDriver>,
        device_id: String,
        config_path: Option<String>,
    ) -> Self {
        Self {
            driver,
            session: McpSession::new(Some(device_id), config_path),
        }
    }

    /// Initialize structured logging to stderr (critical: stdout is for MCP protocol only).
    pub fn init_logging() {
        use tracing_subscriber::{fmt, EnvFilter};
        let _ = fmt::fmt()
            .with_writer(std::io::stderr)
            .with_env_filter(
                EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn")),
            )
            .with_target(false)
            .try_init();
    }

    pub async fn run_stdio(&mut self) -> Result<()> {
        let stdin = io::stdin();
        let mut stdout = io::stdout();

        for line in stdin.lock().lines() {
            let line = line.map_err(|e| {
                VelocityError::Internal(anyhow::anyhow!("Failed to read stdin: {e}"))
            })?;

            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            debug!("Received: {line}");

            let request: JsonRpcRequest = match serde_json::from_str(line) {
                Ok(req) => req,
                Err(e) => {
                    let resp = JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id: Value::Null,
                        result: None,
                        error: Some(JsonRpcError {
                            code: -32700,
                            message: format!("Parse error: {e}"),
                            data: None,
                        }),
                    };
                    write_response(&mut stdout, &resp)?;
                    continue;
                }
            };

            if request.jsonrpc != "2.0" {
                let resp = JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: request.id.unwrap_or(Value::Null),
                    result: None,
                    error: Some(JsonRpcError {
                        code: -32600,
                        message: "Invalid JSON-RPC version".to_string(),
                        data: None,
                    }),
                };
                write_response(&mut stdout, &resp)?;
                continue;
            }

            let id = request.id.clone().unwrap_or(Value::Null);
            let response = self.dispatch(&request).await;

            let resp = match response {
                Ok(result) => JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id,
                    result: Some(result),
                    error: None,
                },
                Err(e) => JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id,
                    result: None,
                    error: Some(JsonRpcError {
                        code: -32000,
                        message: e.to_string(),
                        data: None,
                    }),
                },
            };

            write_response(&mut stdout, &resp)?;
        }

        Ok(())
    }

    async fn dispatch(&mut self, request: &JsonRpcRequest) -> Result<Value> {
        match request.method.as_str() {
            "initialize" => self.handle_initialize(),
            "tools/list" => self.handle_tools_list(),
            "tools/call" => self.handle_tools_call(&request.params).await,
            other => Err(VelocityError::Internal(anyhow::anyhow!(
                "Unknown method: {other}"
            ))),
        }
    }

    fn handle_initialize(&self) -> Result<Value> {
        Ok(serde_json::json!({
            "protocolVersion": "2024-11-05",
            "capabilities": {
                "tools": {}
            },
            "serverInfo": {
                "name": "velocity-mcp",
                "version": env!("CARGO_PKG_VERSION")
            }
        }))
    }

    fn handle_tools_list(&self) -> Result<Value> {
        let tools = self.tool_definitions();
        Ok(serde_json::to_value(tools).map_err(|e| {
            VelocityError::Internal(anyhow::anyhow!("Serialization error: {e}"))
        })?)
    }

    async fn handle_tools_call(&mut self, params: &Value) -> Result<Value> {
        let name = params
            .get("name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| VelocityError::Config("Missing tool name in params".to_string()))?;

        let arguments = params
            .get("arguments")
            .cloned()
            .unwrap_or(Value::Object(serde_json::Map::new()));

        debug!("Calling tool: {name}");

        let device_id = self
            .session
            .resolve_device_id(&*self.driver)
            .await
            .unwrap_or_default();
        let config_path = self.session.config_path.clone();

        let result = match name {
            "list_devices" => {
                let devices = self.session.get_devices(&*self.driver).await?;
                Ok(serde_json::to_value(devices).unwrap_or(Value::Null))
            }
            "screenshot" => {
                device::screenshot(&self.driver, &device_id, &arguments).await
            }
            "tap" => {
                interaction::tap(&self.driver, &device_id, &arguments).await
            }
            "type_text" => {
                interaction::type_text(&self.driver, &device_id, &arguments).await
            }
            "swipe" => {
                interaction::swipe(&self.driver, &device_id, &arguments).await
            }
            "press_key" => {
                interaction::press_key(&self.driver, &device_id, &arguments).await
            }
            "list_elements" => {
                query::list_elements(&self.driver, &device_id, &arguments).await
            }
            "get_element" => {
                query::get_element(&self.driver, &device_id, &arguments).await
            }
            "assert_visible" => {
                query::assert_visible(&self.driver, &device_id, &arguments).await
            }
            "list_flows" => flow::list_flows(&config_path, &arguments),
            "run_flow" => {
                flow::run_flow(&self.driver, &device_id, &config_path, &arguments)
                    .await
            }
            "list_tests" => flow::list_tests(&config_path, &arguments),
            "run_test" => {
                flow::run_test(&self.driver, &device_id, &config_path, &arguments)
                    .await
            }
            "generate_test_skeleton" => flow::generate_test_skeleton(&arguments),
            other => Err(VelocityError::Internal(anyhow::anyhow!(
                "Unknown tool: {other}"
            ))),
        }?;

        Ok(serde_json::json!({
            "content": [{
                "type": "text",
                "text": serde_json::to_string_pretty(&result)
                    .unwrap_or_else(|_| result.to_string())
            }]
        }))
    }

    fn tool_definitions(&self) -> Vec<ToolDefinition> {
        vec![
            ToolDefinition {
                name: "list_devices".to_string(),
                description: "List available devices (simulators/emulators)".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {}
                }),
            },
            ToolDefinition {
                name: "screenshot".to_string(),
                description: "Take a screenshot and return base64 PNG".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {}
                }),
            },
            ToolDefinition {
                name: "tap".to_string(),
                description: "Tap on an element matching the selector".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "selector": {
                            "type": "object",
                            "description": "Selector to find the element (e.g. {\"text\": \"Login\"} or {\"id\": \"btn_submit\"})"
                        }
                    },
                    "required": ["selector"]
                }),
            },
            ToolDefinition {
                name: "type_text".to_string(),
                description: "Type text into an element matching the selector".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "selector": {
                            "type": "object",
                            "description": "Selector to find the element"
                        },
                        "text": {
                            "type": "string",
                            "description": "Text to type"
                        }
                    },
                    "required": ["selector", "text"]
                }),
            },
            ToolDefinition {
                name: "swipe".to_string(),
                description: "Swipe in a direction on the screen".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "direction": {
                            "type": "string",
                            "enum": ["up", "down", "left", "right"],
                            "description": "Swipe direction"
                        }
                    },
                    "required": ["direction"]
                }),
            },
            ToolDefinition {
                name: "press_key".to_string(),
                description: "Press a hardware key".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "key": {
                            "type": "string",
                            "enum": ["back", "home", "enter", "volumeUp", "volumeDown"],
                            "description": "Key to press"
                        }
                    },
                    "required": ["key"]
                }),
            },
            ToolDefinition {
                name: "list_elements".to_string(),
                description: "List accessibility tree elements, optionally filtered".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "filter": {
                            "type": "string",
                            "description": "Optional text filter to match against element labels and text"
                        }
                    }
                }),
            },
            ToolDefinition {
                name: "get_element".to_string(),
                description: "Find a specific element by selector".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "selector": {
                            "type": "object",
                            "description": "Selector to find the element"
                        }
                    },
                    "required": ["selector"]
                }),
            },
            ToolDefinition {
                name: "assert_visible".to_string(),
                description: "Assert that an element matching the selector is visible".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "selector": {
                            "type": "object",
                            "description": "Selector to check visibility for"
                        }
                    },
                    "required": ["selector"]
                }),
            },
            ToolDefinition {
                name: "list_flows".to_string(),
                description: "List available flows from the test config".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {}
                }),
            },
            ToolDefinition {
                name: "run_flow".to_string(),
                description: "Execute a named flow by ID".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "id": {
                            "type": "string",
                            "description": "Flow ID to execute"
                        }
                    },
                    "required": ["id"]
                }),
            },
            ToolDefinition {
                name: "list_tests".to_string(),
                description: "List available tests from the test config".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {}
                }),
            },
            ToolDefinition {
                name: "run_test".to_string(),
                description: "Execute a named test".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "name": {
                            "type": "string",
                            "description": "Test name to execute"
                        }
                    },
                    "required": ["name"]
                }),
            },
            ToolDefinition {
                name: "generate_test_skeleton".to_string(),
                description: "Generate a YAML test skeleton from a description".to_string(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "description": {
                            "type": "string",
                            "description": "Natural language description of the test to generate"
                        },
                        "app_id": {
                            "type": "string",
                            "description": "Application bundle/package ID (e.g. com.mycompany.myapp). Defaults to com.example.app if omitted."
                        }
                    },
                    "required": ["description"]
                }),
            },
        ]
    }
}

fn write_response(stdout: &mut io::Stdout, resp: &JsonRpcResponse) -> Result<()> {
    let json = serde_json::to_string(resp)
        .map_err(|e| VelocityError::Internal(anyhow::anyhow!("Serialization error: {e}")))?;
    debug!("Sending: {json}");
    writeln!(stdout, "{json}").map_err(|e| {
        VelocityError::Internal(anyhow::anyhow!("Failed to write stdout: {e}"))
    })?;
    stdout.flush().map_err(|e| {
        VelocityError::Internal(anyhow::anyhow!("Failed to flush stdout: {e}"))
    })?;
    Ok(())
}
