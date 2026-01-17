use clap::Subcommand;
use colored::Colorize;
use velocity_common::{DeviceState, Platform};

use super::create_driver;

#[derive(Subcommand)]
pub enum DeviceCommand {
    /// List available devices
    List {
        /// Filter by platform (ios or android)
        #[arg(short, long)]
        platform: Option<String>,
    },

    /// Boot a device by name or ID
    Boot {
        /// Device name or ID
        name: String,

        /// Platform (ios or android)
        #[arg(short, long, default_value = "ios")]
        platform: String,
    },

    /// Shutdown a device by name or ID
    Shutdown {
        /// Device name or ID
        name: String,

        /// Platform (ios or android)
        #[arg(short, long, default_value = "ios")]
        platform: String,
    },

    /// Take a screenshot of a device
    Screenshot {
        /// Device name or ID
        name: String,

        /// Platform (ios or android)
        #[arg(short, long, default_value = "ios")]
        platform: String,

        /// Output file path
        #[arg(short, long, default_value = "screenshot.png")]
        output: String,
    },
}

fn parse_platform(s: &str) -> anyhow::Result<Platform> {
    match s {
        "ios" => Ok(Platform::Ios),
        "android" => Ok(Platform::Android),
        other => anyhow::bail!("Unknown platform '{other}'. Use 'ios' or 'android'."),
    }
}

pub async fn execute(command: DeviceCommand) -> anyhow::Result<i32> {
    match command {
        DeviceCommand::List { platform } => {
            let platforms = match platform.as_deref() {
                Some(p) => vec![parse_platform(p)?],
                None => vec![Platform::Ios, Platform::Android],
            };

            println!("{} Scanning for devices...", "=>".cyan().bold());
            println!();
            println!(
                "  {:<36}  {:<10}  {:<10}  {}",
                "ID".bold(),
                "NAME".bold(),
                "PLATFORM".bold(),
                "STATE".bold()
            );
            println!("  {}", "─".repeat(72).dimmed());

            for p in platforms {
                let driver = create_driver(p);
                match driver.list_devices().await {
                    Ok(devices) => {
                        for d in &devices {
                            let state_colored = match d.state {
                                DeviceState::Booted => d.state.to_string().green(),
                                DeviceState::Shutdown => d.state.to_string().dimmed(),
                                DeviceState::Unknown => d.state.to_string().yellow(),
                            };
                            println!(
                                "  {:<36}  {:<10}  {:<10}  {}",
                                d.id, d.name, d.platform, state_colored
                            );
                        }
                    }
                    Err(_) => {
                        // Silently skip platforms that aren't available
                    }
                }
            }

            println!();
            Ok(0)
        }

        DeviceCommand::Boot { name, platform } => {
            let p = parse_platform(&platform)?;
            println!("{} Booting device {}...", "=>".cyan().bold(), name.bold());
            let driver = create_driver(p);
            driver.boot_device(&name).await?;
            println!("{} Device {} is now booted", "=>".green().bold(), name.bold());
            Ok(0)
        }

        DeviceCommand::Shutdown { name, platform } => {
            let p = parse_platform(&platform)?;
            println!(
                "{} Shutting down device {}...",
                "=>".cyan().bold(),
                name.bold()
            );
            let driver = create_driver(p);
            driver.shutdown_device(&name).await?;
            println!(
                "{} Device {} is now shut down",
                "=>".green().bold(),
                name.bold()
            );
            Ok(0)
        }

        DeviceCommand::Screenshot {
            name,
            platform,
            output,
        } => {
            let p = parse_platform(&platform)?;
            println!(
                "{} Taking screenshot of {}...",
                "=>".cyan().bold(),
                name.bold()
            );
            let driver = create_driver(p);
            let png_data = driver.screenshot(&name).await?;
            std::fs::write(&output, &png_data)?;
            println!(
                "{} Screenshot saved to {} ({} bytes)",
                "=>".green().bold(),
                output.underline(),
                png_data.len()
            );
            Ok(0)
        }
    }
}
