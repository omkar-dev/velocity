use std::path::PathBuf;
use std::time::{Duration, Instant};

use velocity_common::{DeviceInfo, PlatformDriver, Result};

/// Tracks state across an MCP session for caching and context.
pub struct McpSession {
    /// Currently selected device ID.
    pub current_device: Option<String>,

    /// Cached device list with expiration.
    device_list_cache: Option<(Vec<DeviceInfo>, Instant)>,

    /// Path to last captured screenshot.
    pub last_screenshot: Option<PathBuf>,

    /// Path to the velocity config file, if available.
    pub config_path: Option<String>,

    /// Cache TTL for device list queries.
    cache_ttl: Duration,

    /// Port the inspector is running on, if started.
    pub inspector_port: Option<u16>,

    /// Handle to the inspector background task.
    pub inspector_handle: Option<tokio::task::JoinHandle<()>>,
}

impl McpSession {
    pub fn new(device_id: Option<String>, config_path: Option<String>) -> Self {
        Self {
            current_device: device_id,
            device_list_cache: None,
            last_screenshot: None,
            config_path,
            cache_ttl: Duration::from_secs(30),
            inspector_port: None,
            inspector_handle: None,
        }
    }

    /// Get device list, using cache if still valid.
    pub async fn get_devices(&mut self, driver: &dyn PlatformDriver) -> Result<Vec<DeviceInfo>> {
        if let Some((ref devices, timestamp)) = self.device_list_cache {
            if timestamp.elapsed() < self.cache_ttl {
                return Ok(devices.clone());
            }
        }

        let devices = driver.list_devices().await?;
        self.device_list_cache = Some((devices.clone(), Instant::now()));
        Ok(devices)
    }

    /// Invalidate the device list cache (e.g. after boot/shutdown).
    pub fn invalidate_device_cache(&mut self) {
        self.device_list_cache = None;
    }

    /// Get the current device ID, falling back to auto-detection.
    pub async fn resolve_device_id(&mut self, driver: &dyn PlatformDriver) -> Result<String> {
        if let Some(ref id) = self.current_device {
            return Ok(id.clone());
        }

        let devices = self.get_devices(driver).await?;
        let id = devices
            .first()
            .ok_or_else(|| {
                velocity_common::VelocityError::Config(
                    "No devices found. Boot a device first.".to_string(),
                )
            })?
            .id
            .clone();

        self.current_device = Some(id.clone());
        Ok(id)
    }
}
