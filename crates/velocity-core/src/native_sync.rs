use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tracing::{debug, warn};
use velocity_common::{Result, VelocityError};

/// Request sent to the native sync probe.
#[derive(Debug, Serialize)]
struct ProbeRequest {
    cmd: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    timeout_ms: Option<u64>,
}

/// Response from the native sync probe.
#[derive(Debug, Deserialize)]
pub struct ProbeResponse {
    pub idle: bool,
    #[serde(default)]
    pub pending: Vec<String>,
    #[serde(default)]
    pub waited_ms: Option<u64>,
    #[serde(default)]
    pub ts: Option<u64>,
}

/// Client for communicating with a native sync probe (iOS or Android)
/// running inside the app under test.
///
/// Protocol: newline-delimited JSON over TCP.
/// - iOS probe listens on localhost:19400
/// - Android probe listens on localhost:19401 (via adb port-forward)
pub struct NativeSyncClient {
    host: String,
    port: u16,
    connect_timeout: Duration,
    stream: Option<BufReader<TcpStream>>,
}

impl NativeSyncClient {
    pub fn new(port: u16, connect_timeout_ms: u64) -> Self {
        Self {
            host: "127.0.0.1".to_string(),
            port,
            connect_timeout: Duration::from_millis(connect_timeout_ms),
            stream: None,
        }
    }

    /// Try to connect to the probe. Returns true if successful.
    pub async fn connect(&mut self) -> bool {
        let addr = format!("{}:{}", self.host, self.port);
        debug!(addr = %addr, "connecting to native sync probe");

        match tokio::time::timeout(
            self.connect_timeout,
            TcpStream::connect(&addr),
        )
        .await
        {
            Ok(Ok(stream)) => {
                debug!("native sync probe connected");
                self.stream = Some(BufReader::new(stream));
                true
            }
            Ok(Err(e)) => {
                debug!(error = %e, "native sync probe connection failed");
                self.stream = None;
                false
            }
            Err(_) => {
                debug!("native sync probe connection timed out");
                self.stream = None;
                false
            }
        }
    }

    /// Check if currently connected.
    pub fn is_connected(&self) -> bool {
        self.stream.is_some()
    }

    /// Disconnect from the probe.
    pub fn disconnect(&mut self) {
        self.stream = None;
    }

    /// Send a status query to the probe.
    pub async fn status(&mut self) -> Result<ProbeResponse> {
        self.send_command(ProbeRequest {
            cmd: "status",
            timeout_ms: None,
        })
        .await
    }

    /// Wait for the app to become idle, with a timeout.
    pub async fn wait_for_idle(&mut self, timeout_ms: u64) -> Result<ProbeResponse> {
        self.send_command(ProbeRequest {
            cmd: "wait_idle",
            timeout_ms: Some(timeout_ms),
        })
        .await
    }

    /// Send a command and read the response.
    async fn send_command(&mut self, request: ProbeRequest) -> Result<ProbeResponse> {
        let stream = self.stream.as_mut().ok_or_else(|| {
            VelocityError::Config("Native sync probe not connected".to_string())
        })?;

        // Serialize request as JSON + newline
        let mut json = serde_json::to_string(&request).map_err(|e| {
            VelocityError::Config(format!("Failed to serialize probe request: {e}"))
        })?;
        json.push('\n');

        // Write request
        stream.get_mut().write_all(json.as_bytes()).await.map_err(|e| {
            VelocityError::Config(format!("Failed to write to probe: {e}"))
        })?;

        // Read response (one line)
        let mut line = String::new();
        stream.read_line(&mut line).await.map_err(|e| {
            VelocityError::Config(format!("Failed to read from probe: {e}"))
        })?;

        if line.is_empty() {
            self.stream = None;
            return Err(VelocityError::Config(
                "Native sync probe disconnected".to_string(),
            ));
        }

        serde_json::from_str(&line).map_err(|e| {
            VelocityError::Config(format!("Failed to parse probe response: {e}"))
        })
    }
}

