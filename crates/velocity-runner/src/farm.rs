use std::collections::HashMap;
use std::sync::Arc;

use tokio::sync::{Mutex, Semaphore, OwnedSemaphorePermit};
use tracing::{debug, warn};
use velocity_common::{DeviceInfo, DeviceState, PlatformDriver, Result, VelocityError};

/// A pooled device that can be leased for test execution.
#[derive(Debug, Clone)]
pub struct PooledDevice {
    pub info: DeviceInfo,
    pub in_use: bool,
}

/// A lease on a device from the farm. Automatically returns the device when dropped.
pub struct Lease {
    device_id: String,
    farm: Arc<Mutex<FarmState>>,
    _permit: OwnedSemaphorePermit,
}

impl Lease {
    pub fn device_id(&self) -> &str {
        &self.device_id
    }
}

impl Drop for Lease {
    fn drop(&mut self) {
        let farm = self.farm.clone();
        let device_id = self.device_id.clone();
        tokio::spawn(async move {
            let mut state = farm.lock().await;
            if let Some(device) = state.devices.get_mut(&device_id) {
                device.in_use = false;
                debug!(device = %device_id, "device returned to pool");
            }
        });
    }
}

struct FarmState {
    devices: HashMap<String, PooledDevice>,
}

/// Manages a pool of devices for parallel test execution.
///
/// Devices are discovered from the platform driver on `refresh()`, and can be
/// leased exclusively via `acquire()`. When the `Lease` is dropped, the device
/// is automatically returned to the pool.
pub struct DeviceFarm {
    driver: Arc<dyn PlatformDriver>,
    state: Arc<Mutex<FarmState>>,
    semaphore: Arc<Semaphore>,
    max_devices: usize,
}

impl DeviceFarm {
    pub fn new(driver: Arc<dyn PlatformDriver>, max_devices: usize) -> Self {
        Self {
            driver,
            state: Arc::new(Mutex::new(FarmState {
                devices: HashMap::new(),
            })),
            semaphore: Arc::new(Semaphore::new(max_devices)),
            max_devices,
        }
    }

    /// Discover available devices from the driver and populate the pool.
    pub async fn refresh(&self) -> Result<usize> {
        let devices = self.driver.list_devices().await?;
        let mut state = self.state.lock().await;

        let booted: Vec<DeviceInfo> = devices
            .into_iter()
            .filter(|d| d.state == DeviceState::Booted)
            .take(self.max_devices)
            .collect();

        let count = booted.len();

        // Keep existing in_use status for devices that are still present
        let mut new_devices = HashMap::new();
        for info in booted {
            let in_use = state
                .devices
                .get(&info.id)
                .map(|d| d.in_use)
                .unwrap_or(false);
            new_devices.insert(info.id.clone(), PooledDevice { info, in_use });
        }

        state.devices = new_devices;
        debug!(count, "device pool refreshed");
        Ok(count)
    }

    /// Acquire a lease on an available device. Blocks until a device is free.
    pub async fn acquire(&self) -> Result<Lease> {
        let permit = self
            .semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|_| VelocityError::Config("Device pool semaphore closed".to_string()))?;

        let mut state = self.state.lock().await;
        let device_id = state
            .devices
            .iter_mut()
            .find(|(_, d)| !d.in_use)
            .map(|(id, d)| {
                d.in_use = true;
                id.clone()
            });

