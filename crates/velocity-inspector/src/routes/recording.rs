use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::yaml_gen;

#[derive(Deserialize)]
pub struct SaveFlowRequest {
    pub name: String,
    pub app_id: String,
    pub steps: Vec<String>,
    pub path: Option<String>,
}

#[derive(Serialize)]
pub struct SaveFlowResponse {
    pub path: String,
    pub yaml: String,
}

pub async fn save_flow(
    Json(req): Json<SaveFlowRequest>,
) -> Result<Json<SaveFlowResponse>, (StatusCode, String)> {
    if req.steps.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "No steps to save".to_string()));
    }

    let yaml = yaml_gen::flow_yaml(&req.name, &req.app_id, &req.steps);

    // Determine output path
    let sanitized_name = req
        .name
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
        .collect::<String>();
    let path = req
        .path
        .unwrap_or_else(|| format!("{sanitized_name}.yaml"));

    if Path::new(&path).is_absolute() || path.split('/').any(|part| part == "..") {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("Invalid output path: {path}"),
        ));
    }

    let base_dir = std::env::current_dir()
        .and_then(|dir| dir.canonicalize())
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to resolve output directory: {e}"),
            )
        })?;
    let requested_path = PathBuf::from(&path);
    let joined_path = base_dir.join(&requested_path);
    let parent_dir = joined_path.parent().unwrap_or(&base_dir);
    std::fs::create_dir_all(parent_dir).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to create output directory {}: {e}", parent_dir.display()),
        )
    })?;
    let canonical_parent = parent_dir.canonicalize().map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to resolve output directory {}: {e}", parent_dir.display()),
        )
    })?;
    let final_path = canonical_parent.join(
        joined_path
            .file_name()
            .ok_or_else(|| (StatusCode::BAD_REQUEST, format!("Invalid output path: {path}")))?,
    );

    if !final_path.starts_with(&base_dir) {
        return Err((
            StatusCode::BAD_REQUEST,
            format!("Output path escapes workspace: {path}"),
        ));
    }

    // Write the file
    std::fs::write(&final_path, &yaml).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to write {}: {e}", final_path.display()),
        )
    })?;

    let output_path = final_path.display().to_string();

    tracing::info!(path = %output_path, steps = req.steps.len(), "saved recorded flow");

    Ok(Json(SaveFlowResponse {
        path: output_path,
        yaml,
    }))
}
