use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};
use tracing::{debug, info, warn};
use velocity_common::{Result, VelocityError};

const WDA_REPO: &str = "https://github.com/appium/WebDriverAgent.git";
const WDA_DIR_NAME: &str = "WebDriverAgent";
const WDA_PORT: u16 = 8100;
const WDA_READY_TIMEOUT: Duration = Duration::from_secs(120);
const WDA_POLL_INTERVAL: Duration = Duration::from_millis(500);

/// Manages the full WDA lifecycle: download, build, launch, health-check, teardown.
pub struct WdaBootstrap {
    cache_dir: PathBuf,
    wda_process: Option<Child>,
    port: u16,
}

impl Default for WdaBootstrap {
    fn default() -> Self {
        Self::new()
    }
}

impl WdaBootstrap {
    pub fn new() -> Self {
        let cache_dir = dirs_home().join(".velocity").join("wda");
        Self {
            cache_dir,
            wda_process: None,
            port: WDA_PORT,
        }
    }

    pub fn with_port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }

    /// Check if WDA is already running and responsive on the expected port.
    pub async fn is_running(&self) -> bool {
        let url = format!("http://localhost:{}/status", self.port);
        match reqwest::Client::new()
            .get(&url)
            .timeout(Duration::from_secs(3))
            .send()
            .await
        {
            Ok(resp) => resp.status().is_success(),
            Err(_) => false,
        }
    }

    /// Ensure WDA is running. If already running, returns immediately.
    /// Otherwise downloads (if needed), builds (if needed), and launches WDA.
    pub async fn ensure_running(&mut self, device_id: &str) -> Result<()> {
        if self.is_running().await {
            info!("WDA already running on port {}", self.port);
            return Ok(());
        }

        info!("WDA not running — bootstrapping...");

        // Step 1: Ensure WDA source exists
        let wda_project = self.ensure_source().await?;

        // Step 2: Build WDA for the target simulator
        self.build(&wda_project, device_id).await?;

        // Step 3: Launch WDA with log tailing for readiness detection
        self.launch_and_wait_ready(&wda_project, device_id).await?;

        info!("WDA is ready on port {}", self.port);
        Ok(())
    }

    /// Download WDA source from GitHub if not already cached.
    async fn ensure_source(&self) -> Result<PathBuf> {
        let wda_dir = self.cache_dir.join(WDA_DIR_NAME);
        let project_file = wda_dir.join("WebDriverAgent.xcodeproj");

        if project_file.exists() {
            debug!("WDA source already cached at {}", wda_dir.display());
            // Pull latest changes
            let pull_result = Command::new("git")
                .args(["pull", "--ff-only"])
                .current_dir(&wda_dir)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .await;
            if let Ok(status) = pull_result {
                if !status.success() {
                    debug!("git pull failed (non-fatal), using cached version");
                }
            }
            return Ok(wda_dir);
        }

        info!("Downloading WebDriverAgent source...");
        std::fs::create_dir_all(&self.cache_dir).map_err(|e| {
            VelocityError::Config(format!(
                "Failed to create WDA cache directory {}: {e}",
                self.cache_dir.display()
            ))
        })?;

        let output = Command::new("git")
            .args([
                "clone",
                "--depth",
                "1",
                WDA_REPO,
                wda_dir.to_str().unwrap_or("WebDriverAgent"),
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| VelocityError::Config(format!("Failed to run git clone: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(VelocityError::Config(format!(
                "Failed to clone WDA repository: {stderr}"
            )));
        }

        info!("WDA source downloaded to {}", wda_dir.display());
        Ok(wda_dir)
    }

    /// Build WDA for the given simulator device.
    async fn build(&self, wda_dir: &PathBuf, device_id: &str) -> Result<()> {
        info!("Building WebDriverAgent for device {device_id}...");

        let destination = format!("platform=iOS Simulator,id={device_id}");

        let output = Command::new("xcodebuild")
            .args([
                "build-for-testing",
                "-project",
                "WebDriverAgent.xcodeproj",
                "-scheme",
                "WebDriverAgentRunner",
                "-destination",
                &destination,
                "-allowProvisioningUpdates",
                "-quiet",
            ])
            .current_dir(wda_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await
            .map_err(|e| VelocityError::Config(format!("Failed to run xcodebuild: {e}")))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            let combined = format!("{stdout}\n{stderr}");
            if combined.contains("BUILD FAILED") || combined.contains("** FAILED **") {
                return Err(VelocityError::Config(format!(
                    "WDA build failed:\n{stderr}"
                )));
            }
            if !combined.contains("BUILD SUCCEEDED") && !combined.contains("TEST BUILD SUCCEEDED") {
                return Err(VelocityError::Config(format!(
                    "WDA build failed (exit code {}):\n{stderr}",
                    output.status
                )));
            }
        }

        info!("WDA build succeeded");
        Ok(())
    }

    /// Launch WDA and wait for readiness using a race between log tailing
    /// and HTTP health polling. Whichever confirms readiness first wins.
    async fn launch_and_wait_ready(&mut self, wda_dir: &PathBuf, device_id: &str) -> Result<()> {
        info!("Launching WebDriverAgent on device {device_id}...");

        let destination = format!("platform=iOS Simulator,id={device_id}");

        let mut child = Command::new("xcodebuild")
            .args([
                "test-without-building",
                "-project",
                "WebDriverAgent.xcodeproj",
                "-scheme",
                "WebDriverAgentRunner",
                "-destination",
                &destination,
            ])
            .env("USE_PORT", self.port.to_string())
            .current_dir(wda_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| VelocityError::Config(format!("Failed to launch WDA: {e}")))?;

        // Take stdout for log tailing
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();

        self.wda_process = Some(child);

        // Race: log tailing vs HTTP polling vs timeout
        let log_future = Self::tail_logs_for_ready(stdout, stderr);
        let http_future = self.poll_health_endpoint();
        let timeout_future = tokio::time::sleep(WDA_READY_TIMEOUT);

        tokio::select! {
            result = log_future => {
                match result {
                    Ok(()) => {
                        debug!("WDA readiness confirmed via log output");
                        Ok(())
                    }
                    Err(e) => {
                        warn!("WDA log tailing ended without readiness signal: {e}");
                        // Fall through to check if HTTP is ready
                        if self.is_running().await {
                            Ok(())
                        } else {
                            Err(VelocityError::Config(
                                "WDA process ended without becoming ready".to_string(),
                            ))
                        }
                    }
                }
            }
            result = http_future => {
                debug!("WDA readiness confirmed via HTTP health check");
                result
            }
            _ = timeout_future => {
                Err(VelocityError::Config(format!(
                    "WDA failed to start within {}s on port {}",
                    WDA_READY_TIMEOUT.as_secs(),
                    self.port
                )))
            }
        }
    }

    /// Tail WDA stdout/stderr looking for readiness markers.
    async fn tail_logs_for_ready(
        stdout: Option<tokio::process::ChildStdout>,
        stderr: Option<tokio::process::ChildStderr>,
    ) -> Result<()> {
        // Prefer stderr since xcodebuild writes test output there
        if let Some(stderr) = stderr {
            let reader = BufReader::new(stderr);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                debug!(target: "wda", "{line}");
                if line.contains("ServerURLHere")
                    || line.contains("WebDriverAgent started")
                    || line.contains("ServerReady")
                {
                    return Ok(());
                }
            }
        }

        // Fallback to stdout
        if let Some(stdout) = stdout {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                debug!(target: "wda", "{line}");
                if line.contains("ServerURLHere")
                    || line.contains("WebDriverAgent started")
                    || line.contains("ServerReady")
                {
                    return Ok(());
                }
            }
        }

        Err(VelocityError::Config(
            "WDA log stream ended without readiness signal".to_string(),
        ))
    }

    /// Poll WDA status endpoint with exponential backoff until it responds.
    async fn poll_health_endpoint(&self) -> Result<()> {
        let url = format!("http://localhost:{}/status", self.port);
        let client = reqwest::Client::new();

        debug!("Polling WDA health endpoint on port {}...", self.port);

        let mut interval = WDA_POLL_INTERVAL;
        let max_interval = Duration::from_secs(3);

        loop {
            match client
                .get(&url)
                .timeout(Duration::from_secs(2))
                .send()
                .await
            {
                Ok(resp) if resp.status().is_success() => return Ok(()),
                _ => {
                    tokio::time::sleep(interval).await;
                    // Exponential backoff capped at max_interval
                    interval = (interval * 3 / 2).min(max_interval);
                }
            }
        }
    }

    /// Stop the WDA process if we started it.
    pub async fn stop(&mut self) {
        if let Some(mut child) = self.wda_process.take() {
            debug!("Stopping WDA process");
            let _ = child.kill().await;
            let _ = child.wait().await;
        }
    }

    /// Get the WDA base URL.
    pub fn base_url(&self) -> String {
        format!("http://localhost:{}", self.port)
    }
}

impl Drop for WdaBootstrap {
    fn drop(&mut self) {
        if let Some(ref mut child) = self.wda_process {
            let _ = child.start_kill();
        }
    }
}

fn dirs_home() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("/tmp"))
}
