use tokio::sync::Mutex;
use tracing::{debug, warn};
use velocity_common::*;

use crate::parser::parse_ios_hierarchy;
use crate::simctl::Simctl;
use crate::wda_bootstrap::WdaBootstrap;
use crate::wda_manager::WdaManager;

/// iOS platform driver using xcrun simctl for device management and
/// WebDriverAgent for UI automation.
pub struct IosDriver {
    simctl: Simctl,
    wda: Mutex<WdaManager>,
    bootstrap: Mutex<WdaBootstrap>,
}

impl Default for IosDriver {
    fn default() -> Self {
        Self::new()
    }
}

impl IosDriver {
    /// Create a new iOS driver with the default WDA URL (http://localhost:8100).
    pub fn new() -> Self {
        Self::with_wda_url("http://localhost:8100")
    }

    /// Create a new iOS driver with a custom WDA base URL.
    pub fn with_wda_url(wda_base_url: &str) -> Self {
        Self {
            simctl: Simctl::new(),
            wda: Mutex::new(WdaManager::new(wda_base_url)),
            bootstrap: Mutex::new(WdaBootstrap::new()),
        }
    }

    /// Ensure WDA is running before test execution.
    /// Downloads, builds, and launches WDA if needed.
    /// Also creates a WDA session if one doesn't exist (needed for inspector mode).
    pub async fn prepare(&self, device_id: &str) -> Result<()> {
        let mut bootstrap = self.bootstrap.lock().await;
        bootstrap.ensure_running(device_id).await?;
        drop(bootstrap);

        // Create a WDA session if one doesn't exist yet.
        // This enables operations like tap_at, swipe, etc. without requiring
        // launch_app to be called first (e.g. when using the inspector).
        let mut wda = self.wda.lock().await;
        if wda.client().session_id().is_none() {
            debug!(device_id, "creating default WDA session for inspector");
            wda.client_mut().create_session("").await?;
        }
        Ok(())
    }

    /// Stop the WDA process if we started it.
    pub async fn cleanup(&self) {
        let mut bootstrap = self.bootstrap.lock().await;
        bootstrap.stop().await;
    }

    /// Translate a velocity Selector into a WDA locator strategy and value.
    fn selector_to_wda(selector: &Selector) -> Result<(&'static str, String)> {
        match selector {
            Selector::Id(id) => Ok(("accessibility id", id.clone())),
            Selector::Text(text) => Ok(("name", text.clone())),
            Selector::TextContains(sub) => {
                // WDA doesn't have a native "contains" strategy, so use a
                // class chain predicate that searches label contains
                Ok((
                    "class chain",
                    format!("**/XCUIElementTypeAny[`label CONTAINS \"{sub}\"`]"),
                ))
            }
            Selector::AccessibilityId(aid) => Ok(("accessibility id", aid.clone())),
            Selector::ClassName(cls) => Ok(("class name", cls.clone())),
            Selector::Index { selector, index } => {
                let (using, value) = Self::selector_to_wda(selector)?;
                // For index selectors we'll resolve at the driver level
                // by finding all elements and picking the Nth one
                let _ = (using, value, index);
                Err(VelocityError::Config(
                    "Index selector must be resolved at driver level".to_string(),
                ))
            }
            Selector::Compound(selectors) => {
                // Build a class chain predicate combining all selectors with AND
                let mut predicates = Vec::new();
                for s in selectors {
                    match s {
                        Selector::Id(id) | Selector::AccessibilityId(id) => {
                            predicates.push(format!("`name == \"{id}\"`"));
                        }
                        Selector::Text(text) => {
                            predicates.push(format!("`label == \"{text}\"`"));
                        }
                        Selector::TextContains(sub) => {
                            predicates.push(format!("`label CONTAINS \"{sub}\"`"));
                        }
                        Selector::ClassName(cls) => {
                            predicates.push(format!("`type == \"{cls}\"`"));
                        }
                        _ => {}
                    }
                }
                if predicates.is_empty() {
                    return Err(VelocityError::Config(
                        "Compound selector produced no predicates".to_string(),
                    ));
                }
                let predicate = predicates.join(" AND ");
                Ok(("class chain", format!("**/XCUIElementTypeAny[{predicate}]")))
            }
        }
    }

    /// Returns true if the element was found via hierarchy parsing (not WDA).
    /// Hierarchy-parsed elements have bounds data but their platform_id is not
    /// a valid WDA session element UUID — it's typically the element's label text.
    fn is_hierarchy_element(element: &Element) -> bool {
        !element.bounds.is_empty() && (element.visible || element.platform_id.is_empty())
    }

