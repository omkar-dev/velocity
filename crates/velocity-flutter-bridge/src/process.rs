use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::process::Child;
use tokio::time::{timeout, Duration};

use crate::config::FlutterBridgeConfig;
use crate::protocol::{FlutterCommand, FlutterResponse};

/// Manages the Flutter test process and TCP communication.
pub struct FlutterProcess {
    config: FlutterBridgeConfig,
    child: Option<Child>,
    stream: Option<TcpStream>,
}

impl FlutterProcess {
    pub fn new(config: FlutterBridgeConfig) -> Self {
        Self {
            config,
            child: None,
            stream: None,
        }
    }

    /// Start the Flutter test process and connect via TCP.
    pub async fn start(&mut self) -> anyhow::Result<()> {
        let addr = format!("127.0.0.1:{}", self.config.port);

        // Try connecting to existing process first
        match timeout(Duration::from_secs(2), TcpStream::connect(&addr)).await {
            Ok(Ok(stream)) => {
                tracing::info!("Connected to existing Flutter test process at {addr}");
                self.stream = Some(stream);
                return Ok(());
            }
            _ => {
                tracing::info!("No existing Flutter process, will need to start one");
            }
        }

        anyhow::bail!(
            "Flutter test process not running on port {}. \
             Start it with: velocity flutter-bridge --port {} --project {}",
            self.config.port,
            self.config.port,
            self.config.project_path
        )
    }

    /// Send a command and receive a response.
    pub async fn send(&mut self, cmd: &FlutterCommand) -> anyhow::Result<FlutterResponse> {
        let stream = self.stream.as_mut().ok_or_else(|| {
            anyhow::anyhow!("Not connected to Flutter test process")
        })?;

        let mut payload = serde_json::to_string(cmd)?;
        payload.push('\n');

        let (reader, mut writer) = stream.split();
        writer.write_all(payload.as_bytes()).await?;
        writer.flush().await?;

        let mut buf_reader = BufReader::new(reader);
        let mut line = String::new();

        let read_result = timeout(Duration::from_secs(30), buf_reader.read_line(&mut line)).await
            .map_err(|_| anyhow::anyhow!("Flutter process response timed out after 30s"))??;

        if read_result == 0 {
            anyhow::bail!("Flutter process disconnected");
        }

        let response: FlutterResponse = serde_json::from_str(line.trim())?;
        Ok(response)
    }

    /// Stop the Flutter test process.
    pub async fn stop(&mut self) {
        if self.stream.is_some() {
            let _ = self.send(&FlutterCommand::Shutdown).await;
            self.stream = None;
        }
        if let Some(mut child) = self.child.take() {
            let _ = child.kill().await;
        }
    }

    pub fn is_running(&self) -> bool {
        self.stream.is_some()
    }
}
