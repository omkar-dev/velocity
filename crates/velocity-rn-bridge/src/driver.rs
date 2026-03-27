use tokio::sync::RwLock;
use velocity_common::{
    DeviceInfo, DeviceState, DeviceType, Direction, Element, HealthStatus, Key, Platform,
    PlatformDriver, Selector, VelocityError,
};
use velocity_common::selector_match::{find_all_in_tree, find_in_tree};

use crate::client::RnBridgeClient;
use crate::config::RnBridgeConfig;
use crate::protocol::{BridgeCommand, BridgeResponse, SidecarElement};

pub struct RnDriver {
    platform: Platform,
    config: RnBridgeConfig,
    client: RwLock<RnBridgeClient>,
    hierarchy_cache: RwLock<Option<Element>>,
    screenshot_cache: RwLock<Option<Vec<u8>>>,
}

impl RnDriver {
    pub fn new(platform: Platform, config: RnBridgeConfig) -> Self {
        let client = RnBridgeClient::new(config.clone());
        Self {
            platform,
            config,
            client: RwLock::new(client),
            hierarchy_cache: RwLock::new(None),
            screenshot_cache: RwLock::new(None),
        }
    }

    async fn send_cmd(&self, cmd: &BridgeCommand) -> velocity_common::Result<BridgeResponse> {
        let mut client = self.client.write().await;
        client
            .send(cmd)
            .await
            .map_err(|e| VelocityError::Internal(anyhow::anyhow!("RN bridge error: {e}")))
    }

    async fn invalidate_cache(&self) {
        *self.hierarchy_cache.write().await = None;
        *self.screenshot_cache.write().await = None;
    }

}

#[async_trait::async_trait]
impl PlatformDriver for RnDriver {
    async fn prepare(&self, _device_id: &str) -> velocity_common::Result<()> {
        let mut client = self.client.write().await;
        client.connect().await.map_err(|e| {
            VelocityError::Internal(anyhow::anyhow!("Failed to connect to RN sidecar: {e}"))
        })?;
        Ok(())
    }

    async fn cleanup(&self) {
        let mut client = self.client.write().await;
        client.disconnect().await;
    }

    async fn health_check(&self) -> HealthStatus {
        let client = self.client.read().await;
        if client.is_connected() {
            HealthStatus::Healthy
        } else {
            HealthStatus::Unhealthy
        }
    }

    async fn list_devices(&self) -> velocity_common::Result<Vec<DeviceInfo>> {
        Ok(vec![DeviceInfo {
            id: "rn-0".to_string(),
            name: format!(
                "React Native Headless ({}x{})",
                self.config.width, self.config.height
            ),
            platform: self.platform,
            state: DeviceState::Booted,
            os_version: Some("headless-rn".to_string()),
            device_type: DeviceType::Simulator,
        }])
    }

    async fn boot_device(&self, _device_id: &str) -> velocity_common::Result<()> {
        Ok(())
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
        Ok(())
    }

    async fn launch_app(
        &self,
        _device_id: &str,
        _app_id: &str,
        _clear_state: bool,
    ) -> velocity_common::Result<()> {
        let cmd = BridgeCommand::Init {
            bundle_path: self.config.bundle_path.clone(),
            component: self.config.component.clone(),
            width: self.config.width,
            height: self.config.height,
            native_mocks: self.config.native_mocks.clone(),
        };
        let resp = self.send_cmd(&cmd).await?;
        match resp {
            BridgeResponse::Ok { .. } => {
                self.invalidate_cache().await;
                Ok(())
            }
            BridgeResponse::Error { message } => Err(VelocityError::Internal(anyhow::anyhow!(
                "RN init failed: {message}"
            ))),
        }
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
        let hierarchy = self.get_hierarchy(device_id).await?;
        find_in_tree(&hierarchy, selector, None).ok_or_else(|| VelocityError::ElementNotFound {
            selector: format!("{selector}"),
            timeout_ms: 0,
            screenshot: None,
            hierarchy_snapshot: None,
        })
    }

    async fn find_elements(
        &self,
        device_id: &str,
        selector: &Selector,
    ) -> velocity_common::Result<Vec<Element>> {
        let hierarchy = self.get_hierarchy(device_id).await?;
        let mut results = Vec::new();
        find_all_in_tree(&hierarchy, selector, None, &mut results);
        Ok(results)
    }

