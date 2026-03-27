use std::collections::HashSet;

use tracing::{debug, info, warn};
use velocity_common::{DialogConfig, Element, PlatformDriver};

/// Known system dialog button labels that should be auto-dismissed.
/// Organized by platform convention and priority (prefer "Allow" over "Don't Allow").
const IOS_DISMISS_LABELS: &[&str] = &[
    // Location
    "Allow While Using App",
    "Allow Once",
    // Notifications
    "Allow",
    // Contacts, Photos, Camera, Microphone
    "OK",
    "Allow Full Access",
    // Generic
    "Continue",
    "Accept",
];

const IOS_ALERT_INDICATORS: &[&str] = &[
    // Common iOS system alert title patterns (element types)
    "Alert",
    "XCUIElementTypeAlert",
];

const ANDROID_DISMISS_LABELS: &[&str] = &[
    // Permissions
    "While using the app",
    "Only this time",
    "Allow",
    "ALLOW",
    // Generic
    "OK",
    "ACCEPT",
    "Continue",
    "CONTINUE",
    "GOT IT",
];

const ANDROID_ALERT_INDICATORS: &[&str] = &[
    "PermissionDialogActivity",
    "GrantPermissionsActivity",
    "android:id/alertTitle",
];

/// Handles automatic detection and dismissal of system dialogs
/// (permission prompts, alerts) that block test execution.
pub struct DialogHandler {
    config: DialogConfig,
    all_dismiss_labels: Vec<String>,
}

impl DialogHandler {
    pub fn new(config: DialogConfig) -> Self {
        let mut all_dismiss_labels = Vec::new();
        let mut seen = HashSet::new();

        for label in IOS_DISMISS_LABELS.iter().chain(ANDROID_DISMISS_LABELS.iter()) {
            if seen.insert(*label) {
                all_dismiss_labels.push((*label).to_string());
            }
        }

        for label in &config.custom_dismiss_labels {
            if seen.insert(label.as_str()) {
                all_dismiss_labels.push(label.clone());
            }
        }
        Self {
            config,
            all_dismiss_labels,
        }
    }

    /// Check the current hierarchy for system dialogs and dismiss them.
    /// Returns the number of dialogs dismissed.
    pub async fn dismiss_if_present(
        &self,
        driver: &dyn PlatformDriver,
        device_id: &str,
    ) -> u32 {
        if !self.config.enabled {
            return 0;
        }

        let mut dismissed = 0;

        for _ in 0..self.config.max_dismissals {
            let hierarchy = match driver.get_hierarchy(device_id).await {
                Ok(h) => h,
                Err(_) => break,
            };

            // Check if there's a system dialog present
            if !self.has_system_dialog(&hierarchy) {
                break;
            }

            // Find a dismiss button and tap it
            if let Some(button) = self.find_dismiss_button(&hierarchy) {
                info!(
                    label = button.label.as_deref().unwrap_or(""),
                    text = button.text.as_deref().unwrap_or(""),
                    "auto-dismissing system dialog"
                );
                match driver.tap(device_id, &button).await {
                    Ok(()) => {
                        dismissed += 1;
                        // Brief pause to let the dialog animate away
                        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
                    }
                    Err(e) => {
                        warn!(error = %e, "failed to dismiss system dialog");
                        break;
                    }
                }
            } else {
                debug!("system dialog detected but no dismiss button found");
                break;
            }
        }

        if dismissed > 0 {
            info!(count = dismissed, "dismissed system dialog(s)");
        }

        dismissed
    }

    /// Check if the hierarchy contains a system dialog/alert.
    fn has_system_dialog(&self, root: &Element) -> bool {
        self.find_dialog_element(root).is_some()
    }

