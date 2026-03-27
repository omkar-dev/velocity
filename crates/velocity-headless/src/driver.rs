use std::collections::HashMap;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::RwLock;
use velocity_common::{
    DeviceInfo, Direction, Element, HealthStatus, Key, Platform, PlatformDriver, Rect, Selector,
    VelocityError,
};
use velocity_common::selector_match::{find_all_in_tree, find_in_tree};

use crate::android::inflate::AndroidInflater;
use crate::android::resources::ResourceTable;
use crate::config::HeadlessConfig;
use crate::ios::inflate::IosInflater;
use crate::ios::resources::IosResourceLoader;
use crate::ios::xib::XibParser;
use crate::session::HeadlessSession;
use crate::text::TextMeasurer;

/// Headless rendering driver implementing PlatformDriver.
///
/// Renders Android XML layouts and iOS XIB/Storyboard files to pixels
/// without requiring an emulator or simulator. Uses CPU-only rendering
/// via tiny-skia and Taffy flexbox layout.
pub struct HeadlessDriver {
    platform: Platform,
    config: HeadlessConfig,
    sessions: RwLock<HashMap<String, HeadlessSession>>,
    text_measurer: Arc<TextMeasurer>,
}

// SAFETY: HeadlessSession has its own unsafe Send/Sync impls. The only non-auto-Send type
// in the chain is taffy's CompactLength which uses `*const ()` as tagged float encoding.
unsafe impl Send for HeadlessDriver {}
unsafe impl Sync for HeadlessDriver {}

impl HeadlessDriver {
    /// Create a new headless driver for the given platform.
    pub fn new(platform: Platform, config: HeadlessConfig) -> Self {
        Self {
            platform,
            config,
            sessions: RwLock::new(HashMap::new()),
            text_measurer: Arc::new(TextMeasurer::new()),
        }
    }

    /// Load and inflate a layout for the given app.
    async fn inflate_layout(
        &self,
        session: &mut HeadlessSession,
        _app_id: &str,
    ) -> Result<(), VelocityError> {
        let app_path: Option<String> = session
            .app_path
            .clone()
            .or_else(|| self.config.app_path.clone());

        let initial_layout: Option<String> = self.config.initial_layout.clone();

        match self.platform {
            Platform::Android => {
                self.inflate_android(session, app_path.as_deref(), initial_layout.as_deref())
                    .await
            }
            Platform::Ios => {
                self.inflate_ios(session, app_path.as_deref(), initial_layout.as_deref()).await
            }
        }
    }

    async fn inflate_android(
        &self,
        session: &mut HeadlessSession,
        app_path: Option<&str>,
        initial_layout: Option<&str>,
    ) -> Result<(), VelocityError> {
        // If we have an APK, extract and inflate from it
        if let Some(apk_path) = app_path {
            let path = Path::new(apk_path);
            if path.extension().and_then(|e| e.to_str()) == Some("apk") {
                let loader = crate::android::apk::ApkLoader::from_path(path).map_err(|e| {
                    VelocityError::Internal(anyhow::anyhow!("APK load failed: {}", e))
                })?;

                let layout_name = initial_layout.unwrap_or("activity_main");
                let layout_data = loader
                    .get_layout(layout_name)
                    .ok_or_else(|| {
                        VelocityError::Config(format!("Layout '{}' not found in APK", layout_name))
                    })?;

                let inflater = AndroidInflater::new(loader.resources.clone());
                let tree = inflater.inflate_binary(layout_data).map_err(|e| {
                    VelocityError::Internal(anyhow::anyhow!("Layout inflate failed: {}", e))
                })?;

                session.set_render_tree(tree).map_err(|e| {
                    VelocityError::Internal(anyhow::anyhow!("Render failed: {}", e))
                })?;

                return Ok(());
            }

            // Plain XML file path
            if path.extension().and_then(|e| e.to_str()) == Some("xml") {
                let xml = std::fs::read_to_string(path).map_err(|e| {
                    VelocityError::Internal(anyhow::anyhow!("Failed to read XML: {}", e))
                })?;

                let inflater = AndroidInflater::new(ResourceTable::empty());
                let tree = inflater.inflate_xml(&xml).map_err(|e| {
                    VelocityError::Internal(anyhow::anyhow!("Layout inflate failed: {}", e))
                })?;

                session.set_render_tree(tree).map_err(|e| {
                    VelocityError::Internal(anyhow::anyhow!("Render failed: {}", e))
                })?;

                return Ok(());
            }
        }

        // No app path or layout — create empty screen
        let root = crate::render_tree::RenderNode::container("View").with_style(
            crate::render_tree::NodeStyle {
                width: taffy::Dimension::length(self.config.width as f32),
                height: taffy::Dimension::length(self.config.height as f32),
                background_color: crate::render_tree::Color::WHITE,
                ..Default::default()
            },
        );

        session
            .set_render_tree(root)
            .map_err(|e| VelocityError::Internal(anyhow::anyhow!("Render failed: {}", e)))?;

        Ok(())
    }

