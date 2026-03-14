use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tracing::debug;
use velocity_common::{DeviceInfo, DeviceState, DeviceType, Platform, Result, VelocityError};

/// Native async ADB client that speaks the ADB wire protocol directly over TCP.
/// Connects to the ADB server on localhost:5037 instead of spawning subprocess.
pub struct AsyncAdb {
    host: String,
    port: u16,
}

impl AsyncAdb {
    pub fn new() -> Self {
        let host = std::env::var("VELOCITY_ADB_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
        let port = std::env::var("VELOCITY_ADB_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(5037);

        Self { host, port }
    }

    /// Open a fresh connection to the ADB server.
    async fn connect(&self) -> Result<TcpStream> {
        let addr = format!("{}:{}", self.host, self.port);
        TcpStream::connect(&addr).await.map_err(|e| {
            VelocityError::Config(format!(
                "Failed to connect to ADB server at {addr}: {e}. Is adb running?"
            ))
        })
    }

    /// Send an ADB protocol message: 4-char hex length prefix + payload.
    async fn send_message(stream: &mut TcpStream, payload: &str) -> Result<()> {
        let msg = format!("{:04x}{}", payload.len(), payload);
        stream
            .write_all(msg.as_bytes())
            .await
            .map_err(|e| VelocityError::Config(format!("ADB write failed: {e}")))
    }

    /// Read the 4-byte status response ("OKAY" or "FAIL").
    async fn read_status(stream: &mut TcpStream) -> Result<bool> {
        let mut buf = [0u8; 4];
        stream
            .read_exact(&mut buf)
            .await
            .map_err(|e| VelocityError::Config(format!("ADB read status failed: {e}")))?;
        Ok(&buf == b"OKAY")
    }

    /// Read a length-prefixed response (4-char hex length + data).
    async fn read_length_prefixed(stream: &mut TcpStream) -> Result<String> {
        let mut len_buf = [0u8; 4];
        stream
            .read_exact(&mut len_buf)
            .await
            .map_err(|e| VelocityError::Config(format!("ADB read length failed: {e}")))?;
        let len_str = std::str::from_utf8(&len_buf)
            .map_err(|e| VelocityError::Config(format!("ADB invalid length prefix: {e}")))?;
        let len = usize::from_str_radix(len_str, 16).map_err(|e| {
            VelocityError::Config(format!("ADB invalid hex length '{len_str}': {e}"))
        })?;

        if len == 0 {
            return Ok(String::new());
        }

        let mut data = vec![0u8; len];
        stream
            .read_exact(&mut data)
            .await
            .map_err(|e| VelocityError::Config(format!("ADB read data failed: {e}")))?;
        String::from_utf8(data)
            .map_err(|e| VelocityError::Config(format!("ADB invalid UTF-8 response: {e}")))
    }

    /// Read all remaining data from the stream until EOF.
    async fn read_until_eof(stream: &mut TcpStream) -> Result<Vec<u8>> {
        let mut buf = Vec::with_capacity(64 * 1024);
        stream
            .read_to_end(&mut buf)
            .await
            .map_err(|e| VelocityError::Config(format!("ADB read to EOF failed: {e}")))?;
        Ok(buf)
    }

    /// Execute an ADB host command (no device transport).
    async fn host_command(&self, command: &str) -> Result<String> {
        let mut stream = self.connect().await?;
        Self::send_message(&mut stream, command).await?;

        if !Self::read_status(&mut stream).await? {
            let error = Self::read_length_prefixed(&mut stream)
                .await
                .unwrap_or_default();
            return Err(VelocityError::Config(format!(
                "ADB host command failed: {error}"
            )));
        }

        Self::read_length_prefixed(&mut stream).await
    }

    /// Switch a connection to a specific device transport, then execute a command.
    async fn device_command(&self, device_id: &str, command: &str) -> Result<String> {
        let mut stream = self.connect().await?;

        // First, switch to device transport
        let transport = format!("host:transport:{device_id}");
        Self::send_message(&mut stream, &transport).await?;

        if !Self::read_status(&mut stream).await? {
            let error = Self::read_length_prefixed(&mut stream)
                .await
                .unwrap_or_default();
            return Err(VelocityError::AdbConnectionLost {
                device_id: device_id.to_string(),
                reason: format!("Transport switch failed: {error}"),
            });
        }

        // Now send the actual command
        Self::send_message(&mut stream, command).await?;

        if !Self::read_status(&mut stream).await? {
            let error = Self::read_length_prefixed(&mut stream)
                .await
                .unwrap_or_default();
            return Err(VelocityError::AdbConnectionLost {
                device_id: device_id.to_string(),
                reason: format!("Device command failed: {error}"),
            });
        }

        // Read all remaining output until connection closes
        let data = Self::read_until_eof(&mut stream).await?;
        String::from_utf8(data).map_err(|e| VelocityError::AdbConnectionLost {
            device_id: device_id.to_string(),
            reason: format!("Invalid UTF-8 in response: {e}"),
        })
    }

    /// Execute a shell command on a device and return stdout as a string.
    async fn shell(&self, device_id: &str, command: &str) -> Result<String> {
        debug!(device = device_id, command, "async adb shell");
        self.device_command(device_id, &format!("shell:{command}"))
            .await
    }

    /// Execute a shell command on a device and return raw bytes (for screenshots).
    async fn shell_raw(&self, device_id: &str, command: &str) -> Result<Vec<u8>> {
        let mut stream = self.connect().await?;

        let transport = format!("host:transport:{device_id}");
        Self::send_message(&mut stream, &transport).await?;

        if !Self::read_status(&mut stream).await? {
            return Err(VelocityError::AdbConnectionLost {
                device_id: device_id.to_string(),
                reason: "Transport switch failed".to_string(),
            });
        }

        Self::send_message(&mut stream, &format!("shell:{command}")).await?;

        if !Self::read_status(&mut stream).await? {
            return Err(VelocityError::AdbConnectionLost {
                device_id: device_id.to_string(),
                reason: "Shell command failed".to_string(),
            });
        }

        Self::read_until_eof(&mut stream).await
    }

    // ── Public API matching Adb interface ──

    pub async fn list_devices(&self) -> Result<Vec<DeviceInfo>> {
        let output = self.host_command("host:devices-long").await?;
        let mut devices = Vec::new();

        for line in output.lines() {
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
        // For install, we still need the subprocess approach since the ADB protocol
        // for push+install is complex. Use the shell pm install approach instead.
        let output = self
            .shell(
                device_id,
                &format!(
                    "pm install -r /data/local/tmp/{}",
                    std::path::Path::new(apk_path)
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("app.apk")
                ),
            )
            .await?;

        if output.contains("Failure") {
            return Err(VelocityError::Config(format!("Install failed: {output}")));
        }
        Ok(())
    }

    pub async fn launch_app(
        &self,
        device_id: &str,
        package: &str,
        clear_state: bool,
    ) -> Result<()> {
        if clear_state {
            self.shell(device_id, &format!("pm clear {package}"))
                .await?;
        }
        self.shell(
            device_id,
            &format!("monkey -p {package} -c android.intent.category.LAUNCHER 1"),
        )
        .await?;
        Ok(())
    }

    pub async fn stop_app(&self, device_id: &str, package: &str) -> Result<()> {
        self.shell(device_id, &format!("am force-stop {package}"))
            .await?;
        Ok(())
    }

    pub async fn dump_hierarchy(&self, device_id: &str) -> Result<String> {
        let output = self.shell(device_id, "uiautomator dump /dev/tty").await?;

        if let Some(xml_start) = output.find("<?xml") {
            Ok(output[xml_start..].to_string())
        } else if let Some(xml_start) = output.find("<hierarchy") {
            Ok(output[xml_start..].to_string())
        } else {
            Ok(output)
        }
    }

    pub async fn tap(&self, device_id: &str, x: i32, y: i32) -> Result<()> {
        self.shell(device_id, &format!("input tap {x} {y}")).await?;
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
        self.shell(
            device_id,
            &format!("input swipe {x} {y} {x} {y} {duration_ms}"),
        )
        .await?;
        Ok(())
    }

    pub async fn input_text(&self, device_id: &str, text: &str) -> Result<()> {
        let escaped = escape_adb_text(text);
        self.shell(device_id, &format!("input text {escaped}"))
            .await?;
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
        self.shell(
            device_id,
            &format!("input swipe {x1} {y1} {x2} {y2} {duration_ms}"),
        )
        .await?;
        Ok(())
    }

    pub async fn press_key(&self, device_id: &str, keycode: u32) -> Result<()> {
        self.shell(device_id, &format!("input keyevent {keycode}"))
            .await?;
        Ok(())
    }

    pub async fn screenshot(&self, device_id: &str) -> Result<Vec<u8>> {
        let bytes = self.shell_raw(device_id, "screencap -p").await?;
        Ok(fix_adb_newlines(&bytes))
    }

    pub async fn screen_size(&self, device_id: &str) -> Result<(i32, i32)> {
        let output = self.shell(device_id, "wm size").await?;

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

    /// Run a device command (used by driver for boot_device wait).
    pub async fn run_device(&self, device_id: &str, args: &[&str]) -> Result<String> {
        // Convert args to a shell command when possible
        if args.first() == Some(&"shell") {
            let cmd = args[1..].join(" ");
            return self.shell(device_id, &cmd).await;
        }
        // For non-shell commands like "wait-for-device", use a compound approach
        let cmd = args.join(" ");
        self.shell(device_id, &cmd).await
    }

    /// Execute multiple shell commands joined with &&.
    pub async fn batch_shell(&self, device_id: &str, commands: &[&str]) -> Result<String> {
        if commands.is_empty() {
            return Ok(String::new());
        }
        let script = commands.join(" && ");
        self.shell(device_id, &script).await
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
