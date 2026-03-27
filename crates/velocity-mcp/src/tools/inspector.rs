use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use velocity_common::{PlatformDriver, Result, VelocityError};
use velocity_inspector::InspectorServer;

use crate::session::McpSession;

const DEFAULT_PORT: u16 = 9876;
const MAX_PORT_ATTEMPTS: u16 = 10;

pub async fn open_inspector(
    driver: &Arc<dyn PlatformDriver>,
    device_id: &str,
    session: &mut McpSession,
    args: &Value,
) -> Result<Value> {
    // Idempotent: return existing URL if already running
    if let Some(port) = session.inspector_port {
        return Ok(serde_json::json!({
            "status": "already_running",
            "url": format!("http://localhost:{port}"),
            "port": port
        }));
    }

    let requested_port = args
        .get("port")
        .and_then(|v| v.as_u64())
        .map(|p| {
            if p > u16::MAX as u64 {
                Err(VelocityError::Config(format!(
                    "Requested inspector port {p} exceeds {}",
                    u16::MAX
                )))
            } else {
                Ok(p as u16)
            }
        })
        .transpose()?
        .unwrap_or(DEFAULT_PORT);

    // Find an available port
    let listener = find_available_port(requested_port).await.map_err(|_| {
        VelocityError::Config(format!(
            "No available port found in range {requested_port}-{}",
            requested_port + MAX_PORT_ATTEMPTS
        ))
    })?;
    let port = listener
        .local_addr()
        .map_err(|e| VelocityError::Internal(anyhow::anyhow!("Failed to inspect listener: {e}")))?
        .port();

    let driver_clone = driver.clone();
    let device_id_owned = device_id.to_string();
    let (ready_tx, ready_rx) = oneshot::channel();

    let handle = tokio::spawn(async move {
        let server = InspectorServer::new(driver_clone, Some(device_id_owned), None);
        let _ = ready_tx.send(Ok(port));
        if let Err(e) = server.start_with_listener(listener).await {
            tracing::error!("Inspector server exited with error: {e}");
        }
    });

    let ready_port = match tokio::time::timeout(Duration::from_secs(2), ready_rx).await {
        Ok(Ok(Ok(port))) => port,
        Ok(Ok(Err(err))) => {
            handle.abort();
            return Err(VelocityError::Config(err));
        }
        Ok(Err(_)) => {
            handle.abort();
            return Err(VelocityError::Internal(anyhow::anyhow!(
                "Inspector startup channel closed before readiness"
            )));
        }
        Err(_) => {
            handle.abort();
            return Err(VelocityError::Internal(anyhow::anyhow!(
                "Timed out waiting for inspector startup"
            )));
        }
    };

    session.inspector_port = Some(ready_port);
    session.inspector_handle = Some(handle);

    Ok(serde_json::json!({
        "status": "started",
        "url": format!("http://localhost:{ready_port}"),
        "port": ready_port
    }))
}

pub async fn close_inspector(
    session: &mut McpSession,
    _args: &Value,
) -> Result<Value> {
    if let Some(handle) = session.inspector_handle.take() {
        handle.abort();
        session.inspector_port = None;
        Ok(serde_json::json!({
            "status": "stopped"
        }))
    } else {
        Ok(serde_json::json!({
            "status": "not_running"
        }))
    }
}

async fn find_available_port(start: u16) -> std::result::Result<TcpListener, ()> {
    let end = start.saturating_add(MAX_PORT_ATTEMPTS.saturating_sub(1));
    for port in start..=end {
        if let Ok(listener) = TcpListener::bind(format!("127.0.0.1:{port}")).await {
            return Ok(listener);
        }
    }
    Err(())
}
