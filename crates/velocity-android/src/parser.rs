use quick_xml::events::Event;
use quick_xml::Reader;
use velocity_common::{Element, Rect, Result, VelocityError};

/// Parse a uiautomator XML dump into an Element tree.
pub fn parse_hierarchy(xml: &str) -> Result<Element> {
    // Delegate to v2 which handles both Start and Empty events correctly
    parse_hierarchy_v2(xml)
}

/// Parse a uiautomator XML dump handling both self-closing and nested nodes.
pub fn parse_hierarchy_v2(xml: &str) -> Result<Element> {
    let mut reader = Reader::from_str(xml);
    let mut stack: Vec<Element> = Vec::new();

    let root = Element {
        platform_id: "hierarchy".to_string(),
        label: None,
        text: None,
        element_type: "hierarchy".to_string(),
        bounds: Rect {
            x: 0,
            y: 0,
            width: 1080,
            height: 2400,
        },
        enabled: true,
        visible: true,
        children: Vec::new(),
    };
    stack.push(root);

    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                if e.name().as_ref() == b"node" {
                    let element = parse_node_attributes_from_bytes(e)?;
                    stack.push(element);
                }
            }
            Ok(Event::Empty(ref e)) => {
                if e.name().as_ref() == b"node" {
                    let element = parse_node_attributes_from_bytes(e)?;
                    if let Some(parent) = stack.last_mut() {
                        parent.children.push(element);
                    }
                }
            }
            Ok(Event::End(ref e)) => {
                if e.name().as_ref() == b"node" && stack.len() > 1 {
                    let child = stack.pop().unwrap();
                    if let Some(parent) = stack.last_mut() {
                        parent.children.push(child);
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                return Err(VelocityError::Config(format!(
                    "Failed to parse uiautomator XML: {e}"
                )));
            }
            _ => {}
        }
        buf.clear();
    }

    stack
        .into_iter()
        .next()
        .ok_or_else(|| VelocityError::Config("Empty hierarchy XML".to_string()))
}

fn parse_node_attributes_from_bytes(e: &quick_xml::events::BytesStart) -> Result<Element> {
    let mut text = None;
    let mut resource_id = String::new();
    let mut content_desc = None;
    let mut class = String::new();
    let mut bounds_str = String::new();
    let mut enabled = true;
    let mut clickable = false;

    for attr in e.attributes().flatten() {
        let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
        let val = String::from_utf8_lossy(&attr.value).to_string();

        match key.as_str() {
            "text" => {
                if !val.is_empty() {
                    text = Some(val);
                }
            }
            "resource-id" => resource_id = val,
            "content-desc" => {
                if !val.is_empty() {
                    content_desc = Some(val);
                }
            }
            "class" => class = val,
            "bounds" => bounds_str = val,
            "enabled" => enabled = val == "true",
            "clickable" => clickable = val == "true",
            _ => {}
        }
    }

    let bounds = parse_bounds(&bounds_str);
    let visible = !bounds.is_empty();

    // Simplify class name: "android.widget.Button" -> "Button"
    let element_type = class.rsplit('.').next().unwrap_or(&class).to_string();

    let _ = clickable; // reserved for future use

    Ok(Element {
        platform_id: resource_id,
        label: content_desc,
        text,
        element_type,
        bounds,
        enabled,
        visible,
        children: Vec::new(),
    })
}

/// Parse bounds string like "[120,640][960,736]" into a Rect.
fn parse_bounds(bounds: &str) -> Rect {
    let nums: Vec<i32> = bounds
        .replace('[', "")
        .replace(']', ",")
        .split(',')
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse().ok())
        .collect();

    if nums.len() == 4 {
        Rect {
            x: nums[0],
            y: nums[1],
            width: nums[2] - nums[0],
            height: nums[3] - nums[1],
        }
    } else {
        Rect {
            x: 0,
            y: 0,
            width: 0,
            height: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_bounds() {
        let rect = parse_bounds("[120,640][960,736]");
        assert_eq!(rect.x, 120);
        assert_eq!(rect.y, 640);
        assert_eq!(rect.width, 840);
        assert_eq!(rect.height, 96);
    }

    #[test]
    fn test_parse_bounds_empty() {
        let rect = parse_bounds("");
        assert!(rect.is_empty());
    }

    #[test]
    fn test_parse_hierarchy() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<hierarchy rotation="0">
  <node index="0" text="" resource-id="" class="android.widget.FrameLayout"
        bounds="[0,0][1080,2400]" enabled="true" clickable="false" content-desc="">
    <node index="0" text="Log In" resource-id="com.app:id/login_button"
          class="android.widget.Button" content-desc="Login"
          bounds="[120,640][960,736]" enabled="true" clickable="true">
    </node>
  </node>
</hierarchy>"#;

        let root = parse_hierarchy_v2(xml).unwrap();
        assert_eq!(root.element_type, "hierarchy");
        assert_eq!(root.children.len(), 1);

        let frame = &root.children[0];
        assert_eq!(frame.element_type, "FrameLayout");
        assert_eq!(frame.children.len(), 1);

        let button = &frame.children[0];
        assert_eq!(button.text.as_deref(), Some("Log In"));
        assert_eq!(button.platform_id, "com.app:id/login_button");
        assert_eq!(button.label.as_deref(), Some("Login"));
        assert_eq!(button.element_type, "Button");
        assert_eq!(button.bounds.x, 120);
        assert_eq!(button.bounds.width, 840);
    }
}
