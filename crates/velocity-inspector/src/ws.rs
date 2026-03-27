use std::sync::Arc;
use std::time::Duration;

use axum::extract::ws::{Message, WebSocket};
use axum::extract::{Path, State, WebSocketUpgrade};
use axum::response::IntoResponse;
use tokio::time::interval;

use crate::state::AppState;

pub async fn handler(
    State(state): State<Arc<AppState>>,
    Path(device_id): Path<String>,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state, device_id))
}

async fn handle_socket(mut socket: WebSocket, state: Arc<AppState>, device_id: String) {
    let mut tick = interval(Duration::from_secs(1));

    loop {
        tokio::select! {
            _ = tick.tick() => {
                // Push screenshot URL notification
                let ts = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis();

                let screenshot_msg = serde_json::json!({
                    "type": "screenshot",
                    "url": format!("/api/devices/{}/screenshot?t={}", device_id, ts)
                });

                if socket.send(Message::Text(screenshot_msg.to_string().into())).await.is_err() {
                    break;
                }

                // Push hierarchy
                match state.driver.get_hierarchy(&device_id).await {
                    Ok(hierarchy) => {
                        let hierarchy_msg = serde_json::json!({
                            "type": "hierarchy",
                            "root": hierarchy
                        });
                        if socket.send(Message::Text(hierarchy_msg.to_string().into())).await.is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        let err_msg = serde_json::json!({
                            "type": "error",
                            "message": e.to_string()
                        });
                        let _ = socket.send(Message::Text(err_msg.to_string().into())).await;
                    }
                }

                // Push performance metrics if app_id is set (opt-in, no overhead when None)
                if let Some(app_id) = state.app_id().await {
                    if let Ok((java, native, pss, cpu)) =
                        state.driver.collect_resource_metrics(&device_id, &app_id).await
                    {
                        let perf_msg = serde_json::json!({
                            "type": "performance",
                            "javaHeapKb": java,
                            "nativeHeapKb": native,
                            "totalPssKb": pss,
                            "cpuPercent": cpu,
                        });
                        if socket.send(Message::Text(perf_msg.to_string().into())).await.is_err() {
                            break;
                        }
                    }
                }
            }

            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        // Handle client messages (e.g., refresh requests)
                        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&text) {
                            if parsed.get("type").and_then(|t| t.as_str()) == Some("refresh") {
                                // Force an immediate tick
                                tick.reset();
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    _ => {}
                }
            }
        }
    }
}
