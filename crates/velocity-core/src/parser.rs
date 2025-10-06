use serde::de::{self, MapAccess, Visitor};
use serde::{Deserialize, Deserializer};
use velocity_common::{
    Action, Direction, Flow, Key, Result, Selector, Step, SuiteConfig, TestCase, TestSuite,
    VelocityError,
};

pub fn parse_suite(path: &str) -> Result<TestSuite> {
    let content = std::fs::read_to_string(path).map_err(|e| VelocityError::Config(e.to_string()))?;
    parse_suite_from_str(&content).map_err(|e| match e {
        VelocityError::Config(msg) => VelocityError::YamlParse {
            file: path.into(),
            line: 0,
            col: 0,
            message: msg,
        },
        other => other,
    })
}

pub fn parse_suite_from_str(yaml: &str) -> Result<TestSuite> {
    let raw: RawTestSuite =
        serde_yaml::from_str(yaml).map_err(|e| VelocityError::Config(e.to_string()))?;
    Ok(raw.into_test_suite())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawTestSuite {
    app_id: String,
    #[serde(default)]
    config: SuiteConfig,
    #[serde(default)]
    flows: Vec<RawFlow>,
    tests: Vec<RawTestCase>,
}

impl RawTestSuite {
    fn into_test_suite(self) -> TestSuite {
        TestSuite {
            app_id: self.app_id,
            config: self.config,
            flows: self.flows.into_iter().map(|f| f.into_flow()).collect(),
            tests: self.tests.into_iter().map(|t| t.into_test_case()).collect(),
        }
    }
}

#[derive(Deserialize)]
struct RawFlow {
    id: String,
    steps: Vec<DslStep>,
}

impl RawFlow {
    fn into_flow(self) -> Flow {
        Flow {
            id: self.id,
            steps: self.steps.into_iter().map(|s| s.into_step()).collect(),
        }
    }
}

#[derive(Deserialize)]
struct RawTestCase {
    name: String,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    isolated: bool,
    steps: Vec<DslStep>,
}

impl RawTestCase {
    fn into_test_case(self) -> TestCase {
        TestCase {
            name: self.name,
            tags: self.tags,
            isolated: self.isolated,
            steps: self.steps.into_iter().map(|s| s.into_step()).collect(),
        }
    }
}

struct DslStep {
    action: Action,
    timeout_ms: Option<u64>,
}

impl DslStep {
    fn into_step(self) -> Step {
        Step {
            action: self.action,
            timeout_ms: self.timeout_ms,
        }
    }
}

impl<'de> Deserialize<'de> for DslStep {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_map(DslStepVisitor)
    }
}

struct DslStepVisitor;

impl<'de> Visitor<'de> for DslStepVisitor {
    type Value = DslStep;

    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("a step map with an action key like tap, inputText, etc.")
    }

    fn visit_map<A>(self, mut map: A) -> std::result::Result<DslStep, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut action: Option<Action> = None;
        let mut timeout_ms: Option<u64> = None;

        while let Some(key) = map.next_key::<String>()? {
            match key.as_str() {
                "timeoutMs" | "timeout_ms" => {
                    timeout_ms = Some(map.next_value()?);
                }
                action_key => {
                    if action.is_some() {
                        return Err(de::Error::custom(
                            "step has multiple action keys; expected exactly one",
                        ));
                    }
                    action = Some(parse_action(action_key, &mut map)?);
                }
            }
        }

        let action = action.ok_or_else(|| de::Error::custom("step has no action key"))?;
        Ok(DslStep { action, timeout_ms })
    }
}