    async fn inflate_ios(
        &self,
        session: &mut HeadlessSession,
        app_path: Option<&str>,
        initial_layout: Option<&str>,
    ) -> Result<(), VelocityError> {
        if let Some(path_str) = app_path {
            let path = Path::new(path_str);

            if let Some(layout_path) = self.resolve_ios_layout_path(path, initial_layout)? {
                let xml = std::fs::read_to_string(&layout_path).map_err(|e| {
                    VelocityError::Internal(anyhow::anyhow!(
                        "Failed to read iOS layout '{}': {}",
                        layout_path.display(),
                        e
                    ))
                })?;

                match layout_path.extension().and_then(|e| e.to_str()) {
                    Some("xib") => {
                        let doc = XibParser::parse(&xml).map_err(|e| {
                            VelocityError::Internal(anyhow::anyhow!("XIB parse failed: {}", e))
                        })?;

                        let inflater = IosInflater::new(IosResourceLoader::empty());
                        let tree = inflater.inflate(&doc).map_err(|e| {
                            VelocityError::Internal(anyhow::anyhow!("XIB inflate failed: {}", e))
                        })?;

                        session.set_render_tree(tree).map_err(|e| {
                            VelocityError::Internal(anyhow::anyhow!("Render failed: {}", e))
                        })?;

                        return Ok(());
                    }
                    Some("storyboard") => {
                        let doc = crate::ios::storyboard::StoryboardParser::parse(&xml).map_err(
                            |e| {
                                VelocityError::Internal(anyhow::anyhow!(
                                    "Storyboard parse failed: {}",
                                    e
                                ))
                            },
                        )?;

                        let inflater = IosInflater::new(IosResourceLoader::empty());
                        let tree = inflater.inflate(&doc).map_err(|e| {
                            VelocityError::Internal(anyhow::anyhow!("XIB inflate failed: {}", e))
                        })?;

                        session.set_render_tree(tree).map_err(|e| {
                            VelocityError::Internal(anyhow::anyhow!("Render failed: {}", e))
                        })?;

                        return Ok(());
                    }
                    Some(other) => {
                        return Err(VelocityError::Config(format!(
                            "Unsupported iOS layout extension '{}'. Expected .xib or .storyboard.",
                            other
                        )));
                    }
                    None => {}
                }
            }

            if path.extension().and_then(|e| e.to_str()) == Some("xib") {
                let xml = std::fs::read_to_string(path).map_err(|e| {
                    VelocityError::Internal(anyhow::anyhow!("Failed to read XIB: {}", e))
                })?;

                let doc = XibParser::parse(&xml).map_err(|e| {
                    VelocityError::Internal(anyhow::anyhow!("XIB parse failed: {}", e))
                })?;

                let inflater = IosInflater::new(IosResourceLoader::empty());
                let tree = inflater.inflate(&doc).map_err(|e| {
                    VelocityError::Internal(anyhow::anyhow!("XIB inflate failed: {}", e))
                })?;

                session.set_render_tree(tree).map_err(|e| {
                    VelocityError::Internal(anyhow::anyhow!("Render failed: {}", e))
                })?;

                return Ok(());
            }

            // Storyboard file
            if path.extension().and_then(|e| e.to_str()) == Some("storyboard") {
                let xml = std::fs::read_to_string(path).map_err(|e| {
                    VelocityError::Internal(anyhow::anyhow!("Failed to read storyboard: {}", e))
                })?;

                let doc =
                    crate::ios::storyboard::StoryboardParser::parse(&xml).map_err(|e| {
                        VelocityError::Internal(anyhow::anyhow!(
                            "Storyboard parse failed: {}",
                            e
                        ))
                    })?;

                let inflater = IosInflater::new(IosResourceLoader::empty());
                let tree = inflater.inflate(&doc).map_err(|e| {
                    VelocityError::Internal(anyhow::anyhow!("XIB inflate failed: {}", e))
                })?;

                session.set_render_tree(tree).map_err(|e| {
                    VelocityError::Internal(anyhow::anyhow!("Render failed: {}", e))
                })?;

                return Ok(());
            }
        }

        // Default empty screen
        let root = crate::render_tree::RenderNode::container("UIView").with_style(
            crate::render_tree::NodeStyle {
                width: taffy::Dimension::length(self.config.width as f32),
                height: taffy::Dimension::length(self.config.height as f32),
                background_color: crate::render_tree::Color::WHITE,
                ..Default::default()
            },
        );

        session
            .set_render_tree(root)
            .map_err(|e| VelocityError::Internal(anyhow::anyhow!("Render failed: {}", e)))?;

        Ok(())
    }

