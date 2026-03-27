use base64::Engine;
use tokio::sync::RwLock;
use velocity_common::{
    DeviceInfo, DeviceState, DeviceType, Direction, Element, HealthStatus, Key,
    Platform, PlatformDriver, Rect, Selector, VelocityError,
};
use velocity_common::selector_match::{find_all_in_tree, find_in_tree};

use crate::config::FlutterBridgeConfig;
use crate::process::FlutterProcess;
use crate::protocol::{FlutterCommand, FlutterElement, FlutterResponse};

/// Flutter platform driver that communicates with a Dart test process
/// running `flutter test --machine` + WidgetTester.
pub struct FlutterDriver {
    platform: Platform,
    config: FlutterBridgeConfig,
    process: RwLock<FlutterProcess>,
    hierarchy_cache: RwLock<Option<Element>>,
    screenshot_cache: RwLock<Option<Vec<u8>>>,
}

impl FlutterDriver {
    /// Create a new Flutter driver.
    pub fn new(platform: Platform, config: FlutterBridgeConfig) -> Self {
        let process = FlutterProcess::new(config.clone());
        Self {
            platform,
            config,
            process: RwLock::new(process),
            hierarchy_cache: RwLock::new(None),
            screenshot_cache: RwLock::new(None),
        }
    }

    /// Send a command to the Flutter test process.
    async fn send_cmd(&self, cmd: &FlutterCommand) -> velocity_common::Result<FlutterResponse> {
        let mut process = self.process.write().await;
        process.send(cmd).await.map_err(|e| {
            VelocityError::Internal(anyhow::anyhow!("Flutter bridge command failed: {}", e))
        })
    }

    /// Invalidate cached hierarchy and screenshot so the next query fetches fresh data.
    async fn invalidate_cache(&self) {
        *self.hierarchy_cache.write().await = None;
        *self.screenshot_cache.write().await = None;
    }

    /// Send PumpFrames after an interaction to advance Flutter's frame pipeline.
    async fn pump(&self) -> velocity_common::Result<()> {
        let resp = self.send_cmd(&FlutterCommand::PumpFrames { count: 1 }).await?;
        match resp {
            FlutterResponse::Ok { .. } => Ok(()),
            FlutterResponse::Error { message } => {
                Err(VelocityError::Internal(anyhow::anyhow!("PumpFrames failed: {}", message)))
            }
        }
    }

    /// Fetch and cache the hierarchy from the Flutter process.
    async fn fetch_hierarchy(&self) -> velocity_common::Result<Element> {
        {
            let cache = self.hierarchy_cache.read().await;
            if let Some(ref cached) = *cache {
                return Ok(cached.clone());
            }
        }

        let resp = self.send_cmd(&FlutterCommand::GetHierarchy).await?;
        match resp {
            FlutterResponse::Ok { data: Some(value) } => {
                let flutter_el: FlutterElement = serde_json::from_value(value).map_err(|e| {
                    VelocityError::Internal(anyhow::anyhow!("Failed to parse hierarchy: {}", e))
                })?;
                let element = flutter_el.to_element();
                *self.hierarchy_cache.write().await = Some(element.clone());
                Ok(element)
            }
            FlutterResponse::Ok { data: None } => {
                Err(VelocityError::Internal(anyhow::anyhow!("GetHierarchy returned no data")))
            }
            FlutterResponse::Error { message } => {
                Err(VelocityError::Internal(anyhow::anyhow!("GetHierarchy failed: {}", message)))
            }
        }
    }

    /// Fetch and cache a screenshot from the Flutter process.
    async fn fetch_screenshot(&self) -> velocity_common::Result<Vec<u8>> {
        {
            let cache = self.screenshot_cache.read().await;
            if let Some(ref cached) = *cache {
                return Ok(cached.clone());
            }
        }

        let resp = self.send_cmd(&FlutterCommand::Screenshot).await?;
        match resp {
            FlutterResponse::Ok { data: Some(value) } => {
                let b64 = value.as_str().ok_or_else(|| {
                    VelocityError::Internal(anyhow::anyhow!("Screenshot data is not a string"))
                })?;
                let bytes = base64::engine::general_purpose::STANDARD.decode(b64).map_err(|e| {
                    VelocityError::Internal(anyhow::anyhow!("Failed to decode screenshot base64: {}", e))
                })?;
                *self.screenshot_cache.write().await = Some(bytes.clone());
                Ok(bytes)
            }
            FlutterResponse::Ok { data: None } => {
                Err(VelocityError::Internal(anyhow::anyhow!("Screenshot returned no data")))
            }
            FlutterResponse::Error { message } => {
                Err(VelocityError::Internal(anyhow::anyhow!("Screenshot failed: {}", message)))
            }
        }
    }
}


// --- PlatformDriver implementation ---

