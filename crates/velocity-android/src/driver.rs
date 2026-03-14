use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::process::Command;
use tokio::sync::Mutex;
use tracing::{debug, info};
use velocity_common::*;

use crate::adb::Adb;
use crate::adb_backend::AdbBackend;
use crate::async_adb::AsyncAdb;
use crate::parser::parse_hierarchy_v2;
use crate::selector::{find_all_elements, find_element, MatchOptions};

/// Android platform driver supporting both subprocess and async TCP ADB backends.
///
/// Set `VELOCITY_ADB_MODE=async` to use the native async TCP client (faster).
/// Default is `subprocess` for backwards compatibility.
pub struct AndroidDriver {
    adb: Box<dyn AdbBackend>,
    hierarchy_cache: Arc<Mutex<Option<(Element, Instant)>>>,
    cache_ttl: Duration,
}

impl Default for AndroidDriver {
    fn default() -> Self {
        Self::new()
    }
}

impl AndroidDriver {
    pub fn new() -> Self {
        let mode = std::env::var("VELOCITY_ADB_MODE").unwrap_or_else(|_| "subprocess".to_string());
        let adb: Box<dyn AdbBackend> = match mode.as_str() {
            "async" | "tcp" => {
                info!("Using async TCP ADB client");
                Box::new(AsyncAdb::new())
            }
            _ => Box::new(Adb::new()),
        };

        Self {
            adb,
            hierarchy_cache: Arc::new(Mutex::new(None)),
            cache_ttl: Duration::from_millis(500),
        }
    }

    async fn get_cached_hierarchy(&self, device_id: &str) -> Result<Element> {
        let mut cache = self.hierarchy_cache.lock().await;
        if let Some((ref tree, ref timestamp)) = *cache {
            if timestamp.elapsed() < self.cache_ttl {
                return Ok(tree.clone());
            }
        }

        let xml = self.adb.dump_hierarchy(device_id).await?;
        let tree = parse_hierarchy_v2(&xml)?;
        *cache = Some((tree.clone(), Instant::now()));
        Ok(tree)
    }

    fn invalidate_cache(&self) {
        let cache = self.hierarchy_cache.clone();
        tokio::spawn(async move {
            let mut cache = cache.lock().await;
            *cache = None;
        });
    }

    async fn screen_bounds(&self, device_id: &str) -> Result<Rect> {
        let (w, h) = self.adb.screen_size(device_id).await?;
        Ok(Rect {
            x: 0,
            y: 0,
            width: w,
            height: h,
        })
    }

    fn key_to_keycode(key: Key) -> u32 {
        match key {
            Key::Back => 4,
            Key::Home => 3,
            Key::Enter => 66,
            Key::VolumeUp => 24,
            Key::VolumeDown => 25,
        }
    }

    async fn swipe_direction_impl(&self, device_id: &str, direction: Direction) -> Result<()> {
        let (w, h) = self.adb.screen_size(device_id).await?;
        let cx = w / 2;
        let cy = h / 2;
        let distance = h / 4;

        let (x1, y1, x2, y2) = match direction {
            Direction::Up => (cx, cy + distance, cx, cy - distance),
            Direction::Down => (cx, cy - distance, cx, cy + distance),
            Direction::Left => (cx + distance, cy, cx - distance, cy),
            Direction::Right => (cx - distance, cy, cx + distance, cy),
        };

        self.adb.swipe(device_id, x1, y1, x2, y2, 300).await
    }
}

#[async_trait::async_trait]
impl PlatformDriver for AndroidDriver {
    async fn list_devices(&self) -> Result<Vec<DeviceInfo>> {
        self.adb.list_devices().await
    }

    async fn boot_device(&self, device_id: &str) -> Result<()> {
        let emulator_path =
            std::env::var("VELOCITY_EMULATOR_PATH").unwrap_or_else(|_| "emulator".to_string());

        debug!(avd = device_id, "Launching Android emulator");

        // Start the emulator in the background. device_id is the AVD name.
        Command::new(&emulator_path)
            .arg("-avd")
            .arg(device_id)
            .arg("-no-snapshot-load")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| {
                VelocityError::Config(format!(
                    "Failed to launch emulator for AVD '{device_id}'. \
                 Ensure the Android emulator is on PATH or set VELOCITY_EMULATOR_PATH. Error: {e}"
                ))
            })?;

        // Poll adb until the device is booted (up to 60 seconds).
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            if Instant::now() > deadline {
                return Err(VelocityError::Config(format!(
                    "Timed out waiting for AVD '{device_id}' to boot"
                )));
            }
            tokio::time::sleep(Duration::from_secs(2)).await;

