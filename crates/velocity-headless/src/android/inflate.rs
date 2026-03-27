use crate::render_tree::{NodeStyle, Position, RenderNode};

use super::axml::{AxmlDocument, AxmlElement, AxmlParser};
use super::resources::ResourceTable;
use super::styles::AndroidStyleMapper;
use taffy::FlexDirection;

/// Inflates Android XML layouts into RenderNode trees.
pub struct AndroidInflater {
    resources: ResourceTable,
}

impl AndroidInflater {
    pub fn new(resources: ResourceTable) -> Self {
        Self { resources }
    }

    /// Inflate a binary XML layout into a RenderNode tree.
    pub fn inflate_binary(&self, data: &[u8]) -> Result<RenderNode, InflateError> {
        let doc = AxmlParser::parse(data).map_err(|e| InflateError::ParseError(e.to_string()))?;
        let root = doc.root.ok_or(InflateError::EmptyLayout)?;
        Ok(self.inflate_element(&root, None))
    }

    /// Inflate a plain XML layout string into a RenderNode tree.
    ///
    /// Used for pre-decompiled XML or test fixtures.
    pub fn inflate_xml(&self, xml: &str) -> Result<RenderNode, InflateError> {
        let doc = self.parse_plain_xml(xml)?;
        Ok(self.inflate_element(&doc, None))
    }

    /// Inflate a single AXML element recursively.
    fn inflate_element(
        &self,
        element: &AxmlElement,
        parent_orientation: Option<FlexDirection>,
    ) -> RenderNode {
        let view_name = simplify_class_name(&element.name);
        let mut style = AndroidStyleMapper::map_attributes(
            &element.attributes,
            &self.resources,
            parent_orientation,
        );

        // Apply view-type-specific defaults
        match view_name.as_str() {
            "FrameLayout" => {
                // FrameLayout children are positioned absolutely (stacked)
                // We model this with relative positioning on the container
                // and children default to top-left
            }
            "LinearLayout" => {
                // orientation already handled in style mapper
            }
            "ConstraintLayout" | "androidx.constraintlayout.widget.ConstraintLayout" => {
                // Simplified: treat as column flex container
                style.flex_direction = taffy::FlexDirection::Column;
            }
            "ScrollView" | "HorizontalScrollView" | "NestedScrollView" => {
                // Treat as a simple container
                style.flex_direction = if view_name == "HorizontalScrollView" {
                    taffy::FlexDirection::Row
                } else {
                    taffy::FlexDirection::Column
                };
            }
            _ => {}
        }

        // Extract id and content description
        let id = element
            .attributes
            .get("id")
            .map(|v| extract_resource_name(v));
        let label = element.attributes.get("contentDescription").cloned();
        let text = element.attributes.get("text").map(|v| {
            let resolved = self.resources.resolve(v);
            resolved
        });

        let mut node = if text.is_some() {
            RenderNode::text_node(text.clone().unwrap(), &view_name)
        } else {
            RenderNode::container(&view_name)
        };

        node.id = id;
        node.label = label;
        node.style = style;
        node.enabled = element
            .attributes
            .get("enabled")
            .map(|v| v != "false" && v != "0")
            .unwrap_or(true);

        // Inflate children
        for child in &element.children {
            // Skip non-view elements (e.g., <requestFocus/>)
            if is_view_element(&child.name) {
                let child_node = self.inflate_element(child, Some(node.style.flex_direction));
                node.children.push(child_node);
            }
        }

        node
    }