    fn key_to_button(key: Key) -> &'static str {
        match key {
            Key::Home => "home",
            Key::VolumeUp => "volumeUp",
            Key::VolumeDown => "volumeDown",
            // iOS has no hardware back button; map to home
            Key::Back => "home",
            // Enter is handled via keyboard, not a hardware button
            Key::Enter => "home",
        }
    }

    /// Convert a WDA element response into a velocity Element by querying its
    /// properties through WDA.
    async fn wda_element_to_element(
        &self,
        wda_element_id: &str,
        _device_id: &str,
    ) -> Result<Element> {
        let wda = self.wda.lock().await;
        let client = wda.client();

        let text = client.get_text(wda_element_id).await.ok();
        let visible = client.is_displayed(wda_element_id).await.unwrap_or(false);

        Ok(Element {
            platform_id: wda_element_id.to_string(),
            label: text.clone(),
            text,
            element_type: "Unknown".to_string(),
            bounds: Rect {
                x: 0,
                y: 0,
                width: 0,
                height: 0,
            },
            enabled: true,
            visible,
            children: Vec::new(),
        })
    }

    /// Find elements using the hierarchy (parsed XML) for richer data, falling
    /// back to WDA direct queries if the hierarchy approach fails.
    async fn find_via_hierarchy(
        &self,
        device_id: &str,
        selector: &Selector,
    ) -> Result<Vec<Element>> {
        let xml = {
            let wda = self.wda.lock().await;
            wda.client().get_source().await?
        };

        let tree = parse_ios_hierarchy(&xml)?;
        let screen = Rect {
            x: 0,
            y: 0,
            width: tree.bounds.width,
            height: tree.bounds.height,
        };

        let matches = find_matching_elements(&tree, selector, &screen);
        if matches.is_empty() {
            // Fall back to WDA direct query
            return self.find_via_wda(device_id, selector).await;
        }
        Ok(matches)
    }

    fn find_via_wda<'a>(
        &'a self,
        device_id: &'a str,
        selector: &'a Selector,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<Element>>> + Send + 'a>>
    {
        Box::pin(async move {
            // Handle Index selector by finding all and picking Nth
            if let Selector::Index {
                selector: inner,
                index,
            } = selector
            {
                let all = self.find_via_wda(device_id, inner).await?;
                return match all.into_iter().nth(*index) {
                    Some(el) => Ok(vec![el]),
                    None => Ok(Vec::new()),
                };
            }

            let (using, value) = Self::selector_to_wda(selector)?;
            let wda = self.wda.lock().await;
            let wda_elements = wda.client().find_elements(using, &value).await?;
            drop(wda);

            let mut elements = Vec::new();
            for we in &wda_elements {
                match self.wda_element_to_element(&we.element_id, device_id).await {
                    Ok(el) => elements.push(el),
                    Err(_) => continue,
                }
            }
            Ok(elements)
        })
    }
}

#[async_trait::async_trait]
impl PlatformDriver for IosDriver {
    async fn prepare(&self, device_id: &str) -> Result<()> {
        self.prepare(device_id).await
    }

    async fn cleanup(&self) {
        self.cleanup().await;
    }

    async fn health_check(&self) -> HealthStatus {
        let wda = self.wda.lock().await;
        match wda.client().health_check().await {
            Ok(true) => HealthStatus::Healthy,
            Ok(false) => HealthStatus::Unhealthy,
            Err(_) => HealthStatus::Unhealthy,
        }
    }

    async fn restart_session(&self) -> Result<()> {
        warn!("Restarting WDA session...");
        let mut wda = self.wda.lock().await;
        wda.invalidate_session();
        Ok(())
    }

    async fn list_devices(&self) -> Result<Vec<DeviceInfo>> {
        self.simctl.list_devices().await
    }

    async fn boot_device(&self, device_id: &str) -> Result<()> {
        self.simctl.boot(device_id).await
    }

    async fn shutdown_device(&self, device_id: &str) -> Result<()> {
        self.simctl.shutdown(device_id).await
    }

    async fn install_app(&self, device_id: &str, app_path: &str) -> Result<()> {
        self.simctl.install(device_id, app_path).await
    }

    async fn launch_app(&self, device_id: &str, app_id: &str, clear_state: bool) -> Result<()> {
        if clear_state {
            let _ = self.simctl.terminate(device_id, app_id).await;

            // Clear the app's data container (AsyncStorage, NSUserDefaults, caches, etc.)
            match self
                .simctl
                .get_app_container(device_id, app_id, "data")
                .await
            {
                Ok(data_path) => {
                    debug!(path = %data_path, "clearing app data container");
                    for subdir in &["Library", "Documents", "tmp"] {
                        let dir = format!("{}/{}", data_path, subdir);
                        let _ = tokio::fs::remove_dir_all(&dir).await;
                        let _ = tokio::fs::create_dir_all(&dir).await;
                    }
                }
                Err(e) => {
                    warn!(error = %e, "could not resolve app data container, skipping clear");
                }
            }
        }

        // Ensure WDA session
        let mut wda = self.wda.lock().await;
        wda.ensure_session(device_id, app_id).await?;
        wda.client().invalidate_source_cache().await;
        drop(wda);

        self.simctl.launch(device_id, app_id).await
    }

