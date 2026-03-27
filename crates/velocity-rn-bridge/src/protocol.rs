use serde::{Deserialize, Serialize};
use velocity_common::{Element, Rect};

/// Commands sent to the RN sidecar.
#[derive(Debug, Serialize)]
#[serde(tag = "cmd", rename_all = "snake_case")]
pub enum BridgeCommand {
    /// Initialize with JS bundle and component.
    Init {
        bundle_path: String,
        component: String,
        width: u32,
        height: u32,
        native_mocks: std::collections::HashMap<String, serde_json::Value>,
    },
    /// Get the current view hierarchy as Element tree.
    GetHierarchy,
    /// Take a screenshot, returned as base64 PNG.
    Screenshot,
    /// Simulate a tap at coordinates.
    Tap { x: i32, y: i32 },
    /// Simulate text input on a focused element.
    InputText { text: String },
    /// Simulate a swipe gesture.
    Swipe {
        from_x: i32,
        from_y: i32,
        to_x: i32,
        to_y: i32,
    },
    /// Navigate to a different component/screen.
    Navigate { component: String },
    /// Shut down the sidecar.
    Shutdown,
}

/// Responses from the RN sidecar.
#[derive(Debug, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum BridgeResponse {
    Ok {
        #[serde(default)]
        data: Option<serde_json::Value>,
    },
    Error {
        message: String,
    },
}

/// Element data returned from the sidecar (before conversion to velocity Element).
#[derive(Debug, Deserialize)]
pub struct SidecarElement {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub text: Option<String>,
    #[serde(rename = "type")]
    pub element_type: String,
    pub bounds: SidecarRect,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_true")]
    pub visible: bool,
    #[serde(default)]
    pub children: Vec<SidecarElement>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Deserialize)]
pub struct SidecarRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl SidecarElement {
    /// Convert to velocity-common Element.
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