fn parse_action<'de, A>(key: &str, map: &mut A) -> std::result::Result<Action, A::Error>
where
    A: MapAccess<'de>,
{
    match key {
        "launchApp" => {
            let v: LaunchAppDsl = map.next_value()?;
            Ok(Action::LaunchApp {
                app_id: v.app_id.unwrap_or_default(),
                clear_state: v.clear_state.unwrap_or(false),
            })
        }
        "stopApp" => {
            let v: StopAppDsl = map.next_value()?;
            Ok(Action::StopApp {
                app_id: v.app_id.unwrap_or_default(),
            })
        }
        "tap" => {
            let v: SelectorDsl = map.next_value()?;
            Ok(Action::Tap {
                selector: v.into_selector().map_err(de::Error::custom)?,
            })
        }
        "doubleTap" => {
            let v: SelectorDsl = map.next_value()?;
            Ok(Action::DoubleTap {
                selector: v.into_selector().map_err(de::Error::custom)?,
            })
        }
        "longPress" => {
            let v: LongPressDsl = map.next_value()?;
            let selector = v.selector.map(|s| s.into_selector());
            let selector = match selector {
                Some(Ok(s)) => s,
                Some(Err(e)) => return Err(de::Error::custom(e)),
                None => {
                    // Direct selector fields on the longPress object
                    SelectorDsl {
                        id: v.id,
                        text: v.text,
                        text_contains: v.text_contains,
                        accessibility_id: v.accessibility_id,
                        class_name: v.class_name,
                        index: v.index,
                    }
                    .into_selector()
                    .map_err(de::Error::custom)?
                }
            };
            Ok(Action::LongPress {
                selector,
                duration_ms: v.duration_ms,
            })
        }
        "inputText" => {
            let v: InputTextDsl = map.next_value()?;
            let selector = v.selector.into_selector().map_err(de::Error::custom)?;
            Ok(Action::InputText {
                selector,
                text: v.text,
            })
        }
        "clearText" => {
            let v: SelectorDsl = map.next_value()?;
            Ok(Action::ClearText {
                selector: v.into_selector().map_err(de::Error::custom)?,
            })
        }
        "assertVisible" => {
            let v: SelectorDsl = map.next_value()?;
            Ok(Action::AssertVisible {
                selector: v.into_selector().map_err(de::Error::custom)?,
            })
        }
        "assertNotVisible" => {
            let v: SelectorDsl = map.next_value()?;
            Ok(Action::AssertNotVisible {
                selector: v.into_selector().map_err(de::Error::custom)?,
            })
        }
        "assertText" => {
            let v: AssertTextDsl = map.next_value()?;
            let selector = v.selector.into_selector().map_err(de::Error::custom)?;
            Ok(Action::AssertText {
                selector,
                expected: v.expected,
            })
        }
        "scrollUntilVisible" => {
            let v: ScrollDsl = map.next_value()?;
            let selector = v.selector.into_selector().map_err(de::Error::custom)?;
            Ok(Action::ScrollUntilVisible {
                selector,
                direction: v.direction.unwrap_or(Direction::Down),
                max_scrolls: v.max_scrolls.unwrap_or(5),
            })
        }
        "swipe" => {
            let v: SwipeDsl = map.next_value()?;
            Ok(Action::Swipe {
                direction: v.direction,
                from: v.from.map(|c| (c.x, c.y)),
                to: v.to.map(|c| (c.x, c.y)),
            })
        }
        "screenshot" => {
            let v: ScreenshotDsl = map.next_value()?;
            Ok(Action::Screenshot {
                filename: v.filename,
            })
        }
        "pressKey" => {
            let v: PressKeyDsl = map.next_value()?;
            Ok(Action::PressKey { key: v.key })
        }
        "wait" => {
            let v: WaitDsl = map.next_value()?;
            Ok(Action::Wait { ms: v.ms })
        }
        "runFlow" => {
            let v: RunFlowDsl = map.next_value()?;
            Ok(Action::RunFlow {
                flow_id: v.flow_id.unwrap_or(v.flow.unwrap_or_default()),
            })
        }
        other => Err(de::Error::custom(format!("unknown action: {other}"))),
    }
}

// DSL helper structs for deserialization

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SelectorDsl {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    text_contains: Option<String>,
    #[serde(default)]
    accessibility_id: Option<String>,
    #[serde(default)]
    class_name: Option<String>,
    #[serde(default)]
    index: Option<usize>,
}

