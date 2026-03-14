use std::sync::Arc;

use clap::Args;
use colored::Colorize;
use velocity_common::{Platform, PlatformDriver, ReportFormat, ResilientDriver, RuntimeConfig};
use velocity_core::{parse_suite, resolve_flows, validate_suite};
use velocity_runner::SuiteRunner;

#[derive(Args)]
pub struct RunArgs {
    /// Path to the test suite YAML config
    #[arg(short, long, default_value = "velocity.yaml")]
    config: String,

    /// Target platform (ios or android). Auto-detected if omitted.
    #[arg(short, long)]
    platform: Option<String>,

    /// Target device ID. Uses first available if omitted.
    #[arg(short, long)]
    device: Option<String>,

    /// Run only tests matching these tags (comma-separated)
    #[arg(long, value_delimiter = ',')]
    tags: Vec<String>,

    /// Run only tests whose name matches this glob pattern (use * for wildcards)
    #[arg(long)]
    filter: Option<String>,

    /// Shard index (0-based) for parallel execution
    #[arg(long)]
    shard_index: Option<usize>,

    /// Total number of shards
    #[arg(long)]
    shard_total: Option<usize>,

    /// Number of retries for failed tests
    #[arg(long)]
    retries: Option<u32>,

    /// Report format: junit or json
    #[arg(long, default_value = "junit")]
    report: String,

    /// Output directory for artifacts
    #[arg(long, default_value = "./velocity-results")]
    artifacts_dir: String,

    /// Suite-level timeout in milliseconds
    #[arg(long)]
    timeout: Option<u64>,

    /// Stop on first failure
    #[arg(long)]
    fail_fast: bool,

    /// Environment variable overrides (KEY=VALUE)
    #[arg(long = "env", value_delimiter = ',')]
    env_overrides: Vec<String>,
}

pub async fn execute(args: RunArgs) -> anyhow::Result<i32> {
    println!(
        "{} Loading suite from {}",
        "=>".cyan().bold(),
        args.config.bold()
    );

    let platform = match args.platform.as_deref() {
        Some("ios") => Some(Platform::Ios),
        Some("android") => Some(Platform::Android),
        Some(other) => anyhow::bail!("Unknown platform '{other}'. Use 'ios' or 'android'."),
        None => None,
    };

    let report_format = match args.report.as_str() {
        "junit" => ReportFormat::Junit,
        "json" => ReportFormat::Json,
        other => anyhow::bail!("Unknown report format '{other}'. Use 'junit' or 'json'."),
    };

    let env_overrides: Vec<(String, String)> = args
        .env_overrides
        .iter()
        .filter_map(|e| {
            let mut parts = e.splitn(2, '=');
            let key = parts.next()?.to_string();
            let val = parts.next()?.to_string();
            Some((key, val))
        })
        .collect();

    let runtime_config = RuntimeConfig {
        config_path: args.config.clone(),
        platform,
        device_id: args.device.clone(),
        tags: args.tags.clone(),
        test_filter: args.filter.clone(),
        shard_index: args.shard_index,
        shard_total: args.shard_total,
        retry_count: args.retries,
        report_format,
        artifacts_dir: args.artifacts_dir.clone(),
        suite_timeout_ms: args.timeout,
        fail_fast: args.fail_fast,
        env_overrides,
    };

    // Parse, validate, and resolve flows
    let mut suite = parse_suite(&args.config)?;
    validate_suite(&suite)?;

    // Interpolate env vars
    let overrides: std::collections::HashMap<String, String> =
        runtime_config.env_overrides.iter().cloned().collect();
    velocity_core::env::interpolate_suite(&mut suite, &overrides)?;

    // Resolve flows (inline runFlow steps)
    let resolved_tests = resolve_flows(&suite)?;
    suite.tests = resolved_tests;

    // Determine platform
    let effective_platform = runtime_config
        .platform
        .or(suite.config.platform)
        .unwrap_or(Platform::Ios);

    println!(
        "{} Platform: {}",
        "=>".cyan().bold(),
        effective_platform.to_string().green()
    );

    // Create platform driver wrapped in resilience layer for automatic
    // retries on transient failures and circuit breaking on sustained failures.
    let raw_driver: Arc<dyn velocity_common::PlatformDriver> = match effective_platform {
        Platform::Ios => Arc::new(velocity_ios::IosDriver::new()),
        Platform::Android => Arc::new(velocity_android::AndroidDriver::new()),
    };
    let driver = ResilientDriver::new(raw_driver);

    // Create artifacts dir
    let _ = std::fs::create_dir_all(&args.artifacts_dir);

    // Determine device for bootstrap
    let boot_device_id = match &args.device {
        Some(id) => id.clone(),
        None => {
            let devices = driver.list_devices().await?;
            devices.first().map(|d| d.id.clone()).unwrap_or_default()
        }
    };

    // Bootstrap platform (e.g. auto-start WDA on iOS)
    if !boot_device_id.is_empty() {
        println!(
            "{} Preparing {} driver...",
            "=>".cyan().bold(),
            effective_platform.to_string().green()
        );
        driver.prepare(&boot_device_id).await?;
    }

    // Run suite
    let result = SuiteRunner::run(suite, runtime_config, &driver).await?;

    // Write report
    match report_format {
        ReportFormat::Junit => {
            let path = format!("{}/junit.xml", args.artifacts_dir);
            velocity_runner::write_junit(&result, &path)?;
            println!("{} Report: {}", "=>".cyan().bold(), path.underline());
        }
        ReportFormat::Json => {
            let path = format!("{}/results.json", args.artifacts_dir);
            velocity_runner::write_json(&result, &path)?;
            println!("{} Report: {}", "=>".cyan().bold(), path.underline());
        }
    }

    // Cleanup platform services
    driver.cleanup().await;

    Ok(result.exit_code())
}