    async fn get_hierarchy(&self, _device_id: &str) -> velocity_common::Result<Element> {
        // Check cache first
        if let Some(cached) = self.hierarchy_cache.read().await.as_ref() {
            return Ok(cached.clone());
        }

        let resp = self.send_cmd(&BridgeCommand::GetHierarchy).await?;
        match resp {
            BridgeResponse::Ok { data: Some(data) } => {
                let sidecar_elem: SidecarElement = serde_json::from_value(data).map_err(|e| {
                    VelocityError::Internal(anyhow::anyhow!("Failed to parse hierarchy: {e}"))
                })?;
                let element = sidecar_elem.to_element();
                *self.hierarchy_cache.write().await = Some(element.clone());
                Ok(element)
            }
            BridgeResponse::Ok { data: None } => Err(VelocityError::Internal(anyhow::anyhow!(
                "Sidecar returned empty hierarchy"
            ))),
            BridgeResponse::Error { message } => Err(VelocityError::Internal(anyhow::anyhow!(
                "Hierarchy fetch failed: {message}"
            ))),
        }
    }

    async fn tap(&self, _device_id: &str, element: &Element) -> velocity_common::Result<()> {
        let (cx, cy) = element.bounds.center();
        self.send_cmd(&BridgeCommand::Tap { x: cx, y: cy })
            .await?;
        self.invalidate_cache().await;
        Ok(())
    }

    async fn double_tap(
        &self,
        device_id: &str,
        element: &Element,
    ) -> velocity_common::Result<()> {
        self.tap(device_id, element).await?;
        self.tap(device_id, element).await
    }

    async fn long_press(
        &self,
        device_id: &str,
        element: &Element,
        _duration_ms: u64,
    ) -> velocity_common::Result<()> {
        self.tap(device_id, element).await
    }

    async fn input_text(
        &self,
        _device_id: &str,
        _element: &Element,
        text: &str,
    ) -> velocity_common::Result<()> {
        self.send_cmd(&BridgeCommand::InputText {
            text: text.to_string(),
        })
        .await?;
        self.invalidate_cache().await;
        Ok(())
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
        direction: Direction,
    ) -> velocity_common::Result<()> {
        let (w, h) = (self.config.width as i32, self.config.height as i32);
        let (fx, fy, tx, ty) = match direction {
            Direction::Up => (w / 2, h * 3 / 4, w / 2, h / 4),
            Direction::Down => (w / 2, h / 4, w / 2, h * 3 / 4),
            Direction::Left => (w * 3 / 4, h / 2, w / 4, h / 2),
            Direction::Right => (w / 4, h / 2, w * 3 / 4, h / 2),
        };
        self.send_cmd(&BridgeCommand::Swipe {
            from_x: fx,
            from_y: fy,
            to_x: tx,
            to_y: ty,
        })
        .await?;
        self.invalidate_cache().await;
        Ok(())
    }

    async fn swipe_coords(
        &self,
        _device_id: &str,
        from: (i32, i32),
        to: (i32, i32),
    ) -> velocity_common::Result<()> {
        self.send_cmd(&BridgeCommand::Swipe {
            from_x: from.0,
            from_y: from.1,
            to_x: to.0,
            to_y: to.1,
        })
        .await?;
        self.invalidate_cache().await;
        Ok(())
    }

    async fn press_key(&self, _device_id: &str, _key: Key) -> velocity_common::Result<()> {
        Ok(())
    }

    async fn screenshot(&self, _device_id: &str) -> velocity_common::Result<Vec<u8>> {
        if let Some(cached) = self.screenshot_cache.read().await.as_ref() {
            return Ok(cached.clone());
        }

        let resp = self.send_cmd(&BridgeCommand::Screenshot).await?;
        match resp {
            BridgeResponse::Ok { data: Some(data) } => {
                let b64 = data.as_str().ok_or_else(|| {
                    VelocityError::Internal(anyhow::anyhow!("Screenshot data is not a string"))
                })?;
                let bytes = base64_decode(b64)?;
                *self.screenshot_cache.write().await = Some(bytes.clone());
                Ok(bytes)
            }
            BridgeResponse::Ok { data: None } => Err(VelocityError::Internal(anyhow::anyhow!(
                "Sidecar returned empty screenshot"
            ))),
            BridgeResponse::Error { message } => Err(VelocityError::Internal(anyhow::anyhow!(
                "Screenshot failed: {message}"
            ))),
        }
    }

    async fn screen_size(&self, _device_id: &str) -> velocity_common::Result<(i32, i32)> {
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
        Ok(element.visible && !element.bounds.is_empty())
    }
}

fn base64_decode(input: &str) -> velocity_common::Result<Vec<u8>> {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD
        .decode(input)
        .map_err(|e| VelocityError::Internal(anyhow::anyhow!("Base64 decode failed: {e}")))
}
