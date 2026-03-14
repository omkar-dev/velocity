use std::process::Stdio;

use tokio::process::Command;
use tracing::debug;
use velocity_common::{DeviceInfo, DeviceState, Platform, Result, VelocityError};

/// Wrapper around `xcrun simctl` subprocess calls for iOS simulator management.
pub struct Simctl {
    xcrun_path: String,
}

impl Simctl {
    pub fn new() -> Self {
        Self {
            xcrun_path: std::env::var("VELOCITY_XCRUN_PATH")
                .unwrap_or_else(|_| "xcrun".to_string()),
        }
    }

    async fn run(&self, args: &[&str]) -> Result<Vec<u8>> {
        let mut cmd = Command::new(&self.xcrun_path);
        cmd.arg("simctl");
        cmd.args(args);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        debug!(args = ?args, "simctl command");

        let output = cmd.output().await.map_err(|e| {
            VelocityError::Config(format!("Failed to execute xcrun simctl: {e}"))
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(VelocityError::Config(format!(
                "simctl command failed: {stderr}"
            )));
        }

        Ok(output.stdout)
    }

    async fn run_text(&self, args: &[&str]) -> Result<String> {
        let bytes = self.run(args).await?;
        Ok(String::from_utf8_lossy(&bytes).to_string())
    }

    /// List all available simulator devices by parsing `xcrun simctl list devices --json`.
    pub async fn list_devices(&self) -> Result<Vec<DeviceInfo>> {
        let output = self.run_text(&["list", "devices", "--json"]).await?;
        let parsed: serde_json::Value = serde_json::from_str(&output).map_err(|e| {
            VelocityError::Config(format!("Failed to parse simctl JSON: {e}"))
        })?;

        let mut devices = Vec::new();
        let devices_obj = parsed
            .get("devices")
            .and_then(|d| d.as_object())
            .ok_or_else(|| {
                VelocityError::Config("Missing 'devices' key in simctl output".to_string())
            })?;

        for (runtime, device_list) in devices_obj {
            let os_version = parse_runtime_version(runtime);
            let entries = match device_list.as_array() {
                Some(arr) => arr,
                None => continue,
            };

            for entry in entries {
                let udid = entry
                    .get("udid")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();
                let name = entry
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unknown")
                    .to_string();
                let state_str = entry
                    .get("state")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Unknown");
                let is_available = entry
                    .get("isAvailable")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                if !is_available {
                    continue;
                }

                let state = match state_str {
                    "Booted" => DeviceState::Booted,
                    "Shutdown" => DeviceState::Shutdown,
                    _ => DeviceState::Unknown,
                };

                devices.push(DeviceInfo {
                    id: udid,
                    name,
                    platform: Platform::Ios,
                    state,
                    os_version: os_version.clone(),
                    device_type: velocity_common::DeviceType::Simulator,
                });
            }
        }

        Ok(devices)
    }

    /// Boot a simulator device.
    pub async fn boot(&self, device_id: &str) -> Result<()> {
        debug!(device = device_id, "booting simulator");
        self.run_text(&["boot", device_id]).await.map_err(|e| {
            VelocityError::DeviceBootFailed {
                id: device_id.to_string(),
                reason: format!("{e}"),
            }
        })?;
        Ok(())
    }

    /// Shut down a simulator device.
    pub async fn shutdown(&self, device_id: &str) -> Result<()> {
        debug!(device = device_id, "shutting down simulator");
        self.run_text(&["shutdown", device_id]).await?;
        Ok(())
    }

    /// Install an app bundle onto a simulator.
    pub async fn install(&self, device_id: &str, app_path: &str) -> Result<()> {
        debug!(device = device_id, app = app_path, "installing app");
        self.run_text(&["install", device_id, app_path]).await?;
        Ok(())
    }

    /// Launch an app by bundle identifier.
    pub async fn launch(&self, device_id: &str, bundle_id: &str) -> Result<()> {
        debug!(device = device_id, bundle = bundle_id, "launching app");
        self.run_text(&["launch", device_id, bundle_id]).await?;
        Ok(())
    }

    /// Terminate a running app by bundle identifier.
    pub async fn terminate(&self, device_id: &str, bundle_id: &str) -> Result<()> {
        debug!(device = device_id, bundle = bundle_id, "terminating app");
        self.run_text(&["terminate", device_id, bundle_id]).await?;
        Ok(())
    }

    /// Take a screenshot from the simulator, returning raw PNG bytes.
    /// Uses `xcrun simctl io screenshot -` to pipe output to stdout.
    pub async fn screenshot(&self, device_id: &str) -> Result<Vec<u8>> {
        debug!(device = device_id, "taking screenshot via simctl");
        self.run(&["io", device_id, "screenshot", "--type=png", "-"])
            .await
    }
}

/// Extract an iOS version string from a simctl runtime identifier.
/// e.g. "com.apple.CoreSimulator.SimRuntime.iOS-17-4" -> Some("17.4")
fn parse_runtime_version(runtime: &str) -> Option<String> {
    let suffix = runtime.rsplit('.').next()?;
    if !suffix.starts_with("iOS-") {
        return None;
    }
    let version = suffix.strip_prefix("iOS-")?;
    Some(version.replace('-', "."))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_runtime_version() {
        assert_eq!(
            parse_runtime_version("com.apple.CoreSimulator.SimRuntime.iOS-17-4"),
            Some("17.4".to_string())
        );
        assert_eq!(
            parse_runtime_version("com.apple.CoreSimulator.SimRuntime.iOS-16-2"),
            Some("16.2".to_string())
        );
        assert_eq!(
            parse_runtime_version("com.apple.CoreSimulator.SimRuntime.tvOS-17-0"),
            None
        );
        assert_eq!(parse_runtime_version("garbage"), None);
    }

    #[test]
    fn test_parse_simctl_json() {
        let json = r#"{
            "devices": {
                "com.apple.CoreSimulator.SimRuntime.iOS-17-4": [
                    {
                        "udid": "AAAA-BBBB-CCCC",
                        "name": "iPhone 15 Pro",
                        "state": "Booted",
                        "isAvailable": true
                    },
                    {
                        "udid": "DDDD-EEEE-FFFF",
                        "name": "iPhone SE",
                        "state": "Shutdown",
                        "isAvailable": true
                    },
                    {
                        "udid": "XXXX-YYYY-ZZZZ",
                        "name": "Unavailable Device",
                        "state": "Shutdown",
                        "isAvailable": false
                    }
                ]
            }
        }"#;

        let parsed: serde_json::Value = serde_json::from_str(json).unwrap();
        let devices_obj = parsed["devices"].as_object().unwrap();

        let mut devices = Vec::new();
        for (runtime, device_list) in devices_obj {
            let os_version = parse_runtime_version(runtime);
            for entry in device_list.as_array().unwrap() {
                let is_available = entry["isAvailable"].as_bool().unwrap_or(false);
                if !is_available {
                    continue;
                }
                devices.push(DeviceInfo {
                    id: entry["udid"].as_str().unwrap().to_string(),
                    name: entry["name"].as_str().unwrap().to_string(),
                    platform: Platform::Ios,
                    state: match entry["state"].as_str().unwrap() {
                        "Booted" => DeviceState::Booted,
                        "Shutdown" => DeviceState::Shutdown,
                        _ => DeviceState::Unknown,
                    },
                    os_version: os_version.clone(),
                    device_type: velocity_common::DeviceType::Simulator,
                });
            }
        }

        assert_eq!(devices.len(), 2);
        assert_eq!(devices[0].name, "iPhone 15 Pro");
        assert_eq!(devices[0].state, DeviceState::Booted);
        assert_eq!(devices[0].os_version.as_deref(), Some("17.4"));
        assert_eq!(devices[1].name, "iPhone SE");
        assert_eq!(devices[1].state, DeviceState::Shutdown);
    }
}