impl SelectorDsl {
    fn into_selector(self) -> std::result::Result<Selector, String> {
        let base = if let Some(id) = self.id {
            Selector::Id(id)
        } else if let Some(text) = self.text {
            Selector::Text(text)
        } else if let Some(sub) = self.text_contains {
            Selector::TextContains(sub)
        } else if let Some(aid) = self.accessibility_id {
            Selector::AccessibilityId(aid)
        } else if let Some(cls) = self.class_name {
            Selector::ClassName(cls)
        } else {
            return Err("selector must have at least one of: id, text, textContains, accessibilityId, className".to_string());
        };

        Ok(match self.index {
            Some(idx) => Selector::Index {
                selector: Box::new(base),
                index: idx,
            },
            None => base,
        })
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LaunchAppDsl {
    #[serde(default)]
    app_id: Option<String>,
    #[serde(default)]
    clear_state: Option<bool>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StopAppDsl {
    #[serde(default)]
    app_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LongPressDsl {
    #[serde(default)]
    selector: Option<SelectorDsl>,
    #[serde(default)]
    duration_ms: Option<u64>,
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    text: Option<String>,
    #[serde(default)]
    text_contains: Option<String>,
    #[serde(default)]
    accessibility_id: Option<String>,
    #[serde(default)]
    class_name: Option<String>,
    #[serde(default)]
    index: Option<usize>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct InputTextDsl {
    selector: SelectorDsl,
    text: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct AssertTextDsl {
    selector: SelectorDsl,
    expected: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ScrollDsl {
    selector: SelectorDsl,
    #[serde(default)]
    direction: Option<Direction>,
    #[serde(default)]
    max_scrolls: Option<u32>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Coords {
    x: i32,
    y: i32,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct SwipeDsl {
    #[serde(default)]
    direction: Option<Direction>,
    #[serde(default)]
    from: Option<Coords>,
    #[serde(default)]
    to: Option<Coords>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ScreenshotDsl {
    #[serde(default)]
    filename: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PressKeyDsl {
    key: Key,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WaitDsl {
    ms: u64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RunFlowDsl {
    #[serde(default)]
    flow_id: Option<String>,
    #[serde(default)]
    flow: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn minimal_suite() -> &'static str {
        r#"
appId: com.example.app
tests:
  - name: "basic test"
    steps:
      - tap: { id: "login_button" }
      - inputText: { selector: { id: "email_field" }, text: "user@example.com" }
      - assertVisible: { text: "Welcome" }
      - wait: { ms: 500 }
"#
    }

    #[test]
    fn parse_minimal_suite() {
        let suite = parse_suite_from_str(minimal_suite()).unwrap();
        assert_eq!(suite.app_id, "com.example.app");
        assert_eq!(suite.tests.len(), 1);
        assert_eq!(suite.tests[0].steps.len(), 4);
    }

    #[test]
    fn parse_tap_selector() {
        let yaml = r#"
appId: com.test
tests:
  - name: t
    steps:
      - tap: { id: "btn" }
"#;
        let suite = parse_suite_from_str(yaml).unwrap();
        match &suite.tests[0].steps[0].action {
            Action::Tap { selector } => {
                assert_eq!(*selector, Selector::Id("btn".to_string()));
            }
            other => panic!("expected Tap, got {:?}", other),
        }
    }

    #[test]
    fn parse_text_contains_selector() {
        let yaml = r#"
appId: com.test
tests:
  - name: t
    steps:
      - assertVisible: { textContains: "partial" }
"#;
        let suite = parse_suite_from_str(yaml).unwrap();
        match &suite.tests[0].steps[0].action {
            Action::AssertVisible { selector } => {
                assert_eq!(*selector, Selector::TextContains("partial".to_string()));
            }
            other => panic!("expected AssertVisible, got {:?}", other),
        }
    }

    #[test]
    fn parse_indexed_selector() {
        let yaml = r#"
appId: com.test
tests:
  - name: t
    steps:
      - tap: { className: "Button", index: 2 }
"#;
        let suite = parse_suite_from_str(yaml).unwrap();
        match &suite.tests[0].steps[0].action {
            Action::Tap { selector } => {
                assert_eq!(
                    *selector,
                    Selector::Index {
                        selector: Box::new(Selector::ClassName("Button".to_string())),
                        index: 2,
                    }
                );
            }
            other => panic!("expected Tap, got {:?}", other),
        }
    }

    #[test]
    fn parse_input_text() {
        let yaml = r#"
appId: com.test
tests:
  - name: t
    steps:
      - inputText: { selector: { id: "field" }, text: "hello" }
"#;
        let suite = parse_suite_from_str(yaml).unwrap();
        match &suite.tests[0].steps[0].action {
            Action::InputText { selector, text } => {
                assert_eq!(*selector, Selector::Id("field".to_string()));
                assert_eq!(text, "hello");
            }
            other => panic!("expected InputText, got {:?}", other),
        }
    }

    #[test]
    fn parse_launch_stop_app() {
        let yaml = r#"
appId: com.test
tests:
  - name: t
    steps:
      - launchApp: { appId: "com.other", clearState: true }
      - stopApp: { appId: "com.other" }
"#;
        let suite = parse_suite_from_str(yaml).unwrap();
        match &suite.tests[0].steps[0].action {
            Action::LaunchApp {
                app_id,
                clear_state,
            } => {
                assert_eq!(app_id, "com.other");
                assert!(*clear_state);
            }
            other => panic!("expected LaunchApp, got {:?}", other),
        }
        match &suite.tests[0].steps[1].action {
            Action::StopApp { app_id } => {
                assert_eq!(app_id, "com.other");
            }
            other => panic!("expected StopApp, got {:?}", other),
        }
    }

    #[test]
    fn parse_run_flow() {
        let yaml = r#"
appId: com.test
flows:
  - id: login
    steps:
      - tap: { id: "btn" }
tests:
  - name: t
    steps:
      - runFlow: { flowId: "login" }
"#;
        let suite = parse_suite_from_str(yaml).unwrap();
        assert_eq!(suite.flows.len(), 1);
        match &suite.tests[0].steps[0].action {
            Action::RunFlow { flow_id } => {
                assert_eq!(flow_id, "login");
            }
            other => panic!("expected RunFlow, got {:?}", other),
        }
    }

    #[test]
    fn parse_scroll_until_visible() {
        let yaml = r#"
appId: com.test
tests:
  - name: t
    steps:
      - scrollUntilVisible: { selector: { text: "Footer" }, direction: up, maxScrolls: 10 }
"#;
        let suite = parse_suite_from_str(yaml).unwrap();
        match &suite.tests[0].steps[0].action {
            Action::ScrollUntilVisible {
                selector,
                direction,
                max_scrolls,
            } => {
                assert_eq!(*selector, Selector::Text("Footer".to_string()));
                assert_eq!(*direction, Direction::Up);
                assert_eq!(*max_scrolls, 10);
            }
            other => panic!("expected ScrollUntilVisible, got {:?}", other),
        }
    }

    #[test]
    fn parse_swipe_direction() {
        let yaml = r#"
appId: com.test
tests:
  - name: t
    steps:
      - swipe: { direction: left }
"#;
        let suite = parse_suite_from_str(yaml).unwrap();
        match &suite.tests[0].steps[0].action {
            Action::Swipe {
                direction,
                from,
                to,
            } => {
                assert_eq!(*direction, Some(Direction::Left));
                assert!(from.is_none());
                assert!(to.is_none());
            }
            other => panic!("expected Swipe, got {:?}", other),
        }
    }

    #[test]
    fn parse_assert_text() {
        let yaml = r#"
appId: com.test
tests:
  - name: t
    steps:
      - assertText: { selector: { id: "title" }, expected: "Hello World" }
"#;
        let suite = parse_suite_from_str(yaml).unwrap();
        match &suite.tests[0].steps[0].action {
            Action::AssertText { selector, expected } => {
                assert_eq!(*selector, Selector::Id("title".to_string()));
                assert_eq!(expected, "Hello World");
            }
            other => panic!("expected AssertText, got {:?}", other),
        }
    }

    #[test]
    fn parse_press_key_and_screenshot() {
        let yaml = r#"
appId: com.test
tests:
  - name: t
    steps:
      - pressKey: { key: back }
      - screenshot: { filename: "after_back.png" }
"#;
        let suite = parse_suite_from_str(yaml).unwrap();
        match &suite.tests[0].steps[0].action {
            Action::PressKey { key } => assert_eq!(*key, Key::Back),
            other => panic!("expected PressKey, got {:?}", other),
        }
        match &suite.tests[0].steps[1].action {
            Action::Screenshot { filename } => {
                assert_eq!(filename.as_deref(), Some("after_back.png"));
            }
            other => panic!("expected Screenshot, got {:?}", other),
        }
    }

    #[test]
    fn parse_step_with_timeout() {
        let yaml = r#"
appId: com.test
tests:
  - name: t
    steps:
      - tap: { id: "btn" }
        timeoutMs: 5000
"#;
        let suite = parse_suite_from_str(yaml).unwrap();
        assert_eq!(suite.tests[0].steps[0].timeout_ms, Some(5000));
    }

    #[test]
    fn parse_config() {
        let yaml = r#"
appId: com.test
config:
  platform: ios
  sync:
    intervalMs: 100
    stabilityCount: 5
    timeoutMs: 20000
tests:
  - name: t
    steps:
      - wait: { ms: 100 }
"#;
        let suite = parse_suite_from_str(yaml).unwrap();
        assert_eq!(suite.config.sync.interval_ms, 100);
        assert_eq!(suite.config.sync.stability_count, 5);
    }

    #[test]
    fn parse_unknown_action_fails() {
        let yaml = r#"
appId: com.test
tests:
  - name: t
    steps:
      - unknownAction: { id: "btn" }
"#;
        let result = parse_suite_from_str(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn parse_empty_selector_fails() {
        let yaml = r#"
appId: com.test
tests:
  - name: t
    steps:
      - tap: {}
"#;
        let result = parse_suite_from_str(yaml);
        assert!(result.is_err());
    }

    #[test]
    fn parse_tags() {
        let yaml = r#"
appId: com.test
tests:
  - name: tagged test
    tags: [smoke, login]
    steps:
      - wait: { ms: 100 }
"#;
        let suite = parse_suite_from_str(yaml).unwrap();
        assert_eq!(suite.tests[0].tags, vec!["smoke", "login"]);
    }

    #[test]
    fn parse_long_press_with_nested_selector() {
        let yaml = r#"
appId: com.test
tests:
  - name: t
    steps:
      - longPress: { selector: { id: "item" }, durationMs: 1500 }
"#;
        let suite = parse_suite_from_str(yaml).unwrap();
        match &suite.tests[0].steps[0].action {
            Action::LongPress {
                selector,
                duration_ms,
            } => {
                assert_eq!(*selector, Selector::Id("item".to_string()));
                assert_eq!(*duration_ms, Some(1500));
            }
            other => panic!("expected LongPress, got {:?}", other),
        }
    }

    #[test]
    fn parse_double_tap() {
        let yaml = r#"
appId: com.test
tests:
  - name: t
    steps:
      - doubleTap: { accessibilityId: "heart" }
"#;
        let suite = parse_suite_from_str(yaml).unwrap();
        match &suite.tests[0].steps[0].action {
            Action::DoubleTap { selector } => {
                assert_eq!(
                    *selector,
                    Selector::AccessibilityId("heart".to_string())
                );
            }
            other => panic!("expected DoubleTap, got {:?}", other),
        }
    }

    #[test]
    fn parse_clear_text() {
        let yaml = r#"
appId: com.test
tests:
  - name: t
    steps:
      - clearText: { id: "email" }
"#;
        let suite = parse_suite_from_str(yaml).unwrap();
        match &suite.tests[0].steps[0].action {
            Action::ClearText { selector } => {
                assert_eq!(*selector, Selector::Id("email".to_string()));
            }
            other => panic!("expected ClearText, got {:?}", other),
        }
    }
}