        match device_id {
            Some(id) => {
                debug!(device = %id, "device leased");
                Ok(Lease {
                    device_id: id,
                    farm: self.state.clone(),
                    _permit: permit,
                })
            }
            None => {
                warn!("no available device despite semaphore permit");
                Err(VelocityError::Config(
                    "No available devices in pool".to_string(),
                ))
            }
        }
    }

    /// Number of devices currently available (not leased).
    pub async fn available_count(&self) -> usize {
        let state = self.state.lock().await;
        state.devices.values().filter(|d| !d.in_use).count()
    }

    /// Total number of booted devices in the pool.
    pub async fn total_count(&self) -> usize {
        let state = self.state.lock().await;
        state.devices.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use velocity_common::Platform;

    struct MockDriver {
        devices: Vec<DeviceInfo>,
    }

    #[async_trait::async_trait]
    impl PlatformDriver for MockDriver {
        async fn list_devices(&self) -> Result<Vec<DeviceInfo>> {
            Ok(self.devices.clone())
        }
        async fn boot_device(&self, _: &str) -> Result<()> { Ok(()) }
        async fn shutdown_device(&self, _: &str) -> Result<()> { Ok(()) }
        async fn install_app(&self, _: &str, _: &str) -> Result<()> { Ok(()) }
        async fn launch_app(&self, _: &str, _: &str, _: bool) -> Result<()> { Ok(()) }
        async fn stop_app(&self, _: &str, _: &str) -> Result<()> { Ok(()) }
        async fn find_element(&self, _: &str, _: &velocity_common::Selector) -> Result<velocity_common::Element> {
            Err(VelocityError::ElementNotFound {
                selector: "mock".to_string(),
                timeout_ms: 0,
                screenshot: None,
                hierarchy_snapshot: None,
            })
        }
        async fn find_elements(&self, _: &str, _: &velocity_common::Selector) -> Result<Vec<velocity_common::Element>> {
            Ok(vec![])
        }
        async fn get_hierarchy(&self, _: &str) -> Result<velocity_common::Element> {
            Err(VelocityError::ElementNotFound {
                selector: "mock".to_string(),
                timeout_ms: 0,
                screenshot: None,
                hierarchy_snapshot: None,
            })
        }
        async fn tap(&self, _: &str, _: &velocity_common::Element) -> Result<()> { Ok(()) }
        async fn double_tap(&self, _: &str, _: &velocity_common::Element) -> Result<()> { Ok(()) }
        async fn long_press(&self, _: &str, _: &velocity_common::Element, _: u64) -> Result<()> { Ok(()) }
        async fn input_text(&self, _: &str, _: &velocity_common::Element, _: &str) -> Result<()> { Ok(()) }
        async fn clear_text(&self, _: &str, _: &velocity_common::Element) -> Result<()> { Ok(()) }
        async fn swipe(&self, _: &str, _: velocity_common::Direction) -> Result<()> { Ok(()) }
        async fn swipe_coords(&self, _: &str, _: (i32, i32), _: (i32, i32)) -> Result<()> { Ok(()) }
        async fn press_key(&self, _: &str, _: velocity_common::Key) -> Result<()> { Ok(()) }
        async fn screenshot(&self, _: &str) -> Result<Vec<u8>> { Ok(vec![]) }
        async fn screen_size(&self, _: &str) -> Result<(i32, i32)> { Ok((1080, 2400)) }
        async fn get_element_text(&self, _: &str, _: &velocity_common::Element) -> Result<String> { Ok(String::new()) }
        async fn is_element_visible(&self, _: &str, _: &velocity_common::Element) -> Result<bool> { Ok(false) }
    }

    fn make_devices(count: usize) -> Vec<DeviceInfo> {
        (0..count)
            .map(|i| DeviceInfo {
                id: format!("device-{i}"),
                name: format!("Device {i}"),
                platform: Platform::Android,
                state: DeviceState::Booted,
                os_version: None,
            })
            .collect()
    }

    #[tokio::test]
    async fn test_refresh_populates_pool() {
        let driver = Arc::new(MockDriver {
            devices: make_devices(3),
        });
        let farm = DeviceFarm::new(driver, 10);
        let count = farm.refresh().await.unwrap();
        assert_eq!(count, 3);
        assert_eq!(farm.total_count().await, 3);
        assert_eq!(farm.available_count().await, 3);
    }

    #[tokio::test]
    async fn test_acquire_and_drop_returns_device() {
        let driver = Arc::new(MockDriver {
            devices: make_devices(2),
        });
        let farm = DeviceFarm::new(driver, 2);
        farm.refresh().await.unwrap();

        let lease = farm.acquire().await.unwrap();
        assert_eq!(farm.available_count().await, 1);

        drop(lease);
        // Give the spawned drop task a moment to run
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        assert_eq!(farm.available_count().await, 2);
    }

    #[tokio::test]
    async fn test_max_devices_cap() {
        let driver = Arc::new(MockDriver {
            devices: make_devices(10),
        });
        let farm = DeviceFarm::new(driver, 3);
        farm.refresh().await.unwrap();
        assert_eq!(farm.total_count().await, 3);
    }
}