/// Unified sync manager that supports native probe, polling, or auto-detection.
pub struct SyncManager {
    mode: velocity_common::SyncMode,
    native_client: Option<NativeSyncClient>,
    polling_engine: crate::sync::AdaptiveSyncEngine,
    native_available: Option<bool>,
}

impl SyncManager {
    pub fn new(config: &velocity_common::SyncConfig, platform: velocity_common::Platform) -> Self {
        let native_port = match platform {
            velocity_common::Platform::Ios => config.native_port_ios,
            velocity_common::Platform::Android => config.native_port_android,
        };

        let native_client = match config.mode {
            velocity_common::SyncMode::Polling => None,
            _ => Some(NativeSyncClient::new(native_port, config.probe_connect_timeout_ms)),
        };

        Self {
            mode: config.mode,
            native_client,
            polling_engine: crate::sync::AdaptiveSyncEngine::new(config.clone()),
            native_available: None,
        }
    }

    /// Wait for the UI to stabilize using the configured sync strategy.
    pub async fn wait_for_idle(
        &mut self,
        driver: &dyn velocity_common::PlatformDriver,
        device_id: &str,
    ) -> Result<()> {
        match self.mode {
            velocity_common::SyncMode::Native => {
                self.native_wait_for_idle().await
            }
            velocity_common::SyncMode::Polling => {
                self.polling_engine.wait_for_idle(driver, device_id).await
            }
            velocity_common::SyncMode::Auto => {
                // Try native first, fallback to polling
                if self.try_native().await {
                    match self.native_wait_for_idle().await {
                        Ok(()) => return Ok(()),
                        Err(e) => {
                            warn!(error = %e, "native sync failed, falling back to polling");
                            self.native_available = Some(false);
                        }
                    }
                }
                self.polling_engine.wait_for_idle(driver, device_id).await
            }
        }
    }

    /// Wait for idle with a selector-specific key for prediction.
    pub async fn wait_for_idle_keyed(
        &mut self,
        driver: &dyn velocity_common::PlatformDriver,
        device_id: &str,
        key: &str,
    ) -> Result<()> {
        match self.mode {
            velocity_common::SyncMode::Native => {
                self.native_wait_for_idle().await
            }
            velocity_common::SyncMode::Polling => {
                self.polling_engine
                    .wait_for_idle_keyed(driver, device_id, key)
                    .await
            }
            velocity_common::SyncMode::Auto => {
                if self.try_native().await {
                    match self.native_wait_for_idle().await {
                        Ok(()) => return Ok(()),
                        Err(e) => {
                            warn!(error = %e, "native sync failed, falling back to polling");
                            self.native_available = Some(false);
                        }
                    }
                }
                self.polling_engine
                    .wait_for_idle_keyed(driver, device_id, key)
                    .await
            }
        }
    }

    /// Reset state after mutating actions.
    pub fn invalidate(&mut self) {
        self.polling_engine.invalidate_tree_diff();
    }

    /// Try to establish native sync connection (cached result).
    async fn try_native(&mut self) -> bool {
        if let Some(available) = self.native_available {
            return available;
        }

        if let Some(ref mut client) = self.native_client {
            let connected = client.connect().await;
            self.native_available = Some(connected);
            connected
        } else {
            self.native_available = Some(false);
            false
        }
    }

    /// Use native probe to wait for idle.
    async fn native_wait_for_idle(&mut self) -> Result<()> {
        let client = self.native_client.as_mut().ok_or_else(|| {
            VelocityError::Config("Native sync client not available".to_string())
        })?;

        if !client.is_connected() {
            if !client.connect().await {
                return Err(VelocityError::Config(
                    "Cannot connect to native sync probe".to_string(),
                ));
            }
        }

        let timeout_ms = self.polling_engine.config().timeout_ms;
        let response = client.wait_for_idle(timeout_ms).await?;

        if response.idle {
            debug!(
                waited_ms = response.waited_ms.unwrap_or(0),
                "native sync: app is idle"
            );
            Ok(())
        } else {
            Err(VelocityError::SyncTimeout {
                timeout_ms,
                stable_count: 0,
                required: 1,
            })
        }
    }
}
