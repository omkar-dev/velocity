use serde::{Deserialize, Serialize};

/// A selector used to find UI elements on the screen.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum Selector {
    Id(String),
    Text(String),
    TextContains(String),
    AccessibilityId(String),
    ClassName(String),
    Index {
        selector: Box<Selector>,
        index: usize,
    },
    Compound(Vec<Selector>),
}

impl std::fmt::Display for Selector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Id(id) => write!(f, "id={id:?}"),
            Self::Text(text) => write!(f, "text={text:?}"),
            Self::TextContains(sub) => write!(f, "textContains={sub:?}"),
            Self::AccessibilityId(aid) => write!(f, "accessibilityId={aid:?}"),
            Self::ClassName(cls) => write!(f, "className={cls:?}"),
            Self::Index { selector, index } => write!(f, "{selector}[{index}]"),
            Self::Compound(selectors) => {
                let parts: Vec<String> = selectors.iter().map(|s| s.to_string()).collect();
                write!(f, "compound({})", parts.join(" AND "))
            }
        }
    }
}

/// A rectangular region on the screen.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl Rect {
    pub fn center(&self) -> (i32, i32) {
        (self.x + self.width / 2, self.y + self.height / 2)
    }

    pub fn intersects(&self, other: &Rect) -> bool {
        self.x < other.x + other.width
            && self.x + self.width > other.x
            && self.y < other.y + other.height
            && self.y + self.height > other.y
    }

    pub fn is_empty(&self) -> bool {
        self.width <= 0 || self.height <= 0
    }
}

/// A UI element from the accessibility tree.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Element {
    pub platform_id: String,
    pub label: Option<String>,
    pub text: Option<String>,
    pub element_type: String,
    pub bounds: Rect,
    pub enabled: bool,
    pub visible: bool,
    pub children: Vec<Element>,
}

/// Swipe direction.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

/// Hardware/soft keys that can be pressed.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum Key {
    Back,
    Home,
    Enter,
    VolumeUp,
    VolumeDown,
}

/// An action to perform on the device.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Action {
    LaunchApp {
        app_id: String,
        clear_state: bool,
    },
    StopApp {
        app_id: String,
    },
    Tap {
        selector: Selector,
    },
    DoubleTap {
        selector: Selector,
    },
    LongPress {
        selector: Selector,
        duration_ms: Option<u64>,
    },
    InputText {
        selector: Selector,
        text: String,
    },
    ClearText {
        selector: Selector,
    },
    AssertVisible {
        selector: Selector,
    },
    AssertNotVisible {
        selector: Selector,
    },
    AssertText {
        selector: Selector,
        expected: String,
    },
    ScrollUntilVisible {
        selector: Selector,
        direction: Direction,
        max_scrolls: u32,
    },
    Swipe {
        direction: Option<Direction>,
        from: Option<(i32, i32)>,
        to: Option<(i32, i32)>,
    },
    Screenshot {
        filename: Option<String>,
    },
    PressKey {
        key: Key,
    },
    Wait {
        ms: u64,
    },
    RunFlow {
        flow_id: String,
    },
}

/// The target platform.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    Ios,
    Android,
}

impl std::fmt::Display for Platform {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ios => write!(f, "ios"),
            Self::Android => write!(f, "android"),
        }
    }
}

/// Type of device (physical hardware vs virtual).
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum DeviceType {
    Physical,
    Simulator,
    Emulator,
    #[default]
    Unknown,
}

impl std::fmt::Display for DeviceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Physical => write!(f, "physical"),
            Self::Simulator => write!(f, "simulator"),
            Self::Emulator => write!(f, "emulator"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}

/// Information about a connected device.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
    pub platform: Platform,
    pub state: DeviceState,
    pub os_version: Option<String>,
    #[serde(default)]
    pub device_type: DeviceType,
}

/// State of a device.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum DeviceState {
    Booted,
    Shutdown,
    Unknown,
}

impl std::fmt::Display for DeviceState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Booted => write!(f, "booted"),
            Self::Shutdown => write!(f, "shutdown"),
            Self::Unknown => write!(f, "unknown"),
        }
    }
}
