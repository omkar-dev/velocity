use std::sync::Arc;

use clap::Args;
use colored::Colorize;
use velocity_common::Platform;
use velocity_mcp::McpServer;

use super::create_driver;

#[derive(Args)]
pub struct McpArgs {
    /// Target device ID
    #[arg(short, long)]
    device: Option<String>,

    /// Target platform (ios or android)
    #[arg(short, long, default_value = "ios")]
    platform: String,

    /// Path to test suite config for flow/test tools
    #[arg(short, long)]
    config: Option<String>,
}

pub async fn execute(args: McpArgs) -> anyhow::Result<i32> {
    let platform = match args.platform.as_str() {
        "ios" => Platform::Ios,
        "android" => Platform::Android,
        other => anyhow::bail!("Unknown platform '{other}'. Use 'ios' or 'android'."),
    };

    eprintln!(
        "{} Starting MCP server (platform={}, transport=stdio)",
        "=>".cyan().bold(),
        platform
    );

    let driver = create_driver(platform);

    let device_id = match args.device {
        Some(id) => id,
        None => {
            let devices = driver.list_devices().await?;
            let booted = devices
                .iter()
                .find(|d| d.state == velocity_common::DeviceState::Booted);
            match booted {
                Some(d) => {
                    eprintln!(
                        "{} Auto-selected device: {} ({})",
                        "=>".cyan().bold(),
                        d.name,
                        d.id
                    );
                    d.id.clone()
                }
                None => {
                    anyhow::bail!(
                        "No booted device found. Boot a device first with: velocity device boot <name>"
                    );
                }
            }
        }
    };

    McpServer::init_logging();
    let driver_arc: Arc<dyn velocity_common::PlatformDriver> = Arc::from(driver);
    let mut server = McpServer::new(driver_arc, device_id, args.config);
    server.run_stdio().await?;
    Ok(0)
}