#[async_trait::async_trait]
impl PlatformDriver for FlutterDriver {
    async fn prepare(&self, _device_id: &str) -> velocity_common::Result<()> {
        let mut process = self.process.write().await;
        process.start().await.map_err(|e| {
            VelocityError::Internal(anyhow::anyhow!("Flutter process start failed: {}", e))
        })?;

        drop(process);

        // Send Init command to boot the widget
        let resp = self.send_cmd(&FlutterCommand::Init {
            target: self.config.target.clone(),
            width: self.config.width,
            height: self.config.height,
        }).await?;

        match resp {
            FlutterResponse::Ok { .. } => {
                tracing::info!("Flutter bridge initialized");
                Ok(())
            }
            FlutterResponse::Error { message } => {
                Err(VelocityError::Internal(anyhow::anyhow!("Flutter Init failed: {}", message)))
            }
        }
    }

    async fn cleanup(&self) {
        let mut process = self.process.write().await;
        process.stop().await;
    }

    async fn health_check(&self) -> HealthStatus {
        let process = self.process.read().await;
        if process.is_running() {
            HealthStatus::Healthy
        } else {
            HealthStatus::Unhealthy
        }
    }

    async fn restart_session(&self) -> velocity_common::Result<()> {
        self.cleanup().await;
        self.invalidate_cache().await;
        self.prepare("flutter-0").await
    }

    async fn list_devices(&self) -> velocity_common::Result<Vec<DeviceInfo>> {
        Ok(vec![DeviceInfo {
            id: "flutter-0".to_string(),
            name: format!(
                "Flutter Headless ({}x{})",
                self.config.width, self.config.height
            ),
            platform: self.platform,
            state: DeviceState::Booted,
            os_version: Some("flutter".to_string()),
            device_type: DeviceType::Simulator,
        }])
    }

    async fn boot_device(&self, device_id: &str) -> velocity_common::Result<()> {
        self.prepare(device_id).await
    }

    async fn shutdown_device(&self, _device_id: &str) -> velocity_common::Result<()> {
        self.cleanup().await;
        Ok(())
    }

    async fn install_app(
        &self,
        _device_id: &str,
        _app_path: &str,
    ) -> velocity_common::Result<()> {
        // Flutter apps are loaded via the test harness target, no separate install step.
        Ok(())
    }

    async fn launch_app(
        &self,
        device_id: &str,
        _app_id: &str,
        _clear_state: bool,
    ) -> velocity_common::Result<()> {
        // Ensure process is connected and initialized.
        let process = self.process.read().await;
        if !process.is_running() {
            drop(process);
            self.prepare(device_id).await?;
        }
        self.invalidate_cache().await;
        Ok(())
    }

    async fn stop_app(
        &self,
        _device_id: &str,
        _app_id: &str,
    ) -> velocity_common::Result<()> {
        self.cleanup().await;
        Ok(())
    }

