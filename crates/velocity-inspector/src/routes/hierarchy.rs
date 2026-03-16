use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use velocity_common::types::Element;

use crate::state::AppState;

pub async fn get(
    State(state): State<Arc<AppState>>,
    Path(device_id): Path<String>,
) -> Result<Json<Element>, (StatusCode, String)> {
    let hierarchy = state
        .driver
        .get_hierarchy(&device_id)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    Ok(Json(hierarchy))
}
