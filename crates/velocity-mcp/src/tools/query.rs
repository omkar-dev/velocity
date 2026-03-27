use std::sync::Arc;
use std::time::{Duration, Instant};

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

// --- get_screen_summary ---

const INTERACTIVE_TYPES: &[&str] = &[
    "Button", "TextField", "EditText", "Switch", "Toggle",
    "Link", "Input", "Slider", "Checkbox", "Radio",
    "SecureTextField", "SearchField", "TextInput",
];

const NAV_TYPES: &[&str] = &[
    "TabBar", "NavigationBar", "Toolbar", "BottomNavigation",
    "BottomBar", "TabView",
];

const HEADER_TYPES: &[&str] = &["NavigationBar", "Header", "NavBar"];

#[derive(Default)]
struct ScreenSummary {
    screen_title: Option<String>,
    screen_text: Vec<String>,
    interactive: Vec<Value>,
    navigation: Vec<String>,
    total: usize,
    visible: usize,
    interactive_count: usize,
    text_fields: usize,
    buttons: usize,
}

fn type_matches_any(element_type: &str, patterns: &[&str]) -> bool {
    let lower = element_type.to_lowercase();
    patterns.iter().any(|p| lower.contains(&p.to_lowercase()))
}

fn element_display_text(el: &Element) -> Option<String> {
    el.label
        .as_deref()
        .or(el.text.as_deref())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

fn summarize_tree(root: &Element, summary: &mut ScreenSummary) {
    summary.total += 1;

    if root.visible {
        summary.visible += 1;
    }

    // Extract screen title from first header-type element
    if summary.screen_title.is_none() && type_matches_any(&root.element_type, HEADER_TYPES) {
        summary.screen_title = element_display_text(root).or_else(|| {
            // Try first child with text
            root.children.iter().find_map(element_display_text)
        });
    }

    // Collect navigation items
    if type_matches_any(&root.element_type, NAV_TYPES) {
        for child in &root.children {
            if let Some(text) = element_display_text(child) {
                if !summary.navigation.contains(&text) {
                    summary.navigation.push(text);
                }
            }
        }
    }

    // Collect visible text
    if root.visible {
        if let Some(text) = &root.text {
            if !text.is_empty() && !summary.screen_text.contains(text) {
                summary.screen_text.push(text.clone());
            }
        }
        if let Some(label) = &root.label {
            if !label.is_empty() && !summary.screen_text.contains(label) {
                summary.screen_text.push(label.clone());
            }
        }
    }

    // Collect interactive elements
    if type_matches_any(&root.element_type, INTERACTIVE_TYPES) {
        summary.interactive_count += 1;
        let lower = root.element_type.to_lowercase();
        if lower.contains("button") {
            summary.buttons += 1;
        }
        if lower.contains("text") || lower.contains("input") || lower.contains("search") {
            summary.text_fields += 1;
        }
        summary.interactive.push(serde_json::json!({
            "type": root.element_type,
            "label": element_display_text(root).unwrap_or_default(),
            "enabled": root.enabled
        }));
    }

    for child in &root.children {
        summarize_tree(child, summary);
    }
}

pub async fn get_screen_summary(
    driver: &Arc<dyn PlatformDriver>,
    device_id: &str,
    _args: &Value,
) -> Result<Value> {
    let hierarchy = driver.get_hierarchy(device_id).await?;
    let mut summary = ScreenSummary::default();
    summarize_tree(&hierarchy, &mut summary);

    Ok(serde_json::json!({
        "screen_title": summary.screen_title,
        "screen_text": summary.screen_text,
        "interactive": summary.interactive,
        "navigation": summary.navigation,
        "counts": {
            "total": summary.total,
            "visible": summary.visible,
            "interactive": summary.interactive_count,
            "text_fields": summary.text_fields,
            "buttons": summary.buttons
        }
    }))
}

// --- wait_for_element ---

pub async fn wait_for_element(
    driver: &Arc<dyn PlatformDriver>,
    device_id: &str,
    args: &Value,
) -> Result<Value> {
    let selector_val = args
        .get("selector")
        .ok_or_else(|| VelocityError::Config("Missing 'selector' argument".to_string()))?;

    let selector: Selector = serde_json::from_value(selector_val.clone())
        .map_err(|e| VelocityError::Config(format!("Invalid selector: {e}")))?;

    let timeout_ms = args
        .get("timeout_ms")
        .and_then(|v| v.as_u64())
        .unwrap_or(10_000)
        .min(30_000);

    let poll_interval_ms = args
        .get("poll_interval_ms")
        .and_then(|v| v.as_u64())
        .unwrap_or(500)
        .max(100);

    let start = Instant::now();
    let timeout = Duration::from_millis(timeout_ms);
    let poll_interval = Duration::from_millis(poll_interval_ms);

    loop {
        match driver.find_element(device_id, &selector).await {
            Ok(element) => {
                let visible = driver.is_element_visible(device_id, &element).await?;
                if visible {
                    let waited_ms = start.elapsed().as_millis() as u64;
                    return Ok(serde_json::json!({
                        "found": true,
                        "waited_ms": waited_ms,
                        "element": {
                            "label": element.label,
                            "text": element.text,
                            "bounds": element.bounds,
                            "enabled": element.enabled,
                            "visible": true
                        }
                    }));
                }
            }
            Err(e) if e.kind() == velocity_common::ErrorKind::ElementNotFound => {
                // Expected — element not yet present, keep polling
            }
            Err(e) => return Err(e), // Real driver error — propagate
        }

        if start.elapsed() >= timeout {
            return Err(VelocityError::ElementNotFound {
                selector: selector.to_string(),
                timeout_ms,
                screenshot: None,
                hierarchy_snapshot: None,
            });
        }

        tokio::time::sleep(poll_interval).await;
    }
}
