use std::sync::Arc;

use clap::Args;
use colored::Colorize;
use velocity_common::Platform;
use velocity_inspector::InspectorServer;

use super::create_driver;

#[derive(Args)]
pub struct InspectArgs {
    /// Target platform (ios or android)
    #[arg(short, long, default_value = "ios")]
    platform: String,

    /// Port to serve the inspector UI
    #[arg(long, default_value = "9876")]
    port: u16,

    /// Target device ID (auto-detects if omitted)
    #[arg(short, long)]
    device: Option<String>,

    /// App identifier/package name for performance profiling in the inspector
    #[arg(long)]
    app_id: Option<String>,
}

pub async fn execute(args: InspectArgs) -> anyhow::Result<i32> {
    let platform = match args.platform.as_str() {
        "ios" => Platform::Ios,
        "android" => Platform::Android,
        other => anyhow::bail!("Unknown platform '{other}'. Use 'ios' or 'android'."),
    };

    let driver = create_driver(platform);

    let device_id = match args.device {
        Some(id) => Some(id),
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
                    Some(d.id.clone())
                }
                None => {
                    eprintln!(
                        "{} No booted device found — start without a selected device",
                        "=>".yellow().bold(),
                    );
                    None
                }
            }
        }
    };

    // Bootstrap platform services (e.g. WDA on iOS)
    if let Some(ref id) = device_id {
        eprintln!(
            "{} Preparing device (bootstrapping platform services)...",
            "=>".cyan().bold(),
        );
        driver.prepare(id).await?;
    }

    eprintln!(
        "{} Inspector running at {}",
        "=>".cyan().bold(),
        format!("http://localhost:{}", args.port).underline()
    );

    let driver_arc: Arc<dyn velocity_common::PlatformDriver> = Arc::from(driver);
    let server = InspectorServer::new(driver_arc, device_id, args.app_id);
    server.start(args.port).await?;

    Ok(0)
}