    fn resolve_ios_layout_path(
        &self,
        app_path: &Path,
        initial_layout: Option<&str>,
    ) -> Result<Option<PathBuf>, VelocityError> {
        if app_path.is_file() {
            return Ok(Some(app_path.to_path_buf()));
        }

        if !app_path.is_dir() {
            return Ok(None);
        }

        if let Some(layout_name) = initial_layout {
            return Self::find_ios_layout(app_path, layout_name).map(Some).ok_or_else(|| {
                VelocityError::Config(format!(
                    "iOS layout '{}' not found under {}",
                    layout_name,
                    app_path.display()
                ))
            });
        }

        if let Some(layout_name) = Self::detect_ios_main_layout(app_path) {
            return Ok(Self::find_ios_layout(app_path, &layout_name));
        }

        Ok(None)
    }

    fn detect_ios_main_layout(app_path: &Path) -> Option<String> {
        let info_plist = app_path.join("Info.plist");
        let plist = plist::Value::from_file(&info_plist).ok()?;
        let dict = plist.as_dictionary()?;

        dict.get("UIMainStoryboardFile")
            .and_then(|value| value.as_string())
            .or_else(|| dict.get("NSMainNibFile").and_then(|value| value.as_string()))
            .map(ToOwned::to_owned)
    }

    fn find_ios_layout(root: &Path, layout_name: &str) -> Option<PathBuf> {
        let path = Path::new(layout_name);
        let mut candidates = Vec::new();

        if path.extension().is_some() {
            candidates.push(layout_name.to_string());
        } else {
            candidates.push(format!("{}.xib", layout_name));
            candidates.push(format!("{}.storyboard", layout_name));
        }

        Self::find_ios_layout_recursive(root, &candidates)
    }

    fn find_ios_layout_recursive(root: &Path, candidates: &[String]) -> Option<PathBuf> {
        let entries = std::fs::read_dir(root).ok()?;

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if let Some(found) = Self::find_ios_layout_recursive(&path, candidates) {
                    return Some(found);
                }
                continue;
            }

            let file_name = path.file_name()?.to_str()?;
            if candidates.iter().any(|candidate| candidate == file_name) {
                return Some(path);
            }
        }

        None
    }
}

#[async_trait::async_trait]
impl PlatformDriver for HeadlessDriver {
    async fn prepare(&self, device_id: &str) -> velocity_common::Result<()> {
        let sessions = self.sessions.read().await;
        if !sessions.contains_key(device_id) {
            drop(sessions);
            self.boot_device(device_id).await?;
        }
        Ok(())
    }

