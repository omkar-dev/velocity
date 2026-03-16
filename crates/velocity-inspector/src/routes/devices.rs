use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use velocity_common::types::DeviceInfo;

use crate::state::AppState;

pub async fn list(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<DeviceInfo>>, (StatusCode, String)> {
    let devices = state
        .driver
        .list_devices()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(devices))
}

#[derive(Deserialize)]
pub struct SelectRequest {
    pub device_id: String,
}

pub async fn select(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SelectRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    // Verify device exists
    let devices = state
        .driver
        .list_devices()
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let device = devices
        .iter()
        .find(|d| d.id == req.device_id)
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                format!("Device '{}' not found", req.device_id),
            )
        })?;

    let info = serde_json::to_value(device)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    *state.current_device.write().await = Some(req.device_id);

    Ok(Json(info))
}