    /// Recursively search for a system dialog element.
    fn find_dialog_element<'a>(&self, element: &'a Element) -> Option<&'a Element> {
        // Check element type against known alert indicators
        let is_alert = IOS_ALERT_INDICATORS
            .iter()
            .chain(ANDROID_ALERT_INDICATORS.iter())
            .any(|indicator| {
                element.element_type.contains(indicator)
                    || element
                        .platform_id
                        .contains(indicator)
            });

        if is_alert {
            return Some(element);
        }

        // Recurse into children
        for child in &element.children {
            if let Some(found) = self.find_dialog_element(child) {
                return Some(found);
            }
        }

        None
    }

    /// Find a tappable dismiss button within a dialog subtree.
    fn find_dismiss_button(&self, root: &Element) -> Option<Element> {
        // First, find the dialog subtree
        let dialog = self.find_dialog_element(root)?;

        // Search within the dialog for a matching button
        let mut candidates = Vec::new();
        self.collect_dismiss_buttons(dialog, &mut candidates);

        // Sort by priority: first match in dismiss_labels list wins
        candidates.sort_by_key(|(priority, _)| *priority);
        candidates.into_iter().next().map(|(_, el)| el)
    }

    /// Collect all buttons within an element tree that match dismiss labels.
    fn collect_dismiss_buttons(&self, element: &Element, out: &mut Vec<(usize, Element)>) {
        let is_button = element.element_type.contains("Button")
            || element.element_type.contains("button");

        if is_button && element.visible && element.enabled {
            let label_text = element
                .label
                .as_deref()
                .or(element.text.as_deref())
                .unwrap_or("");

            for (priority, dismiss_label) in self.all_dismiss_labels.iter().enumerate() {
                if label_text.eq_ignore_ascii_case(dismiss_label) {
                    out.push((priority, element.clone()));
                    break;
                }
            }
        }

        for child in &element.children {
            self.collect_dismiss_buttons(child, out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use velocity_common::Rect;

    fn make_button(label: &str, element_type: &str) -> Element {
        Element {
            platform_id: String::new(),
            label: Some(label.to_string()),
            text: Some(label.to_string()),
            element_type: element_type.to_string(),
            bounds: Rect {
                x: 0,
                y: 0,
                width: 100,
                height: 44,
            },
            enabled: true,
            visible: true,
            children: vec![],
        }
    }

    fn make_ios_alert(buttons: Vec<Element>) -> Element {
        Element {
            platform_id: "alert".to_string(),
            label: Some("Allow App to access your location?".to_string()),
            text: None,
            element_type: "XCUIElementTypeAlert".to_string(),
            bounds: Rect {
                x: 50,
                y: 300,
                width: 300,
                height: 200,
            },
            enabled: true,
            visible: true,
            children: buttons,
        }
    }

    #[test]
    fn detects_ios_alert() {
        let handler = DialogHandler::new(DialogConfig::default());
        let alert = make_ios_alert(vec![
            make_button("Don't Allow", "XCUIElementTypeButton"),
            make_button("Allow While Using App", "XCUIElementTypeButton"),
        ]);
        let root = Element {
            platform_id: "root".to_string(),
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
            children: vec![alert],
        };

        assert!(handler.has_system_dialog(&root));
    }

    #[test]
    fn finds_allow_button_over_deny() {
        let handler = DialogHandler::new(DialogConfig::default());
        let alert = make_ios_alert(vec![
            make_button("Don't Allow", "XCUIElementTypeButton"),
            make_button("Allow While Using App", "XCUIElementTypeButton"),
        ]);
        let root = Element {
            platform_id: "root".to_string(),
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
            children: vec![alert],
        };

        let button = handler.find_dismiss_button(&root);
        assert!(button.is_some());
        assert_eq!(
            button.unwrap().label.as_deref(),
            Some("Allow While Using App")
        );
    }

    #[test]
    fn no_dialog_returns_false() {
        let handler = DialogHandler::new(DialogConfig::default());
        let root = Element {
            platform_id: "root".to_string(),
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
            children: vec![make_button("Login", "XCUIElementTypeButton")],
        };

        assert!(!handler.has_system_dialog(&root));
    }

    #[test]
    fn custom_dismiss_labels_work() {
        let config = DialogConfig {
            custom_dismiss_labels: vec!["Dismiss".to_string()],
            ..Default::default()
        };
        let handler = DialogHandler::new(config);
        let alert = make_ios_alert(vec![make_button("Dismiss", "XCUIElementTypeButton")]);
        let root = Element {
            platform_id: "root".to_string(),
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
            children: vec![alert],
        };

        let button = handler.find_dismiss_button(&root);
        assert!(button.is_some());
        assert_eq!(button.unwrap().label.as_deref(), Some("Dismiss"));
    }
}