            let devices = self.adb.list_devices().await.unwrap_or_default();
            let booted = devices.iter().any(|d| d.state == DeviceState::Booted);
            if booted {
                // Wait for sys.boot_completed
                if let Ok(output) = self.adb.list_devices().await {
                    if let Some(d) = output.iter().find(|d| d.state == DeviceState::Booted) {
                        let _ = self
                            .adb
                            .run_device(
                                &d.id,
                                &["wait-for-device", "shell", "getprop", "sys.boot_completed"],
                            )
                            .await;
                        return Ok(());
                    }
                }
                return Ok(());
            }
        }
    }

    async fn shutdown_device(&self, device_id: &str) -> Result<()> {
        let _ = self.adb.run_device(device_id, &["emu", "kill"]).await;
        Ok(())
    }

    async fn install_app(&self, device_id: &str, app_path: &str) -> Result<()> {
        self.adb.install_app(device_id, app_path).await
    }

    async fn launch_app(&self, device_id: &str, app_id: &str, clear_state: bool) -> Result<()> {
        self.invalidate_cache();
        self.adb.launch_app(device_id, app_id, clear_state).await
    }

    async fn stop_app(&self, device_id: &str, app_id: &str) -> Result<()> {
        self.invalidate_cache();
        self.adb.stop_app(device_id, app_id).await
    }

    async fn find_element(&self, device_id: &str, selector: &Selector) -> Result<Element> {
        let tree = self.get_cached_hierarchy(device_id).await?;
        let screen = self.screen_bounds(device_id).await?;
        let opts = MatchOptions::default();

        find_element(&tree, selector, &opts, &screen)
            .cloned()
            .ok_or_else(|| VelocityError::ElementNotFound {
                selector: selector.to_string(),
                timeout_ms: 0,
                screenshot: None,
                hierarchy_snapshot: None,
            })
    }

    async fn find_elements(&self, device_id: &str, selector: &Selector) -> Result<Vec<Element>> {
        let tree = self.get_cached_hierarchy(device_id).await?;
        let screen = self.screen_bounds(device_id).await?;
        let opts = MatchOptions::default();

        Ok(find_all_elements(&tree, selector, &opts, &screen)
            .into_iter()
            .cloned()
            .collect())
    }

    async fn get_hierarchy(&self, device_id: &str) -> Result<Element> {
        // Always fetch fresh for sync engine
        let xml = self.adb.dump_hierarchy(device_id).await?;
        parse_hierarchy_v2(&xml)
    }

    async fn tap(&self, device_id: &str, element: &Element) -> Result<()> {
        let (cx, cy) = element.bounds.center();
        self.invalidate_cache();
        self.adb.tap(device_id, cx, cy).await
    }

    async fn double_tap(&self, device_id: &str, element: &Element) -> Result<()> {
        let (cx, cy) = element.bounds.center();
        self.invalidate_cache();
        self.adb.double_tap(device_id, cx, cy).await
    }

    async fn long_press(&self, device_id: &str, element: &Element, duration_ms: u64) -> Result<()> {
        let (cx, cy) = element.bounds.center();
        self.invalidate_cache();
        self.adb.long_press(device_id, cx, cy, duration_ms).await
    }

    async fn input_text(&self, device_id: &str, _element: &Element, text: &str) -> Result<()> {
        self.invalidate_cache();
        self.adb.input_text(device_id, text).await
    }

    async fn clear_text(&self, device_id: &str, element: &Element) -> Result<()> {
        self.tap(device_id, element).await?;
        self.adb.press_key(device_id, 279).await?; // KEYCODE_MOVE_HOME
        self.adb.press_key(device_id, 67).await?; // KEYCODE_DEL (backspace)
        self.invalidate_cache();
        Ok(())
    }

    async fn swipe(&self, device_id: &str, direction: Direction) -> Result<()> {
        self.invalidate_cache();
        self.swipe_direction_impl(device_id, direction).await
    }

    async fn swipe_coords(&self, device_id: &str, from: (i32, i32), to: (i32, i32)) -> Result<()> {
        self.invalidate_cache();
        self.adb
            .swipe(device_id, from.0, from.1, to.0, to.1, 300)
            .await
    }

    async fn press_key(&self, device_id: &str, key: Key) -> Result<()> {
        self.invalidate_cache();
        self.adb
            .press_key(device_id, Self::key_to_keycode(key))
            .await
    }

    async fn screenshot(&self, device_id: &str) -> Result<Vec<u8>> {
        self.adb.screenshot(device_id).await
    }

    async fn screen_size(&self, device_id: &str) -> Result<(i32, i32)> {
        self.adb.screen_size(device_id).await
    }

    async fn get_element_text(&self, device_id: &str, element: &Element) -> Result<String> {
        let tree = self.get_cached_hierarchy(device_id).await?;
        let screen = self.screen_bounds(device_id).await?;
        let opts = MatchOptions::default();
        let selector = Selector::Id(element.platform_id.clone());

        find_element(&tree, &selector, &opts, &screen)
            .and_then(|e| e.text.clone())
            .ok_or_else(|| VelocityError::ElementNotFound {
                selector: format!("id={:?}", element.platform_id),
                timeout_ms: 0,
                screenshot: None,
                hierarchy_snapshot: None,
            })
    }

    async fn is_element_visible(&self, device_id: &str, element: &Element) -> Result<bool> {
        let tree = self.get_cached_hierarchy(device_id).await?;
        let screen = self.screen_bounds(device_id).await?;
        let opts = MatchOptions {
            visible_only: false,
        };
        let selector = Selector::Id(element.platform_id.clone());

        if let Some(found) = find_element(&tree, &selector, &opts, &screen) {
            Ok(found.visible && !found.bounds.is_empty() && found.bounds.intersects(&screen))
        } else {
            Ok(false)
        }
    }
}