    /// Parse plain XML (not binary) using quick-xml.
    fn parse_plain_xml(&self, xml: &str) -> Result<AxmlElement, InflateError> {
        use quick_xml::events::Event;
        use quick_xml::Reader;

        let mut reader = Reader::from_str(xml);
        let mut stack: Vec<AxmlElement> = Vec::new();
        let mut root: Option<AxmlElement> = None;

        loop {
            match reader.read_event() {
                Ok(Event::Start(e)) => {
                    let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                    let mut attributes = std::collections::HashMap::new();

                    for attr in e.attributes().flatten() {
                        let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                        let val = String::from_utf8_lossy(&attr.value).to_string();
                        // Strip android: namespace prefix
                        let key = key
                            .strip_prefix("android:")
                            .unwrap_or(&key)
                            .to_string();
                        attributes.insert(key, val);
                    }

                    stack.push(AxmlElement {
                        name,
                        namespace: None,
                        attributes,
                        children: Vec::new(),
                    });
                }
                Ok(Event::Empty(e)) => {
                    let name = String::from_utf8_lossy(e.name().as_ref()).to_string();
                    let mut attributes = std::collections::HashMap::new();

                    for attr in e.attributes().flatten() {
                        let key = String::from_utf8_lossy(attr.key.as_ref()).to_string();
                        let val = String::from_utf8_lossy(&attr.value).to_string();
                        let key = key
                            .strip_prefix("android:")
                            .unwrap_or(&key)
                            .to_string();
                        attributes.insert(key, val);
                    }

                    let element = AxmlElement {
                        name,
                        namespace: None,
                        attributes,
                        children: Vec::new(),
                    };

                    if let Some(parent) = stack.last_mut() {
                        parent.children.push(element);
                    } else {
                        root = Some(element);
                    }
                }
                Ok(Event::End(_)) => {
                    if let Some(element) = stack.pop() {
                        if let Some(parent) = stack.last_mut() {
                            parent.children.push(element);
                        } else {
                            root = Some(element);
                        }
                    }
                }
                Ok(Event::Eof) => break,
                Err(e) => return Err(InflateError::ParseError(e.to_string())),
                _ => {}
            }
        }

        root.ok_or(InflateError::EmptyLayout)
    }
}

/// Simplify Android class names: `android.widget.Button` -> `Button`.
fn simplify_class_name(name: &str) -> String {
    name.rsplit('.').next().unwrap_or(name).to_string()
}

/// Extract resource name from `@+id/name` or `@id/name` format.
fn extract_resource_name(value: &str) -> String {
    value
        .strip_prefix("@+id/")
        .or_else(|| value.strip_prefix("@id/"))
        .unwrap_or(value)
        .to_string()
}

/// Check if an element name represents an Android view.
fn is_view_element(name: &str) -> bool {
    !matches!(
        name,
        "requestFocus" | "tag" | "include" | "merge" | "data" | "variable" | "import"
    )
}

#[derive(Debug, thiserror::Error)]
pub enum InflateError {
    #[error("XML parse error: {0}")]
    ParseError(String),
    #[error("Empty layout — no root element found")]
    EmptyLayout,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inflate_simple_xml() {
        let xml = r#"
            <LinearLayout
                android:id="@+id/root"
                android:layout_width="match_parent"
                android:layout_height="match_parent"
                android:orientation="vertical">
                <TextView
                    android:id="@+id/title"
                    android:layout_width="wrap_content"
                    android:layout_height="wrap_content"
                    android:text="Hello World"
                    android:textSize="16sp" />
                <View
                    android:id="@+id/spacer"
                    android:layout_width="match_parent"
                    android:layout_height="16dp" />
            </LinearLayout>
        "#;

        let inflater = AndroidInflater::new(ResourceTable::empty());
        let root = inflater.inflate_xml(xml).unwrap();

        assert_eq!(root.node_type, "LinearLayout");
        assert_eq!(root.id.as_deref(), Some("root"));
        assert_eq!(root.children.len(), 2);

        assert_eq!(root.children[0].node_type, "TextView");
        assert_eq!(root.children[0].text.as_deref(), Some("Hello World"));
        assert_eq!(root.children[0].id.as_deref(), Some("title"));

        assert_eq!(root.children[1].node_type, "View");
        assert_eq!(root.children[1].id.as_deref(), Some("spacer"));
    }

    #[test]
    fn test_inflate_nested_layout() {
        let xml = r#"
            <FrameLayout
                android:layout_width="match_parent"
                android:layout_height="match_parent">
                <LinearLayout
                    android:layout_width="match_parent"
                    android:layout_height="wrap_content"
                    android:orientation="horizontal">
                    <TextView
                        android:text="Left"
                        android:layout_width="0dp"
                        android:layout_height="wrap_content"
                        android:layout_weight="1" />
                    <TextView
                        android:text="Right"
                        android:layout_width="0dp"
                        android:layout_height="wrap_content"
                        android:layout_weight="1" />
                </LinearLayout>
            </FrameLayout>
        "#;

        let inflater = AndroidInflater::new(ResourceTable::empty());
        let root = inflater.inflate_xml(xml).unwrap();

        assert_eq!(root.node_type, "FrameLayout");
        assert_eq!(root.children.len(), 1);
        assert_eq!(root.children[0].children.len(), 2);
    }
}
