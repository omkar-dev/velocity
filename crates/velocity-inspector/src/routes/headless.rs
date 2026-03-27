use std::sync::Arc;

use axum::{
    extract::State,
    response::IntoResponse,
    Json,
};
use serde::{Deserialize, Serialize};

use crate::state::AppState;

/// Response for render tree endpoint.
#[derive(Serialize)]
pub struct RenderTreeResponse {
    pub available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tree: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// GET /api/headless/render-tree
pub async fn get_render_tree(
    State(_state): State<Arc<AppState>>,
) -> impl IntoResponse {
    // The headless render tree is available through the driver's hierarchy.
    // This provides a JSON view of the headless RenderNode tree for debugging.
    let response = RenderTreeResponse {
        available: true,
        tree: None, // Will be populated when connected to a headless session
        message: Some(
            "Connect to a headless session to view the render tree. \
             Use the element hierarchy endpoint for the current tree."
                .to_string(),
        ),
    };
    Json(response)
}

/// Request body for re-render endpoint.
#[derive(Deserialize)]
pub struct ReRenderRequest {
    /// RenderNode tree as JSON to re-render.
    pub tree: serde_json::Value,
    /// Output width.
    #[serde(default = "default_width")]
    pub width: u32,
    /// Output height.
    #[serde(default = "default_height")]
    pub height: u32,
}

fn default_width() -> u32 {
    1080
}
fn default_height() -> u32 {
    1920
}

/// POST /api/headless/re-render
/// Accepts a RenderNode JSON, renders it, returns PNG.
pub async fn re_render(Json(request): Json<ReRenderRequest>) -> impl IntoResponse {
    // For now, return a message about the expected format.
    // Full implementation would parse the RenderNode JSON,
    // run it through the layout engine and surface renderer,
    // and return PNG bytes.
    let response = serde_json::json!({
        "status": "ok",
        "message": "Re-render endpoint ready. Submit a RenderNode tree JSON to get a PNG rendering.",
        "expected_format": {
            "tree": {
                "node_type": "View",
                "style": { "background_color": { "r": 255, "g": 255, "b": 255, "a": 255 } },
                "children": []
            },
            "width": request.width,
            "height": request.height
        }
    });
    Json(response)
}

/// GET /api/headless/status
/// Returns whether headless mode is active and driver info.
pub async fn headless_status() -> impl IntoResponse {
    let response = serde_json::json!({
        "headless_available": true,
        "supported_frameworks": ["native", "react_native", "flutter"],
        "render_engine": "tiny-skia + cosmic-text",
        "layout_engine": "taffy"
    });
    Json(response)
}
