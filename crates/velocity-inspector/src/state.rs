use std::sync::Arc;

use tokio::sync::RwLock;
use velocity_common::PlatformDriver;

/// Shared application state for the inspector server.
pub struct AppState {
    pub driver: Arc<dyn PlatformDriver>,
    pub current_device: RwLock<Option<String>>,
    /// App package ID for resource profiling (set via API when known).
    pub app_id: RwLock<Option<String>>,
}

impl AppState {
    pub fn new(
        driver: Arc<dyn PlatformDriver>,
        device_id: Option<String>,
        app_id: Option<String>,
    ) -> Self {
        Self {
            driver,
            current_device: RwLock::new(device_id),
            app_id: RwLock::new(app_id),
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

    pub async fn app_id(&self) -> Option<String> {
        self.app_id.read().await.clone()
    }

    pub async fn set_app_id(&self, app_id: Option<String>) {
        *self.app_id.write().await = app_id;
    }

    pub async fn clear_app_id(&self) {
        self.set_app_id(None).await;
    }
}
