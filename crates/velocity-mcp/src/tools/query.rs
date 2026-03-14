use std::sync::Arc;

use serde_json::Value;
use velocity_common::{Element, PlatformDriver, Result, Selector, VelocityError};

fn flatten_elements(root: &Element, out: &mut Vec<Value>, filter: Option<&str>) {
    let matches_filter = filter.is_none_or(|f| {
        let f_lower = f.to_lowercase();
        root.label
            .as_deref()
            .is_some_and(|l| l.to_lowercase().contains(&f_lower))
            || root
                .text
                .as_deref()
                .is_some_and(|t| t.to_lowercase().contains(&f_lower))
            || root.element_type.to_lowercase().contains(&f_lower)
    });

    if matches_filter {
        out.push(serde_json::json!({
            "platform_id": root.platform_id,
            "label": root.label,
            "text": root.text,
            "element_type": root.element_type,
            "bounds": root.bounds,
            "enabled": root.enabled,
            "visible": root.visible
        }));
    }

    for child in &root.children {
        flatten_elements(child, out, filter);
    }
}

pub async fn list_elements(
    driver: &Arc<dyn PlatformDriver>,
    device_id: &str,
    args: &Value,
) -> Result<Value> {
    let filter = args.get("filter").and_then(|v| v.as_str());
    let hierarchy = driver.get_hierarchy(device_id).await?;
    let mut elements = Vec::new();
    flatten_elements(&hierarchy, &mut elements, filter);
    Ok(serde_json::json!({
        "count": elements.len(),
        "elements": elements
    }))
}

pub async fn get_element(
    driver: &Arc<dyn PlatformDriver>,
    device_id: &str,
    args: &Value,
) -> Result<Value> {
    let selector_val = args
        .get("selector")
        .ok_or_else(|| VelocityError::Config("Missing 'selector' argument".to_string()))?;

    let selector: Selector = serde_json::from_value(selector_val.clone())
        .map_err(|e| VelocityError::Config(format!("Invalid selector: {e}")))?;

    let element = driver.find_element(device_id, &selector).await?;
    serde_json::to_value(&element)
        .map_err(|e| VelocityError::Internal(anyhow::anyhow!("Serialization error: {e}")))
}

pub async fn assert_visible(
    driver: &Arc<dyn PlatformDriver>,
    device_id: &str,
    args: &Value,
) -> Result<Value> {
    let selector_val = args
        .get("selector")
        .ok_or_else(|| VelocityError::Config("Missing 'selector' argument".to_string()))?;

    let selector: Selector = serde_json::from_value(selector_val.clone())
        .map_err(|e| VelocityError::Config(format!("Invalid selector: {e}")))?;

    let element = driver.find_element(device_id, &selector).await?;
    let visible = driver.is_element_visible(device_id, &element).await?;

    if !visible {
        return Err(VelocityError::AssertionFailed {
            expected: "visible".to_string(),
            actual: "not visible".to_string(),
            selector: selector.to_string(),
            screenshot: None,
        });
    }

    Ok(serde_json::json!({
        "assertion": "visible",
        "selector": selector.to_string(),
        "visible": true,
        "element": {
            "label": element.label,
            "text": element.text,
            "bounds": element.bounds
        }
    }))
}
