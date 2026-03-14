use std::process::Stdio;

use tokio::process::Command;
use tracing::debug;
use velocity_common::{DeviceInfo, DeviceState, DeviceType, Platform, Result, VelocityError};

/// Wrapper around ADB subprocess calls with optional command batching.
pub struct Adb {
    adb_path: String,
}

impl Adb {
    pub fn new() -> Self {
        Self {
            adb_path: std::env::var("VELOCITY_ADB_PATH").unwrap_or_else(|_| "adb".to_string()),
        }
    }

    /// Execute multiple shell commands in a single ADB invocation by
    /// joining them with `&&`. Returns combined stdout. Use for independent
    /// commands where ordering doesn't matter (e.g. pre-flight checks).
    pub async fn batch_shell(
        &self,
        device_id: &str,
        commands: &[&str],
    ) -> Result<String> {
        if commands.is_empty() {
            return Ok(String::new());
        }
        if commands.len() == 1 {
            return self.run(device_id, &["shell", commands[0]]).await;
        }
        let script = commands.join(" && ");
        self.run(device_id, &["shell", &script]).await
    }

    async fn run(&self, device_id: &str, args: &[&str]) -> Result<String> {
        let mut cmd = Command::new(&self.adb_path);
        cmd.arg("-s").arg(device_id);
        cmd.args(args);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        debug!(device = device_id, args = ?args, "adb command");

        let output = cmd.output().await.map_err(|e| {
            VelocityError::AdbConnectionLost {
                device_id: device_id.to_string(),
                reason: format!("Failed to execute adb: {e}"),
            }
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(VelocityError::AdbConnectionLost {
                device_id: device_id.to_string(),
                reason: format!("adb command failed: {stderr}"),
            });
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    /// Public version of run for use by the driver.
    pub async fn run_device(&self, device_id: &str, args: &[&str]) -> Result<String> {
        self.run(device_id, args).await
    }

    async fn run_global(&self, args: &[&str]) -> Result<String> {
        let mut cmd = Command::new(&self.adb_path);
        cmd.args(args);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let output = cmd.output().await.map_err(|e| {
            VelocityError::Config(format!("Failed to execute adb: {e}"))
        })?;

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    pub async fn list_devices(&self) -> Result<Vec<DeviceInfo>> {
        let output = self.run_global(&["devices", "-l"]).await?;
        let mut devices = Vec::new();

        for line in output.lines().skip(1) {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 2 {
                continue;
            }

            let id = parts[0].to_string();
            let state = match parts[1] {
                "device" => DeviceState::Booted,
                "offline" => DeviceState::Shutdown,
                _ => DeviceState::Unknown,
            };

            let mut name = id.clone();
            let mut os_version = None;
            let mut device_type = DeviceType::Emulator;
            for part in &parts[2..] {
                if let Some(model) = part.strip_prefix("model:") {
                    name = model.to_string();
                }
                if let Some(ver) = part.strip_prefix("transport_id:") {
                    os_version = Some(ver.to_string());
                }
                if part.starts_with("usb:") {
                    device_type = DeviceType::Physical;
                }
            }

            devices.push(DeviceInfo {
                id,
                name,
                platform: Platform::Android,
                state,
                os_version,
                device_type,
            });
        }

        Ok(devices)
    }

    pub async fn install_app(&self, device_id: &str, apk_path: &str) -> Result<()> {
        self.run(device_id, &["install", "-r", apk_path]).await?;
        Ok(())
    }

    pub async fn launch_app(
        &self,
        device_id: &str,
        package: &str,
        clear_state: bool,
    ) -> Result<()> {
        if clear_state {
            self.run(device_id, &["shell", "pm", "clear", package]).await?;
        }
        // Launch the main activity via monkey (auto-detects launcher activity)
        self.run(
            device_id,
            &[
                "shell",
                "monkey",
                "-p",
                package,
                "-c",
                "android.intent.category.LAUNCHER",
                "1",
            ],
        )
        .await?;
        Ok(())
    }

    pub async fn stop_app(&self, device_id: &str, package: &str) -> Result<()> {
        self.run(device_id, &["shell", "am", "force-stop", package]).await?;
        Ok(())
    }

    pub async fn dump_hierarchy(&self, device_id: &str) -> Result<String> {
        let output = self
            .run(device_id, &["shell", "uiautomator", "dump", "/dev/tty"])
            .await?;

        // The output may have a trailing message like "UI hierarchy dumped to: /dev/tty"
        // We need just the XML part
        if let Some(xml_start) = output.find("<?xml") {
            Ok(output[xml_start..].to_string())
        } else if let Some(xml_start) = output.find("<hierarchy") {
            Ok(output[xml_start..].to_string())
        } else {
            Ok(output)
        }
    }

    pub async fn tap(&self, device_id: &str, x: i32, y: i32) -> Result<()> {
        self.run(
            device_id,
            &["shell", "input", "tap", &x.to_string(), &y.to_string()],
        )
        .await?;
        Ok(())
    }

    pub async fn double_tap(&self, device_id: &str, x: i32, y: i32) -> Result<()> {
        self.tap(device_id, x, y).await?;
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        self.tap(device_id, x, y).await?;
        Ok(())
    }

    pub async fn long_press(
        &self,
        device_id: &str,
        x: i32,
        y: i32,
        duration_ms: u64,
    ) -> Result<()> {
        self.run(
            device_id,
            &[
                "shell",
                "input",
                "swipe",
                &x.to_string(),
                &y.to_string(),
                &x.to_string(),
                &y.to_string(),
                &duration_ms.to_string(),
            ],
        )
        .await?;
        Ok(())
    }

    pub async fn input_text(&self, device_id: &str, text: &str) -> Result<()> {
        // Escape special characters for adb shell input text
        let escaped = escape_adb_text(text);
        self.run(device_id, &["shell", "input", "text", &escaped]).await?;
        Ok(())
    }

    pub async fn swipe(
        &self,
        device_id: &str,
        x1: i32,
        y1: i32,
        x2: i32,
        y2: i32,
        duration_ms: u64,
    ) -> Result<()> {
        self.run(
            device_id,
            &[
                "shell",
                "input",
                "swipe",
                &x1.to_string(),
                &y1.to_string(),
                &x2.to_string(),
                &y2.to_string(),
                &duration_ms.to_string(),
            ],
        )
        .await?;
        Ok(())
    }

    pub async fn press_key(&self, device_id: &str, keycode: u32) -> Result<()> {
        self.run(
            device_id,
            &["shell", "input", "keyevent", &keycode.to_string()],
        )
        .await?;
        Ok(())
    }

    pub async fn screenshot(&self, device_id: &str) -> Result<Vec<u8>> {
        let mut cmd = Command::new(&self.adb_path);
        cmd.arg("-s").arg(device_id);
        cmd.args(["shell", "screencap", "-p"]);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let output = cmd.output().await.map_err(|e| {
            VelocityError::AdbConnectionLost {
                device_id: device_id.to_string(),
                reason: format!("Failed to take screenshot: {e}"),
            }
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(VelocityError::AdbConnectionLost {
                device_id: device_id.to_string(),
                reason: format!("Screenshot failed: {stderr}"),
            });
        }

        // ADB on Windows may convert \n to \r\n; fix the PNG output
        let bytes = fix_adb_newlines(&output.stdout);
        Ok(bytes)
    }

    pub async fn screen_size(&self, device_id: &str) -> Result<(i32, i32)> {
        let output = self
            .run(device_id, &["shell", "wm", "size"])
            .await?;

        // Output: "Physical size: 1080x2400"
        for line in output.lines() {
            if let Some(size_str) = line.strip_prefix("Physical size:") {
                let size_str = size_str.trim();
                if let Some((w, h)) = size_str.split_once('x') {
                    let width = w.trim().parse::<i32>().unwrap_or(1080);
                    let height = h.trim().parse::<i32>().unwrap_or(2400);
                    return Ok((width, height));
                }
            }
        }

        Ok((1080, 2400))
    }
}

fn escape_adb_text(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len() * 2);
    for c in text.chars() {
        match c {
            ' ' => escaped.push_str("%s"),
            '(' | ')' | '<' | '>' | '|' | ';' | '&' | '*' | '\\' | '~' | '"' | '\'' => {
                escaped.push('\\');
                escaped.push(c);
            }
            _ => escaped.push(c),
        }
    }
    escaped
}

fn fix_adb_newlines(bytes: &[u8]) -> Vec<u8> {
    let mut fixed = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if i + 1 < bytes.len() && bytes[i] == b'\r' && bytes[i + 1] == b'\n' {
            fixed.push(b'\n');
            i += 2;
        } else {
            fixed.push(bytes[i]);
            i += 1;
        }
    }
    fixed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_escape_adb_text() {
        assert_eq!(escape_adb_text("hello world"), "hello%sworld");
        assert_eq!(escape_adb_text("test(1)"), "test\\(1\\)");
        assert_eq!(escape_adb_text("simple"), "simple");
    }

    #[test]
    fn test_fix_adb_newlines() {
        let input = b"hello\r\nworld\r\n";
        let fixed = fix_adb_newlines(input);
        assert_eq!(fixed, b"hello\nworld\n");
    }
}
