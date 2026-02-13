use serde::Serialize;
use serde_json::Value;

/// Structured response from MCP tools, optimized for LLM consumption.
#[derive(Debug, Serialize)]
pub struct ToolResponse {
    /// Concise summary for LLM context window.
    pub summary: String,

    /// Full structured data if needed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,

    /// Suggested next actions for the LLM.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub next_steps: Vec<String>,
}

impl ToolResponse {
    pub fn success(summary: impl Into<String>) -> Self {
        Self {
            summary: summary.into(),
            data: None,
            next_steps: Vec::new(),
        }
    }

    pub fn with_data(mut self, data: Value) -> Self {
        self.data = Some(data);
        self
    }

    pub fn with_next_steps(mut self, steps: Vec<String>) -> Self {
        self.next_steps = steps;
        self
    }

    /// Convert to MCP content format.
    pub fn into_mcp_content(self) -> Value {
        let text = if let Some(ref data) = self.data {
            format!(
                "{}\n\n{}",
                self.summary,
                serde_json::to_string_pretty(data).unwrap_or_default()
            )
        } else {
            self.summary.clone()
        };

        let mut content = serde_json::json!({
            "content": [{
                "type": "text",
                "text": text
            }]
        });

        if !self.next_steps.is_empty() {
            content["next_steps"] = serde_json::to_value(&self.next_steps).unwrap_or_default();
        }

        content
    }
}

/// Definition of a tool for MCP tools/list.
#[derive(Debug, Clone, Serialize)]
pub struct ToolDefinition {
    pub name: String,
    pub description: String,
    #[serde(rename = "inputSchema")]
    pub input_schema: Value,
}