    async fn stop_app(&self, device_id: &str, app_id: &str) -> Result<()> {
        self.simctl.terminate(device_id, app_id).await
    }

    async fn find_element(&self, device_id: &str, selector: &Selector) -> Result<Element> {
        debug!(selector = %selector, "finding element");

        let elements = self.find_via_hierarchy(device_id, selector).await?;
        elements
            .into_iter()
            .next()
            .ok_or_else(|| VelocityError::ElementNotFound {
                selector: selector.to_string(),
                timeout_ms: 0,
                screenshot: None,
                hierarchy_snapshot: None,
            })
    }

    async fn find_elements(&self, device_id: &str, selector: &Selector) -> Result<Vec<Element>> {
        debug!(selector = %selector, "finding elements");
        self.find_via_hierarchy(device_id, selector).await
    }

    async fn get_hierarchy(&self, device_id: &str) -> Result<Element> {
        debug!(device_id, "getting hierarchy");
        let wda = self.wda.lock().await;
        // Always fetch fresh for sync engine — bypass cache
        let xml = wda.client().get_source_fresh().await?;
        drop(wda);
        parse_ios_hierarchy(&xml)
    }

    async fn tap(&self, _device_id: &str, element: &Element) -> Result<()> {
        debug!(element_id = %element.platform_id, "tapping element");
        if Self::is_hierarchy_element(element) {
            let (cx, cy) = element.bounds.center();
            let wda = self.wda.lock().await;
            return wda.client().tap_at(cx as f64, cy as f64).await;
        }
        let wda = self.wda.lock().await;
        wda.client().click(&element.platform_id).await
    }

    async fn double_tap(&self, _device_id: &str, element: &Element) -> Result<()> {
        debug!(element_id = %element.platform_id, "double tapping element");
        if Self::is_hierarchy_element(element) {
            let (cx, cy) = element.bounds.center();
            let wda = self.wda.lock().await;
            wda.client().tap_at(cx as f64, cy as f64).await?;
            drop(wda);
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            let wda = self.wda.lock().await;
            return wda.client().tap_at(cx as f64, cy as f64).await;
        }
        let wda = self.wda.lock().await;
        wda.client().click(&element.platform_id).await?;
        drop(wda);
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let wda = self.wda.lock().await;
        wda.client().click(&element.platform_id).await
    }

    async fn long_press(
        &self,
        _device_id: &str,
        element: &Element,
        duration_ms: u64,
    ) -> Result<()> {
        debug!(
            element_id = %element.platform_id,
            duration_ms,
            "long pressing element"
        );
        // WDA long press: use dragfromtoforduration with same start/end coordinates
        let (cx, cy) = element.bounds.center();
        let duration_s = duration_ms as f64 / 1000.0;
        let wda = self.wda.lock().await;
        wda.client()
            .swipe(cx as f64, cy as f64, cx as f64, cy as f64, duration_s)
            .await
    }

    async fn input_text(&self, _device_id: &str, element: &Element, text: &str) -> Result<()> {
        debug!(
            element_id = %element.platform_id,
            text,
            "inputting text"
        );
        if Self::is_hierarchy_element(element) {
            // Tap to focus the element, then type via session-level keys
            let (cx, cy) = element.bounds.center();
            let wda = self.wda.lock().await;
            wda.client().tap_at(cx as f64, cy as f64).await?;
            drop(wda);
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            let wda = self.wda.lock().await;
            return wda.client().type_text(text).await;
        }
        let wda = self.wda.lock().await;
        wda.client().send_keys(&element.platform_id, text).await
    }

    async fn clear_text(&self, _device_id: &str, element: &Element) -> Result<()> {
        debug!(element_id = %element.platform_id, "clearing text");
        if Self::is_hierarchy_element(element) {
            // Tap to focus, then select all and delete
            let (cx, cy) = element.bounds.center();
            let wda = self.wda.lock().await;
            wda.client().tap_at(cx as f64, cy as f64).await?;
            drop(wda);
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            // Triple-tap to select all, then delete
            let wda = self.wda.lock().await;
            wda.client().tap_at(cx as f64, cy as f64).await?;
            wda.client().tap_at(cx as f64, cy as f64).await?;
            wda.client().tap_at(cx as f64, cy as f64).await?;
            drop(wda);
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            let wda = self.wda.lock().await;
            return wda.client().type_text("\u{8}").await; // backspace
        }
        let wda = self.wda.lock().await;
        wda.client().clear(&element.platform_id).await
    }

