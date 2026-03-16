use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};
use velocity_common::types::Element;

use crate::selector_gen;
use crate::yaml_gen;

#[derive(Deserialize)]
pub struct GenerateRequest {
    pub element: Element,
}

#[derive(Serialize)]
pub struct GenerateResponse {
    pub selector: velocity_common::types::Selector,
    pub yaml_tap: String,
    pub yaml_assert: String,
}

pub async fn generate(
    Json(req): Json<GenerateRequest>,
) -> Result<Json<GenerateResponse>, (StatusCode, String)> {
    let selector = selector_gen::generate_selector(&req.element);
    let yaml_tap = yaml_gen::tap_yaml(&selector);
    let yaml_assert = yaml_gen::assert_visible_yaml(&selector);

    Ok(Json(GenerateResponse {
        selector,
        yaml_tap,
        yaml_assert,
    }))
}
