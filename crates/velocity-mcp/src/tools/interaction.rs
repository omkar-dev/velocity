use std::sync::Arc;

use serde_json::Value;
use velocity_common::{Direction, Key, PlatformDriver, Result, Selector, VelocityError};

fn parse_selector(args: &Value) -> Result<Selector> {
    let selector_val = args
        .get("selector")
        .ok_or_else(|| VelocityError::Config("Missing 'selector' argument".to_string()))?;

    serde_json::from_value(selector_val.clone())
        .map_err(|e| VelocityError::Config(format!("Invalid selector: {e}")))
}

pub async fn tap(
    driver: &Arc<dyn PlatformDriver>,
    device_id: &str,
    args: &Value,
) -> Result<Value> {
    let selector = parse_selector(args)?;
    let element = driver.find_element(device_id, &selector).await?;
    driver.tap(device_id, &element).await?;
    Ok(serde_json::json!({
        "action": "tap",
        "selector": selector.to_string(),
        "element": {
            "label": element.label,
            "text": element.text,
            "bounds": element.bounds
        }
    }))
}

pub async fn type_text(
    driver: &Arc<dyn PlatformDriver>,
    device_id: &str,
    args: &Value,
) -> Result<Value> {
    let selector = parse_selector(args)?;
    let text = args
        .get("text")
        .and_then(|v| v.as_str())
        .ok_or_else(|| VelocityError::Config("Missing 'text' argument".to_string()))?;

    let element = driver.find_element(device_id, &selector).await?;
    driver.input_text(device_id, &element, text).await?;
    Ok(serde_json::json!({
        "action": "type_text",
        "selector": selector.to_string(),
        "text": text,
        "element": {
            "label": element.label,
            "text": element.text
        }
    }))
}

pub async fn swipe(
    driver: &Arc<dyn PlatformDriver>,
    device_id: &str,
    args: &Value,
) -> Result<Value> {
    let direction_str = args
        .get("direction")
        .and_then(|v| v.as_str())
        .ok_or_else(|| VelocityError::Config("Missing 'direction' argument".to_string()))?;

    let direction: Direction = serde_json::from_value(Value::String(direction_str.to_string()))
        .map_err(|e| {
            VelocityError::Config(format!(
                "Invalid direction '{direction_str}': {e}. Use: up, down, left, right"
            ))
        })?;

    driver.swipe(device_id, direction).await?;
    Ok(serde_json::json!({
        "action": "swipe",
        "direction": direction_str
    }))
}

pub async fn press_key(
    driver: &Arc<dyn PlatformDriver>,
    device_id: &str,
    args: &Value,
) -> Result<Value> {
    let key_str = args
        .get("key")
        .and_then(|v| v.as_str())
        .ok_or_else(|| VelocityError::Config("Missing 'key' argument".to_string()))?;

    let key: Key = serde_json::from_value(Value::String(key_str.to_string())).map_err(|e| {
        VelocityError::Config(format!(
            "Invalid key '{key_str}': {e}. Use: back, home, enter, volumeUp, volumeDown"
        ))
    })?;

    driver.press_key(device_id, key).await?;
    Ok(serde_json::json!({
        "action": "press_key",
        "key": key_str
    }))
}