    async fn swipe(&self, device_id: &str, direction: Direction) -> Result<()> {
        debug!(?direction, "swiping by direction");
        let wda = self.wda.lock().await;
        let (w, h) = wda.client().get_screen_size().await?;
        let cx = w as f64 / 2.0;
        let cy = h as f64 / 2.0;
        let dist_x = w as f64 / 4.0;
        let dist_y = h as f64 / 4.0;

        let (from_x, from_y, to_x, to_y) = match direction {
            Direction::Up => (cx, cy + dist_y, cx, cy - dist_y),
            Direction::Down => (cx, cy - dist_y, cx, cy + dist_y),
            Direction::Left => (cx + dist_x, cy, cx - dist_x, cy),
            Direction::Right => (cx - dist_x, cy, cx + dist_x, cy),
        };

        wda.client().swipe(from_x, from_y, to_x, to_y, 0.3).await?;
        let _ = device_id;
        Ok(())
    }

    async fn swipe_coords(&self, _device_id: &str, from: (i32, i32), to: (i32, i32)) -> Result<()> {
        debug!(?from, ?to, "swiping by coordinates");
        let wda = self.wda.lock().await;
        wda.client()
            .swipe(from.0 as f64, from.1 as f64, to.0 as f64, to.1 as f64, 0.3)
            .await
    }

    async fn press_key(&self, _device_id: &str, key: Key) -> Result<()> {
        let button = Self::key_to_button(key);
        debug!(button, "pressing key");
        let wda = self.wda.lock().await;
        wda.client().press_button(button).await
    }

    async fn screenshot(&self, device_id: &str) -> Result<Vec<u8>> {
        debug!(device_id, "taking screenshot");
        // Prefer simctl screenshot as it's more reliable
        match self.simctl.screenshot(device_id).await {
            Ok(bytes) => Ok(bytes),
            Err(_) => {
                // Fall back to WDA screenshot
                let wda = self.wda.lock().await;
                wda.client().screenshot().await
            }
        }
    }

    async fn screen_size(&self, _device_id: &str) -> Result<(i32, i32)> {
        let wda = self.wda.lock().await;
        wda.client().get_screen_size().await
    }

    async fn get_element_text(&self, _device_id: &str, element: &Element) -> Result<String> {
        debug!(element_id = %element.platform_id, "getting element text");
        if Self::is_hierarchy_element(element) {
            return Ok(element
                .text
                .clone()
                .or_else(|| element.label.clone())
                .unwrap_or_default());
        }
        let wda = self.wda.lock().await;
        wda.client().get_text(&element.platform_id).await
    }

    async fn is_element_visible(&self, _device_id: &str, element: &Element) -> Result<bool> {
        debug!(element_id = %element.platform_id, "checking element visibility");
        // Elements found via hierarchy parsing already have visibility data.
        // Only query WDA for elements that have a WDA session element ID (UUID format).
        if element.visible && !element.bounds.is_empty() {
            return Ok(true);
        }
        let wda = self.wda.lock().await;
        wda.client().is_displayed(&element.platform_id).await
    }
}

/// Search the parsed Element tree for all elements matching a Selector.
fn find_matching_elements(root: &Element, selector: &Selector, screen: &Rect) -> Vec<Element> {
    let mut results = Vec::new();
    collect_matching(root, selector, screen, &mut results);

    // Handle Index selector
    if let Selector::Index {
        selector: inner,
        index,
    } = selector
    {
        let mut all = Vec::new();
        collect_matching(root, inner, screen, &mut all);
        return all.into_iter().nth(*index).into_iter().collect();
    }

    results
}

fn collect_matching(
    element: &Element,
    selector: &Selector,
    screen: &Rect,
    results: &mut Vec<Element>,
) {
    if element_matches(element, selector, screen) {
        results.push(element.clone());
    }
    for child in &element.children {
        collect_matching(child, selector, screen, results);
    }
}

fn element_matches(element: &Element, selector: &Selector, screen: &Rect) -> bool {
    if !element.visible || element.bounds.is_empty() || !element.bounds.intersects(screen) {
        return false;
    }

    match selector {
        Selector::Id(id) | Selector::AccessibilityId(id) => {
            element.platform_id == *id || element.label.as_deref() == Some(id.as_str())
        }
        Selector::Text(text) => {
            element.text.as_deref() == Some(text.as_str())
                || element.label.as_deref() == Some(text.as_str())
        }
        Selector::TextContains(sub) => {
            element
                .text
                .as_ref()
                .is_some_and(|t| t.contains(sub.as_str()))
                || element
                    .label
                    .as_ref()
                    .is_some_and(|l| l.contains(sub.as_str()))
        }
        Selector::ClassName(cls) => element.element_type == *cls,
        Selector::Index { .. } => false,
        Selector::Compound(selectors) => selectors
            .iter()
            .all(|s| element_matches(element, s, screen)),
    }
}
