use crate::error::Result;
use crate::types::{DeviceInfo, Direction, Element, Key, Selector};

/// Health status of a platform driver connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
}

/// The core trait that platform drivers must implement.
/// Each platform (iOS, Android) provides its own implementation.
#[async_trait::async_trait]
pub trait PlatformDriver: Send + Sync {
    // Platform bootstrap — override to auto-start required services (e.g. WDA on iOS)
    async fn prepare(&self, _device_id: &str) -> Result<()> {
        Ok(())
    }
    async fn cleanup(&self) {}

    // Health check for connection validation
    async fn health_check(&self) -> HealthStatus {
        HealthStatus::Healthy
    }

    // Explicit session restart for recovery
    async fn restart_session(&self) -> Result<()> {
        Ok(())
    }

    // Device management
    async fn list_devices(&self) -> Result<Vec<DeviceInfo>>;
    async fn boot_device(&self, device_id: &str) -> Result<()>;
    async fn shutdown_device(&self, device_id: &str) -> Result<()>;

    // App lifecycle
    async fn install_app(&self, device_id: &str, app_path: &str) -> Result<()>;
    async fn launch_app(&self, device_id: &str, app_id: &str, clear_state: bool) -> Result<()>;
    async fn stop_app(&self, device_id: &str, app_id: &str) -> Result<()>;

    // Element queries
    async fn find_element(&self, device_id: &str, selector: &Selector) -> Result<Element>;
    async fn find_elements(&self, device_id: &str, selector: &Selector) -> Result<Vec<Element>>;
    async fn get_hierarchy(&self, device_id: &str) -> Result<Element>;

    // Actions
    async fn tap(&self, device_id: &str, element: &Element) -> Result<()>;
    async fn double_tap(&self, device_id: &str, element: &Element) -> Result<()>;
    async fn long_press(&self, device_id: &str, element: &Element, duration_ms: u64) -> Result<()>;
    async fn input_text(&self, device_id: &str, element: &Element, text: &str) -> Result<()>;
    async fn clear_text(&self, device_id: &str, element: &Element) -> Result<()>;
    async fn swipe(&self, device_id: &str, direction: Direction) -> Result<()>;
    async fn swipe_coords(&self, device_id: &str, from: (i32, i32), to: (i32, i32)) -> Result<()>;
    async fn press_key(&self, device_id: &str, key: Key) -> Result<()>;

    // Screen
    async fn screenshot(&self, device_id: &str) -> Result<Vec<u8>>;
    async fn screen_size(&self, device_id: &str) -> Result<(i32, i32)>;

    // Element state
    async fn get_element_text(&self, device_id: &str, element: &Element) -> Result<String>;
    async fn is_element_visible(&self, device_id: &str, element: &Element) -> Result<bool>;
}
