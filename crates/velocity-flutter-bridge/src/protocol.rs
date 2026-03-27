use serde::{Deserialize, Serialize};
use velocity_common::{Element, Rect};

/// Commands sent to the Flutter test process.
#[derive(Debug, Serialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum FlutterCommand {
    /// Boot the app widget.
    Init {
        target: String,
        width: u32,
        height: u32,
    },
    /// Get the current widget/semantics tree as Element JSON.
    GetHierarchy,
    /// Capture a screenshot as base64 PNG.
    Screenshot,
    /// Simulate a tap at coordinates.
    Tap { x: i32, y: i32 },
    /// Simulate a double-tap at coordinates.
    DoubleTap { x: i32, y: i32 },
    /// Simulate a long press at coordinates.
    LongPress { x: i32, y: i32, duration_ms: u64 },
    /// Simulate text input.
    InputText { text: String },
    /// Clear text from a focused field.
    ClearText,
    /// Simulate a drag/swipe.
    Swipe { from_x: i32, from_y: i32, to_x: i32, to_y: i32 },
    /// Simulate a key press.
    PressKey { key: String },
    /// Pump frames (Flutter-specific: advance animations).
    PumpFrames { count: u32 },
    /// Shut down.
    Shutdown,
}

/// Responses from the Flutter test process.
#[derive(Debug, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum FlutterResponse {
    Ok {
        #[serde(default)]
        data: Option<serde_json::Value>,
    },
    Error {
        message: String,
    },
}

/// Element data from Flutter's Semantics/RenderObject tree.
#[derive(Debug, Deserialize)]
pub struct FlutterElement {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(rename = "type")]
    pub element_type: String,
    pub bounds: FlutterRect,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub visible: bool,
    #[serde(default)]
    pub children: Vec<FlutterElement>,
}

fn default_true() -> bool { true }

#[derive(Debug, Deserialize)]
pub struct FlutterRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl FlutterElement {
    pub fn to_element(&self) -> Element {
        Element {
            platform_id: self.id.clone().unwrap_or_default(),
            label: self.label.clone(),
            text: self.text.clone(),
            element_type: self.element_type.clone(),
            bounds: Rect {
                x: self.bounds.x,
                y: self.bounds.y,
                width: self.bounds.width,
                height: self.bounds.height,
            },
            enabled: self.enabled,
            visible: self.visible,
            children: self.children.iter().map(|c| c.to_element()).collect(),
        }
    }
}
