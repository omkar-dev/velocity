use quick_xml::events::Event;
use quick_xml::Reader;
use velocity_common::{Element, Rect, Result, VelocityError};

/// Parse WDA's /source XML hierarchy into a velocity Element tree.
///
/// The iOS XML hierarchy uses XCUIElement types (e.g. XCUIElementTypeButton) and
/// attributes like `name` (accessibility identifier), `label`, `value`, `visible`,
/// `enabled`, and coordinate attributes `x`, `y`, `width`, `height`.
pub fn parse_ios_hierarchy(xml: &str) -> Result<Element> {
    let mut reader = Reader::from_str(xml);
    let mut stack: Vec<Element> = Vec::new();

    let root = Element {
        platform_id: "ios_hierarchy".to_string(),
        label: None,
        text: None,
        element_type: "Application".to_string(),
        bounds: Rect {
            x: 0,
            y: 0,
            width: 390,
            height: 844,
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
                let element = parse_ios_node(e)?;
                stack.push(element);
            }
            Ok(Event::Empty(ref e)) => {
                let element = parse_ios_node(e)?;
                if let Some(parent) = stack.last_mut() {
                    parent.children.push(element);
                }
            }
            Ok(Event::End(_)) => {
                if stack.len() > 1 {
                    let child = stack.pop().unwrap();
                    if let Some(parent) = stack.last_mut() {
                        parent.children.push(child);
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                return Err(VelocityError::Config(format!(
                    "Failed to parse iOS XML hierarchy: {e}"
                )));
            }
            _ => {}
        }
        buf.clear();
    }

    let mut result = stack.into_iter().next().ok_or_else(|| {
        VelocityError::Config("Empty iOS hierarchy XML".to_string())
    })?;

    // If the root has exactly one child (the actual app element), update root bounds
    if result.children.len() == 1 {
        let child_bounds = result.children[0].bounds;
        if !child_bounds.is_empty() {
            result.bounds = child_bounds;
        }
    }

    Ok(result)
}

fn parse_ios_node(e: &quick_xml::events::BytesStart) -> Result<Element> {
    let tag_name = String::from_utf8_lossy(e.name().as_ref()).to_string();

    let mut name: Option<String> = None;
    let mut label: Option<String> = None;
    let mut value: Option<String> = None;
    let mut element_type = simplify_ios_type(&tag_name);
    let mut x: i32 = 0;
    let mut y: i32 = 0;
    let mut width: i32 = 0;
    let mut height: i32 = 0;
    let mut is_visible = true;
    let mut enabled = true;

    for attr in e.attributes().flatten() {
        let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
        let val = String::from_utf8_lossy(&attr.value).to_string();

        match key.as_str() {
            "type" => element_type = simplify_ios_type(&val),
            "name" => {
                if !val.is_empty() {
                    name = Some(val);
                }
            }
            "label" => {
                if !val.is_empty() {
                    label = Some(val);
                }
            }
            "value" => {
                if !val.is_empty() {
                    value = Some(val);
                }
            }
            "x" => x = val.parse().unwrap_or(0),
            "y" => y = val.parse().unwrap_or(0),
            "width" => width = val.parse().unwrap_or(0),
            "height" => height = val.parse().unwrap_or(0),
            "visible" | "isVisible" => is_visible = val == "true" || val == "1",
            "enabled" => enabled = val == "true" || val == "1",
            _ => {}
        }
    }

    let bounds = Rect {
        x,
        y,
        width,
        height,
    };

    // Map iOS attributes to velocity Element fields:
    // - name (accessibility identifier) -> label (used as accessibility id)
    // - label (display text) -> used as label fallback
    // - value (text content) -> text
    // Priority: `name` is the accessibility identifier, `label` is the display label
    let element_label = name.or(label);
    let element_text = value;

    Ok(Element {
        platform_id: element_label.clone().unwrap_or_default(),
        label: element_label,
        text: element_text,
        element_type,
        bounds,
        enabled,
        visible: is_visible,
        children: Vec::new(),
    })
}

/// Simplify iOS element type names.
/// "XCUIElementTypeButton" -> "Button", "XCUIElementTypeStaticText" -> "StaticText"
fn simplify_ios_type(raw: &str) -> String {
    raw.strip_prefix("XCUIElementType")
        .unwrap_or(raw)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simplify_ios_type() {
        assert_eq!(simplify_ios_type("XCUIElementTypeButton"), "Button");
        assert_eq!(
            simplify_ios_type("XCUIElementTypeStaticText"),
            "StaticText"
        );
        assert_eq!(simplify_ios_type("XCUIElementTypeCell"), "Cell");
        assert_eq!(simplify_ios_type("Other"), "Other");
        assert_eq!(
            simplify_ios_type("XCUIElementTypeApplication"),
            "Application"
        );
    }

    #[test]
    fn test_parse_simple_hierarchy() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<XCUIElementTypeApplication type="XCUIElementTypeApplication" name="MyApp" label="MyApp" enabled="true" visible="true" x="0" y="0" width="390" height="844">
  <XCUIElementTypeWindow type="XCUIElementTypeWindow" enabled="true" visible="true" x="0" y="0" width="390" height="844">
    <XCUIElementTypeButton type="XCUIElementTypeButton" name="loginButton" label="Log In" value="" enabled="true" visible="true" x="50" y="400" width="290" height="44" />
    <XCUIElementTypeStaticText type="XCUIElementTypeStaticText" name="" label="Welcome" value="Welcome" enabled="true" visible="true" x="50" y="200" width="290" height="30" />
  </XCUIElementTypeWindow>
</XCUIElementTypeApplication>"#;

        let root = parse_ios_hierarchy(xml).unwrap();
        assert_eq!(root.element_type, "Application");
        assert_eq!(root.bounds.width, 390);
        assert_eq!(root.bounds.height, 844);

        // Root wraps the Application element
        assert_eq!(root.children.len(), 1);
        let app = &root.children[0];
        assert_eq!(app.element_type, "Application");
        assert_eq!(app.label.as_deref(), Some("MyApp"));

        // App contains a Window
        assert_eq!(app.children.len(), 1);
        let window = &app.children[0];
        assert_eq!(window.element_type, "Window");
        assert_eq!(window.children.len(), 2);

        // Button
        let button = &window.children[0];
        assert_eq!(button.element_type, "Button");
        assert_eq!(button.label.as_deref(), Some("loginButton"));
        assert_eq!(button.platform_id, "loginButton");
        assert_eq!(button.bounds.x, 50);
        assert_eq!(button.bounds.y, 400);
        assert_eq!(button.bounds.width, 290);
        assert_eq!(button.bounds.height, 44);
        assert!(button.enabled);
        assert!(button.visible);

        // StaticText
        let text = &window.children[1];
        assert_eq!(text.element_type, "StaticText");
        assert_eq!(text.text.as_deref(), Some("Welcome"));
        assert_eq!(text.bounds.x, 50);
        assert_eq!(text.bounds.y, 200);
    }

    #[test]
    fn test_parse_nested_hierarchy() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<XCUIElementTypeApplication type="XCUIElementTypeApplication" name="App" label="App" enabled="true" visible="true" x="0" y="0" width="390" height="844">
  <XCUIElementTypeOther type="XCUIElementTypeOther" enabled="true" visible="true" x="0" y="0" width="390" height="844">
    <XCUIElementTypeCell type="XCUIElementTypeCell" name="orderCell_0" label="Order 1" enabled="true" visible="true" x="0" y="100" width="390" height="80">
      <XCUIElementTypeStaticText type="XCUIElementTypeStaticText" name="orderTitle" label="Coffee" value="Coffee" enabled="true" visible="true" x="16" y="110" width="200" height="20" />
      <XCUIElementTypeStaticText type="XCUIElementTypeStaticText" name="orderPrice" label="$4.50" value="$4.50" enabled="true" visible="true" x="300" y="110" width="74" height="20" />
    </XCUIElementTypeCell>
  </XCUIElementTypeOther>
</XCUIElementTypeApplication>"#;

        let root = parse_ios_hierarchy(xml).unwrap();
        let app = &root.children[0];
        let other = &app.children[0];
        let cell = &other.children[0];

        assert_eq!(cell.element_type, "Cell");
        assert_eq!(cell.label.as_deref(), Some("orderCell_0"));
        assert_eq!(cell.children.len(), 2);

        let title = &cell.children[0];
        assert_eq!(title.text.as_deref(), Some("Coffee"));
        assert_eq!(title.label.as_deref(), Some("orderTitle"));

        let price = &cell.children[1];
        assert_eq!(price.text.as_deref(), Some("$4.50"));
    }

    #[test]
    fn test_parse_invisible_element() {
        let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<XCUIElementTypeOther type="XCUIElementTypeOther" enabled="true" visible="true" x="0" y="0" width="390" height="844">
  <XCUIElementTypeButton type="XCUIElementTypeButton" name="hidden" label="Hidden" enabled="true" visible="false" x="0" y="0" width="0" height="0" />
</XCUIElementTypeOther>"#;

        let root = parse_ios_hierarchy(xml).unwrap();
        let other = &root.children[0];
        let button = &other.children[0];

        assert!(!button.visible);
        assert!(button.bounds.is_empty());
    }

    #[test]
    fn test_parse_empty_xml() {
        let xml = "";
        let result = parse_ios_hierarchy(xml);
        // Should still return the synthetic root
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_malformed_xml() {
        let xml = "<unclosed>";
        // quick-xml may or may not error on this; either way we handle gracefully
        let result = parse_ios_hierarchy(xml);
        assert!(result.is_ok());
    }
}