    async fn cleanup(&self) {}

    async fn health_check(&self) -> HealthStatus {
        HealthStatus::Healthy
    }

    async fn restart_session(&self) -> velocity_common::Result<()> {
        Ok(())
    }

    async fn list_devices(&self) -> velocity_common::Result<Vec<DeviceInfo>> {
        Ok(vec![DeviceInfo {
            id: "headless-0".to_string(),
            name: format!(
                "Headless Renderer ({}x{})",
                self.config.width, self.config.height
            ),
            platform: self.platform,
            state: velocity_common::DeviceState::Booted,
            os_version: Some("headless".to_string()),
            device_type: velocity_common::DeviceType::Simulator,
        }])
    }

    async fn boot_device(&self, device_id: &str) -> velocity_common::Result<()> {
        let session = HeadlessSession::new(self.config.clone(), self.text_measurer.clone()).map_err(|e| {
            VelocityError::Internal(anyhow::anyhow!("Session creation failed: {}", e))
        })?;

        self.sessions
            .write()
            .await
            .insert(device_id.to_string(), session);

        Ok(())
    }

    async fn shutdown_device(&self, device_id: &str) -> velocity_common::Result<()> {
        self.sessions.write().await.remove(device_id);
        Ok(())
    }

    async fn install_app(
        &self,
        device_id: &str,
        app_path: &str,
    ) -> velocity_common::Result<()> {
        let mut sessions = self.sessions.write().await;
        if let Some(session) = sessions.get_mut(device_id) {
            session.app_path = Some(app_path.to_string());
        }
        Ok(())
    }

    async fn launch_app(
        &self,
        device_id: &str,
        app_id: &str,
        _clear_state: bool,
    ) -> velocity_common::Result<()> {
        // Ensure session exists
        {
            let sessions = self.sessions.read().await;
            if !sessions.contains_key(device_id) {
                drop(sessions);
                self.boot_device(device_id).await?;
            }
        }

        let mut sessions = self.sessions.write().await;
        let session = sessions.get_mut(device_id).ok_or_else(|| {
            VelocityError::DeviceNotFound {
                id: device_id.to_string(),
                available: vec!["headless-0".to_string()],
            }
        })?;

        self.inflate_layout(session, app_id).await?;
        Ok(())
    }

    async fn stop_app(
        &self,
        _device_id: &str,
        _app_id: &str,
    ) -> velocity_common::Result<()> {
        Ok(())
    }

    async fn find_element(
        &self,
        device_id: &str,
        selector: &Selector,
    ) -> velocity_common::Result<Element> {
        let sessions = self.sessions.read().await;
        let session = sessions.get(device_id).ok_or_else(|| {
            VelocityError::DeviceNotFound {
                id: device_id.to_string(),
                available: vec!["headless-0".to_string()],
            }
        })?;

        let hierarchy = session.get_hierarchy().ok_or_else(|| {
            VelocityError::Internal(anyhow::anyhow!("No layout loaded"))
        })?;

        let screen = Rect {
            x: 0,
            y: 0,
            width: self.config.width as i32,
            height: self.config.height as i32,
        };

        find_in_tree(hierarchy, selector, Some(&screen)).ok_or_else(|| {
            VelocityError::ElementNotFound {
                selector: format!("{:?}", selector),
                timeout_ms: 0,
                screenshot: None,
                hierarchy_snapshot: None,
            }
        })
    }

    async fn find_elements(
        &self,
        device_id: &str,
        selector: &Selector,
    ) -> velocity_common::Result<Vec<Element>> {
        let sessions = self.sessions.read().await;
        let session = sessions.get(device_id).ok_or_else(|| {
            VelocityError::DeviceNotFound {
                id: device_id.to_string(),
                available: vec!["headless-0".to_string()],
            }
        })?;

        let hierarchy = session.get_hierarchy().ok_or_else(|| {
            VelocityError::Internal(anyhow::anyhow!("No layout loaded"))
        })?;

        let screen = Rect {
            x: 0,
            y: 0,
            width: self.config.width as i32,
            height: self.config.height as i32,
        };

        let mut results = Vec::new();
        find_all_in_tree(hierarchy, selector, Some(&screen), &mut results);
        Ok(results)
    }

