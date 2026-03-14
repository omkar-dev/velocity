mod commands;

use clap::{Parser, Subcommand};
use colored::Colorize;
use tracing_subscriber::EnvFilter;

use commands::{device, mcp, migrate, run, validate};

#[derive(Parser)]
#[command(
    name = "velocity",
    about = "Velocity — fast, reliable mobile UI testing",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Run a test suite
    Run(run::RunArgs),

    /// Manage devices (simulators/emulators)
    Device {
        #[command(subcommand)]
        command: device::DeviceCommand,
    },

    /// Migrate tests from other frameworks
    Migrate {
        #[command(subcommand)]
        command: migrate::MigrateCommand,
    },

    /// Start MCP server for AI-driven testing
    Mcp(mcp::McpArgs),

    /// Validate a test configuration file
    Validate(validate::ValidateArgs),

    /// Print version information
    Version,
}

fn main() {
    let cli = Cli::parse();

    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn")),
        )
        .with_target(false)
        .init();

    let result = run_command(cli);

    match result {
        Ok(code) => std::process::exit(code),
        Err(e) => {
            eprintln!("{} {e}", "error:".red().bold());
            let code = if let Some(ve) = e.downcast_ref::<velocity_common::VelocityError>() {
                ve.exit_code()
            } else {
                1
            };
            std::process::exit(code);
        }
    }
}

#[tokio::main]
async fn run_command(cli: Cli) -> anyhow::Result<i32> {
    match cli.command {
        Command::Run(args) => run::execute(args).await,
        Command::Device { command } => device::execute(command).await,
        Command::Migrate { command } => migrate::execute(command),
        Command::Mcp(args) => mcp::execute(args).await,
        Command::Validate(args) => validate::execute(args),
        Command::Version => {
            println!("{} {}", "velocity".bold(), env!("CARGO_PKG_VERSION"));
            Ok(0)
        }
    }
}
