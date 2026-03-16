use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::response::IntoResponse;

use crate::state::AppState;

pub async fn get(
    State(state): State<Arc<AppState>>,
    Path(device_id): Path<String>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let png_bytes = state
        .driver
        .screenshot(&device_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok((
        [
            (header::CONTENT_TYPE, "image/png"),
            (header::CACHE_CONTROL, "no-cache"),
        ],
        png_bytes,
    ))
}
