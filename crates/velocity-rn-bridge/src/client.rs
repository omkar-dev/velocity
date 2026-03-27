use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::time::{timeout, Duration};

use crate::config::RnBridgeConfig;
use crate::protocol::{BridgeCommand, BridgeResponse};

/// TCP client for communicating with the RN sidecar process.
pub struct RnBridgeClient {
    config: RnBridgeConfig,
    stream: Option<TcpStream>,
}

impl RnBridgeClient {
    pub fn new(config: RnBridgeConfig) -> Self {
        Self {
            config,
            stream: None,
        }
    }

    /// Connect to an already-running sidecar.
    pub async fn connect(&mut self) -> anyhow::Result<()> {
        let addr = format!("127.0.0.1:{}", self.config.port);

        // Try connecting to existing sidecar first
        match timeout(Duration::from_secs(2), TcpStream::connect(&addr)).await {
            Ok(Ok(stream)) => {
                tracing::info!("Connected to existing RN sidecar at {addr}");
                self.stream = Some(stream);
                return Ok(());
            }
            _ => {
                tracing::info!("No existing sidecar found at {addr}");
            }
        }

        // If no sidecar found, return error with instructions
        anyhow::bail!(
            "RN sidecar not running on port {}. \
             Start the sidecar with: velocity rn-sidecar --port {} --platform <ios|android>",
            self.config.port,
            self.config.port
        )
    }

    /// Send a command and receive a response.
    pub async fn send(&mut self, cmd: &BridgeCommand) -> anyhow::Result<BridgeResponse> {
        let stream = self
            .stream
            .as_mut()
            .ok_or_else(|| anyhow::anyhow!("Not connected to RN sidecar"))?;

        let mut payload = serde_json::to_string(cmd)?;
        payload.push('\n');

        let (reader, mut writer) = stream.split();
        writer.write_all(payload.as_bytes()).await?;
        writer.flush().await?;

        let mut buf_reader = BufReader::new(reader);
        let mut line = String::new();

        let read_result = timeout(Duration::from_secs(30), buf_reader.read_line(&mut line))
            .await
            .map_err(|_| anyhow::anyhow!("Sidecar response timed out after 30s"))??;

        if read_result == 0 {
            anyhow::bail!("Sidecar disconnected");
        }

        let response: BridgeResponse = serde_json::from_str(line.trim())?;
        Ok(response)
    }

    /// Disconnect and optionally shut down the sidecar.
    pub async fn disconnect(&mut self) {
        if self.stream.is_some() {
            let _ = self.send(&BridgeCommand::Shutdown).await;
            self.stream = None;
        }
    }

    pub fn is_connected(&self) -> bool {
        self.stream.is_some()
    }
}
