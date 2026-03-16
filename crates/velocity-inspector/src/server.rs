use std::sync::Arc;

use axum::routing::{get, post};
use axum::Router;
use tower_http::cors::CorsLayer;

use crate::embedded;
use crate::routes::{actions, devices, hierarchy, screenshot, selector};
use crate::state::AppState;
use crate::ws;

pub fn build_router(state: Arc<AppState>) -> Router {
    let api = Router::new()
        .route("/devices", get(devices::list))
        .route("/devices/select", post(devices::select))
        .route("/devices/{id}/screenshot", get(screenshot::get))
        .route("/devices/{id}/hierarchy", get(hierarchy::get))
        .route("/devices/{id}/tap", post(actions::tap))
        .route("/devices/{id}/type", post(actions::type_text))
        .route("/devices/{id}/swipe", post(actions::swipe))
        .route("/devices/{id}/press-key", post(actions::press_key))
        .route("/selector/generate", post(selector::generate))
        .route("/ws/{device_id}", get(ws::handler));

    Router::new()
        .nest("/api", api)
        .fallback(get(embedded::static_handler))
        .layer(CorsLayer::permissive())
        .with_state(state)
}
