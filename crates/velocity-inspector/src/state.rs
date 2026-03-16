use std::sync::Arc;

use tokio::sync::RwLock;
use velocity_common::PlatformDriver;

/// Shared application state for the inspector server.
pub struct AppState {
    pub driver: Arc<dyn PlatformDriver>,
    pub current_device: RwLock<Option<String>>,
}

impl AppState {
    pub fn new(driver: Arc<dyn PlatformDriver>, device_id: Option<String>) -> Self {
        Self {
            driver,
            current_device: RwLock::new(device_id),
        }
    }

    pub async fn device_id(&self) -> Option<String> {
        self.current_device.read().await.clone()
    }

    pub async fn require_device_id(&self) -> anyhow::Result<String> {
        self.device_id()
            .await
            .ok_or_else(|| anyhow::anyhow!("No device selected"))
    }
}
