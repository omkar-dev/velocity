use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::Deserialize;
use velocity_common::types::{Direction, Key, Selector};

use crate::state::AppState;

#[derive(Deserialize)]
pub struct TapRequest {
    pub selector: Option<Selector>,
    pub coordinates: Option<(i32, i32)>,
}

#[derive(Deserialize)]
pub struct TypeTextRequest {
    pub selector: Selector,
    pub text: String,
}

#[derive(Deserialize)]
pub struct SwipeRequest {
    pub direction: Direction,
}

#[derive(Deserialize)]
pub struct PressKeyRequest {
    pub key: Key,
}

pub async fn tap(
    State(state): State<Arc<AppState>>,
    Path(device_id): Path<String>,
    Json(req): Json<TapRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    if let Some((x, y)) = req.coordinates {
        // Tap by coordinates: create a synthetic element with the given bounds
        let element = velocity_common::types::Element {
            platform_id: String::new(),
            label: None,
            text: None,
            element_type: String::new(),
            bounds: velocity_common::types::Rect {
                x,
                y,
                width: 1,
                height: 1,
            },
            enabled: true,
            visible: true,
            children: vec![],
        };
        state
            .driver
            .tap(&device_id, &element)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    } else if let Some(ref selector) = req.selector {
        let element = state
            .driver
            .find_element(&device_id, selector)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        state
            .driver
            .tap(&device_id, &element)
            .await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    } else {
        return Err((
            StatusCode::BAD_REQUEST,
            "Either 'selector' or 'coordinates' must be provided".to_string(),
        ));
    }

    Ok(Json(serde_json::json!({ "ok": true })))
}

pub async fn type_text(
    State(state): State<Arc<AppState>>,
    Path(device_id): Path<String>,
    Json(req): Json<TypeTextRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let element = state
        .driver
        .find_element(&device_id, &req.selector)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    state
        .driver
        .input_text(&device_id, &element, &req.text)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(serde_json::json!({ "ok": true })))
}

pub async fn swipe(
    State(state): State<Arc<AppState>>,
    Path(device_id): Path<String>,
    Json(req): Json<SwipeRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    state
        .driver
        .swipe(&device_id, req.direction)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(serde_json::json!({ "ok": true })))
}

pub async fn press_key(
    State(state): State<Arc<AppState>>,
    Path(device_id): Path<String>,
    Json(req): Json<PressKeyRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    state
        .driver
        .press_key(&device_id, req.key)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(serde_json::json!({ "ok": true })))
}