    async fn find_element(
        &self,
        _device_id: &str,
        selector: &Selector,
    ) -> velocity_common::Result<Element> {
        let hierarchy = self.fetch_hierarchy().await?;
        let screen = Rect {
            x: 0,
            y: 0,
            width: self.config.width as i32,
            height: self.config.height as i32,
        };

        find_in_tree(&hierarchy, selector, Some(&screen)).ok_or_else(|| {
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
        _device_id: &str,
        selector: &Selector,
    ) -> velocity_common::Result<Vec<Element>> {
        let hierarchy = self.fetch_hierarchy().await?;
        let screen = Rect {
            x: 0,
            y: 0,
            width: self.config.width as i32,
            height: self.config.height as i32,
        };

        let mut results = Vec::new();
        find_all_in_tree(&hierarchy, selector, Some(&screen), &mut results);
        Ok(results)
    }

    async fn get_hierarchy(
        &self,
        _device_id: &str,
    ) -> velocity_common::Result<Element> {
        self.fetch_hierarchy().await
    }

    async fn tap(
        &self,
        _device_id: &str,
        element: &Element,
    ) -> velocity_common::Result<()> {
        let (cx, cy) = element.bounds.center();
        let resp = self.send_cmd(&FlutterCommand::Tap { x: cx, y: cy }).await?;
        match resp {
            FlutterResponse::Ok { .. } => {}
            FlutterResponse::Error { message } => {
                return Err(VelocityError::Internal(anyhow::anyhow!("Tap failed: {}", message)));
            }
        }
        self.pump().await?;
        self.invalidate_cache().await;
        Ok(())
    }

    async fn double_tap(
        &self,
        _device_id: &str,
        element: &Element,
    ) -> velocity_common::Result<()> {
        let (cx, cy) = element.bounds.center();
        let resp = self.send_cmd(&FlutterCommand::DoubleTap { x: cx, y: cy }).await?;
        match resp {
            FlutterResponse::Ok { .. } => {}
            FlutterResponse::Error { message } => {
                return Err(VelocityError::Internal(anyhow::anyhow!("DoubleTap failed: {}", message)));
            }
        }
        self.pump().await?;
        self.invalidate_cache().await;
        Ok(())
    }

    async fn long_press(
        &self,
        _device_id: &str,
        element: &Element,
        duration_ms: u64,
    ) -> velocity_common::Result<()> {
        let (cx, cy) = element.bounds.center();
        let resp = self.send_cmd(&FlutterCommand::LongPress {
            x: cx,
            y: cy,
            duration_ms,
        }).await?;
        match resp {
            FlutterResponse::Ok { .. } => {}
            FlutterResponse::Error { message } => {
                return Err(VelocityError::Internal(anyhow::anyhow!("LongPress failed: {}", message)));
            }
        }
        self.pump().await?;
        self.invalidate_cache().await;
        Ok(())
    }

    async fn input_text(
        &self,
        _device_id: &str,
        _element: &Element,
        text: &str,
    ) -> velocity_common::Result<()> {
        let resp = self.send_cmd(&FlutterCommand::InputText {
            text: text.to_string(),
        }).await?;
        match resp {
            FlutterResponse::Ok { .. } => {}
            FlutterResponse::Error { message } => {
                return Err(VelocityError::Internal(anyhow::anyhow!("InputText failed: {}", message)));
            }
        }
        self.pump().await?;
        self.invalidate_cache().await;
        Ok(())
    }

    async fn clear_text(
        &self,
        _device_id: &str,
        _element: &Element,
    ) -> velocity_common::Result<()> {
        let resp = self.send_cmd(&FlutterCommand::ClearText).await?;
        match resp {
            FlutterResponse::Ok { .. } => {}
            FlutterResponse::Error { message } => {
                return Err(VelocityError::Internal(anyhow::anyhow!("ClearText failed: {}", message)));
            }
        }
        self.pump().await?;
        self.invalidate_cache().await;
        Ok(())
    }

    async fn swipe(
        &self,
        _device_id: &str,
        direction: Direction,
    ) -> velocity_common::Result<()> {
        let mid_x = self.config.width as i32 / 2;
        let mid_y = self.config.height as i32 / 2;
        let distance = self.config.height as i32 / 3;

        let (from_x, from_y, to_x, to_y) = match direction {
            Direction::Up => (mid_x, mid_y + distance / 2, mid_x, mid_y - distance / 2),
            Direction::Down => (mid_x, mid_y - distance / 2, mid_x, mid_y + distance / 2),
            Direction::Left => (mid_x + distance / 2, mid_y, mid_x - distance / 2, mid_y),
            Direction::Right => (mid_x - distance / 2, mid_y, mid_x + distance / 2, mid_y),
        };

        let resp = self.send_cmd(&FlutterCommand::Swipe {
            from_x, from_y, to_x, to_y,
        }).await?;
        match resp {
            FlutterResponse::Ok { .. } => {}
            FlutterResponse::Error { message } => {
                return Err(VelocityError::Internal(anyhow::anyhow!("Swipe failed: {}", message)));
            }
        }
        self.pump().await?;
        self.invalidate_cache().await;
        Ok(())
    }

    async fn swipe_coords(
        &self,
        _device_id: &str,
        from: (i32, i32),
        to: (i32, i32),
    ) -> velocity_common::Result<()> {
        let resp = self.send_cmd(&FlutterCommand::Swipe {
            from_x: from.0,
            from_y: from.1,
            to_x: to.0,
            to_y: to.1,
        }).await?;
        match resp {
            FlutterResponse::Ok { .. } => {}
            FlutterResponse::Error { message } => {
                return Err(VelocityError::Internal(anyhow::anyhow!("SwipeCoords failed: {}", message)));
            }
        }
        self.pump().await?;
        self.invalidate_cache().await;
        Ok(())
    }

    async fn press_key(
        &self,
        _device_id: &str,
        key: Key,
    ) -> velocity_common::Result<()> {
        let key_str = match key {
            Key::Back => "back",
            Key::Home => "home",
            Key::Enter => "enter",
            Key::VolumeUp => "volumeUp",
            Key::VolumeDown => "volumeDown",
        };
        let resp = self.send_cmd(&FlutterCommand::PressKey {
            key: key_str.to_string(),
        }).await?;
        match resp {
            FlutterResponse::Ok { .. } => {}
            FlutterResponse::Error { message } => {
                return Err(VelocityError::Internal(anyhow::anyhow!("PressKey failed: {}", message)));
            }
        }
        self.pump().await?;
        self.invalidate_cache().await;
        Ok(())
    }

    async fn screenshot(
        &self,
        _device_id: &str,
    ) -> velocity_common::Result<Vec<u8>> {
        self.fetch_screenshot().await
    }

    async fn screen_size(
        &self,
        _device_id: &str,
    ) -> velocity_common::Result<(i32, i32)> {
        Ok((self.config.width as i32, self.config.height as i32))
    }

    async fn get_element_text(
        &self,
        _device_id: &str,
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
        Err(VelocityError::Config(
            "Resource profiling not available on Flutter headless bridge".into(),
        ))
    }
}