    async fn get_hierarchy(
        &self,
        device_id: &str,
    ) -> velocity_common::Result<Element> {
        let sessions = self.sessions.read().await;
        let session = sessions.get(device_id).ok_or_else(|| {
            VelocityError::DeviceNotFound {
                id: device_id.to_string(),
                available: vec!["headless-0".to_string()],
            }
        })?;

        session.get_hierarchy().cloned().ok_or_else(|| {
            VelocityError::Internal(anyhow::anyhow!("No layout loaded"))
        })
    }

    async fn tap(
        &self,
        _device_id: &str,
        _element: &Element,
    ) -> velocity_common::Result<()> {
        // No-op for static layouts
        Ok(())
    }

    async fn double_tap(
        &self,
        _device_id: &str,
        _element: &Element,
    ) -> velocity_common::Result<()> {
        Ok(())
    }

    async fn long_press(
        &self,
        _device_id: &str,
        _element: &Element,
        _duration_ms: u64,
    ) -> velocity_common::Result<()> {
        Ok(())
    }

    async fn input_text(
        &self,
        device_id: &str,
        element: &Element,
        text: &str,
    ) -> velocity_common::Result<()> {
        let mut sessions = self.sessions.write().await;
        let session = sessions.get_mut(device_id).ok_or_else(|| VelocityError::DeviceNotFound {
            id: device_id.to_string(),
            available: vec!["headless-0".to_string()],
        })?;
        let element_id = element
            .platform_id
            .strip_prefix("headless::")
            .unwrap_or(&element.platform_id);
        session
            .update_text(element_id, text)
            .map_err(|e| VelocityError::Internal(anyhow::anyhow!("Failed to update text: {}", e)))
    }

    async fn clear_text(
        &self,
        device_id: &str,
        element: &Element,
    ) -> velocity_common::Result<()> {
        self.input_text(device_id, element, "").await
    }

    async fn swipe(
        &self,
        _device_id: &str,
        _direction: Direction,
    ) -> velocity_common::Result<()> {
        Ok(())
    }

    async fn swipe_coords(
        &self,
        _device_id: &str,
        _from: (i32, i32),
        _to: (i32, i32),
    ) -> velocity_common::Result<()> {
        Ok(())
    }

    async fn press_key(
        &self,
        _device_id: &str,
        _key: Key,
    ) -> velocity_common::Result<()> {
        Ok(())
    }

    async fn screenshot(
        &self,
        device_id: &str,
    ) -> velocity_common::Result<Vec<u8>> {
        let sessions = self.sessions.read().await;
        let session = sessions.get(device_id).ok_or_else(|| {
            VelocityError::DeviceNotFound {
                id: device_id.to_string(),
                available: vec!["headless-0".to_string()],
            }
        })?;

        session.screenshot().map_err(|e| {
            VelocityError::Internal(anyhow::anyhow!("Screenshot failed: {}", e))
        })
    }

    async fn screen_size(
        &self,
        device_id: &str,
    ) -> velocity_common::Result<(i32, i32)> {
        let sessions = self.sessions.read().await;
        if let Some(session) = sessions.get(device_id) {
            Ok(session.screen_size())
        } else {
            Ok((self.config.width as i32, self.config.height as i32))
        }
    }

    async fn get_element_text(
        &self,
        device_id: &str,
        element: &Element,
    ) -> velocity_common::Result<String> {
        Ok(element.text.clone().unwrap_or_default())
    }

    async fn is_element_visible(
        &self,
        _device_id: &str,
        element: &Element,
    ) -> velocity_common::Result<bool> {
        Ok(element.visible)
    }

    async fn collect_resource_metrics(
        &self,
        _device_id: &str,
        _package: &str,
    ) -> velocity_common::Result<(u64, u64, u64, f32)> {
        Ok((0, 0, 0, 0.0))
    }
}

