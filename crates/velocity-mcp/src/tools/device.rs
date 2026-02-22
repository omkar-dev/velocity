use std::sync::Arc;

use base64::Engine;
use serde_json::Value;
use velocity_common::{PlatformDriver, Result, VelocityError};

pub async fn list_devices(
    driver: &Arc<dyn PlatformDriver>,
    _args: &Value,
) -> Result<Value> {
    let devices = driver.list_devices().await?;
    serde_json::to_value(&devices)
        .map_err(|e| VelocityError::Internal(anyhow::anyhow!("Serialization error: {e}")))
}

pub async fn screenshot(
    driver: &Arc<dyn PlatformDriver>,
    device_id: &str,
    _args: &Value,
) -> Result<Value> {
    let png_bytes = driver.screenshot(device_id).await?;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&png_bytes);
    Ok(serde_json::json!({
        "format": "png",
        "encoding": "base64",
        "data": b64,
        "size_bytes": png_bytes.len()
    }))
}
