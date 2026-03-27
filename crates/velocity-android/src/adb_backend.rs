use velocity_common::{DeviceInfo, Result};

/// Unified interface for ADB backends (subprocess vs async TCP).
/// Both `Adb` (subprocess) and `AsyncAdb` (native TCP) implement this trait.
#[async_trait::async_trait]
pub trait AdbBackend: Send + Sync {
    async fn list_devices(&self) -> Result<Vec<DeviceInfo>>;
    async fn install_app(&self, device_id: &str, apk_path: &str) -> Result<()>;
    async fn launch_app(&self, device_id: &str, package: &str, clear_state: bool) -> Result<()>;
    async fn stop_app(&self, device_id: &str, package: &str) -> Result<()>;
    async fn dump_hierarchy(&self, device_id: &str) -> Result<String>;
    async fn tap(&self, device_id: &str, x: i32, y: i32) -> Result<()>;
    async fn double_tap(&self, device_id: &str, x: i32, y: i32) -> Result<()>;
    async fn long_press(&self, device_id: &str, x: i32, y: i32, duration_ms: u64) -> Result<()>;
    async fn input_text(&self, device_id: &str, text: &str) -> Result<()>;
    async fn swipe(
        &self,
        device_id: &str,
        x1: i32,
        y1: i32,
        x2: i32,
        y2: i32,
        duration_ms: u64,
    ) -> Result<()>;
    async fn press_key(&self, device_id: &str, keycode: u32) -> Result<()>;
    async fn screenshot(&self, device_id: &str) -> Result<Vec<u8>>;
    async fn screen_size(&self, device_id: &str) -> Result<(i32, i32)>;
    async fn run_device(&self, device_id: &str, args: &[&str]) -> Result<String>;
    async fn batch_shell(&self, device_id: &str, commands: &[&str]) -> Result<String>;

    /// Collect resource metrics (heap, PSS, CPU) for a package.
    /// Returns (java_heap_kb, native_heap_kb, total_pss_kb, cpu_percent).
    async fn collect_resource_metrics(
        &self,
        device_id: &str,
        package: &str,
    ) -> Result<(u64, u64, u64, f32)>;
}

// ── Implement AdbBackend for the subprocess Adb ──

#[async_trait::async_trait]
impl AdbBackend for crate::adb::Adb {
    async fn list_devices(&self) -> Result<Vec<DeviceInfo>> {
        self.list_devices().await
    }
    async fn install_app(&self, device_id: &str, apk_path: &str) -> Result<()> {
        self.install_app(device_id, apk_path).await
    }
    async fn launch_app(&self, device_id: &str, package: &str, clear_state: bool) -> Result<()> {
        self.launch_app(device_id, package, clear_state).await
    }
    async fn stop_app(&self, device_id: &str, package: &str) -> Result<()> {
        self.stop_app(device_id, package).await
    }
    async fn dump_hierarchy(&self, device_id: &str) -> Result<String> {
        self.dump_hierarchy(device_id).await
    }
    async fn tap(&self, device_id: &str, x: i32, y: i32) -> Result<()> {
        self.tap(device_id, x, y).await
    }
    async fn double_tap(&self, device_id: &str, x: i32, y: i32) -> Result<()> {
        self.double_tap(device_id, x, y).await
    }
    async fn long_press(&self, device_id: &str, x: i32, y: i32, duration_ms: u64) -> Result<()> {
        self.long_press(device_id, x, y, duration_ms).await
    }
    async fn input_text(&self, device_id: &str, text: &str) -> Result<()> {
        self.input_text(device_id, text).await
    }
    async fn swipe(
        &self,
        device_id: &str,
        x1: i32,
        y1: i32,
        x2: i32,
        y2: i32,
        duration_ms: u64,
    ) -> Result<()> {
        self.swipe(device_id, x1, y1, x2, y2, duration_ms).await
    }
    async fn press_key(&self, device_id: &str, keycode: u32) -> Result<()> {
        self.press_key(device_id, keycode).await
    }
    async fn screenshot(&self, device_id: &str) -> Result<Vec<u8>> {
        self.screenshot(device_id).await
    }
    async fn screen_size(&self, device_id: &str) -> Result<(i32, i32)> {
        self.screen_size(device_id).await
    }
    async fn run_device(&self, device_id: &str, args: &[&str]) -> Result<String> {
        self.run_device(device_id, args).await
    }
    async fn batch_shell(&self, device_id: &str, commands: &[&str]) -> Result<String> {
        self.batch_shell(device_id, commands).await
    }
    async fn collect_resource_metrics(
        &self,
        device_id: &str,
        package: &str,
    ) -> Result<(u64, u64, u64, f32)> {
        self.collect_resource_metrics(device_id, package).await
    }
}

// ── Implement AdbBackend for the async TCP client ──

#[async_trait::async_trait]
impl AdbBackend for crate::async_adb::AsyncAdb {
    async fn list_devices(&self) -> Result<Vec<DeviceInfo>> {
        self.list_devices().await
    }
    async fn install_app(&self, device_id: &str, apk_path: &str) -> Result<()> {
        self.install_app(device_id, apk_path).await
    }
    async fn launch_app(&self, device_id: &str, package: &str, clear_state: bool) -> Result<()> {
        self.launch_app(device_id, package, clear_state).await
    }
    async fn stop_app(&self, device_id: &str, package: &str) -> Result<()> {
        self.stop_app(device_id, package).await
    }
    async fn dump_hierarchy(&self, device_id: &str) -> Result<String> {
        self.dump_hierarchy(device_id).await
    }
    async fn tap(&self, device_id: &str, x: i32, y: i32) -> Result<()> {
        self.tap(device_id, x, y).await
    }
    async fn double_tap(&self, device_id: &str, x: i32, y: i32) -> Result<()> {
        self.double_tap(device_id, x, y).await
    }
    async fn long_press(&self, device_id: &str, x: i32, y: i32, duration_ms: u64) -> Result<()> {
        self.long_press(device_id, x, y, duration_ms).await
    }
    async fn input_text(&self, device_id: &str, text: &str) -> Result<()> {
        self.input_text(device_id, text).await
    }
    async fn swipe(
        &self,
        device_id: &str,
        x1: i32,
        y1: i32,
        x2: i32,
        y2: i32,
        duration_ms: u64,
    ) -> Result<()> {
        self.swipe(device_id, x1, y1, x2, y2, duration_ms).await
    }
    async fn press_key(&self, device_id: &str, keycode: u32) -> Result<()> {
        self.press_key(device_id, keycode).await
    }
    async fn screenshot(&self, device_id: &str) -> Result<Vec<u8>> {
        self.screenshot(device_id).await
    }
    async fn screen_size(&self, device_id: &str) -> Result<(i32, i32)> {
        self.screen_size(device_id).await
    }
    async fn run_device(&self, device_id: &str, args: &[&str]) -> Result<String> {
        self.run_device(device_id, args).await
    }
    async fn batch_shell(&self, device_id: &str, commands: &[&str]) -> Result<String> {
        self.batch_shell(device_id, commands).await
    }
    async fn collect_resource_metrics(
        &self,
        device_id: &str,
        package: &str,
    ) -> Result<(u64, u64, u64, f32)> {
        self.collect_resource_metrics(device_id, package).await
    }
}
