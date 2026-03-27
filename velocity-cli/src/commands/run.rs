use std::sync::Arc;

use clap::Args;
use colored::Colorize;
use velocity_common::{DriverMode, Framework, Platform, PlatformDriver, ReportFormat, ResilientDriver, RuntimeConfig};
use velocity_core::{parse_suite, resolve_flows, validate_suite};
use velocity_runner::SuiteRunner;

fn parse_driver_mode(value: &str) -> anyhow::Result<DriverMode> {
    match value {
        "device" => Ok(DriverMode::Device),
        "headless" => Ok(DriverMode::Headless),
        other => anyhow::bail!("Unknown driver mode '{other}'. Use 'device' or 'headless'."),
    }
}

fn parse_framework(value: &str) -> anyhow::Result<Framework> {
    match value {
        "native" => Ok(Framework::Native),
        "react_native" | "reactnative" | "rn" => Ok(Framework::ReactNative),
        "flutter" => Ok(Framework::Flutter),
        other => anyhow::bail!("Unknown framework '{other}'. Use 'native', 'react_native', or 'flutter'."),
    }
}

/// Auto-detect app framework from the app path.
fn detect_framework(app_path: Option<&str>, platform: Platform) -> Framework {
    let Some(path) = app_path else {
        return Framework::Native;
    };

    let path_obj = std::path::Path::new(path);

    // Flutter detection: pubspec.yaml in project or libflutter.so in APK
    if path_obj.join("pubspec.yaml").exists() {
        return Framework::Flutter;
    }

    // React Native detection
    if platform == Platform::Android {
        // Check for libreactnativejni.so or index.android.bundle
        if path_obj.join("index.android.bundle").exists() {
            return Framework::ReactNative;
        }
    } else if platform == Platform::Ios {
        // Check for hermes.framework or jsc.framework inside .app
        let hermes = path_obj.join("Frameworks").join("hermes.framework");
        let jsc = path_obj.join("Frameworks").join("JavaScriptCore.framework");
        if hermes.exists() || jsc.exists() {
            return Framework::ReactNative;
        }
    }

    // Check for package.json with react-native dependency (project root)
    if path_obj.join("package.json").exists() {
        if let Ok(content) = std::fs::read_to_string(path_obj.join("package.json")) {
            if content.contains("react-native") {
                return Framework::ReactNative;
            }
        }
    }

    Framework::Native
}

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

    /// Driver mode: device or headless
    #[arg(long)]
    driver: Option<String>,

    /// App framework: native, react_native, or flutter. Auto-detected if omitted.
    #[arg(long)]
    framework: Option<String>,

    /// Update snapshot baselines instead of comparing
    #[arg(long)]
    update_baselines: bool,
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

    let update_baselines = args.update_baselines
        || std::env::var("VELOCITY_UPDATE_BASELINES").map_or(false, |v| v == "1" || v == "true");

    let cli_driver_mode = args
        .driver
        .as_deref()
        .map(parse_driver_mode)
        .transpose()?;

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
        driver_mode: cli_driver_mode.unwrap_or_default(),
        framework: Framework::default(),
        update_baselines,
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

    // Determine effective driver mode (CLI > YAML config > default)
    let effective_driver_mode = match cli_driver_mode {
        Some(mode) => mode,
        None => suite
            .config
            .driver
            .as_deref()
            .map(parse_driver_mode)
            .transpose()?
            .unwrap_or(DriverMode::Device),
    };

    // Determine framework (CLI > YAML > auto-detect)
    let cli_framework = args
        .framework
        .as_deref()
        .map(parse_framework)
        .transpose()?;

    let effective_framework = match cli_framework {
        Some(fw) => fw,
        None => suite
            .config
            .framework
            .as_deref()
            .map(parse_framework)
            .transpose()?
            .unwrap_or_else(|| {
                // Auto-detect from app path
                let app_path = suite.config.headless.as_ref()
                    .and_then(|h| h.app_path.as_deref());
                detect_framework(app_path, effective_platform)
            }),
    };

    // Update runtime config with resolved framework
    let runtime_config = RuntimeConfig {
        framework: effective_framework,
        ..runtime_config
    };

    if effective_framework != Framework::Native {
        println!(
            "{} Framework: {}",
            "=>".cyan().bold(),
            effective_framework.to_string().yellow()
        );
    }

    if runtime_config.update_baselines {
        println!(
            "{} {} Baseline update mode — snapshots will be overwritten",
            "=>".cyan().bold(),
            "WARNING:".yellow().bold()
        );
    }

    // Create platform driver wrapped in resilience layer for automatic
    // retries on transient failures and circuit breaking on sustained failures.
    let raw_driver: Arc<dyn velocity_common::PlatformDriver> = match effective_driver_mode {
        DriverMode::Headless => {
            match effective_framework {
                Framework::ReactNative => {
                    let mut rn_config = velocity_rn_bridge::RnBridgeConfig::default();
                    // Apply YAML headless dimensions
                    if let Some(hc) = &suite.config.headless {
                        if let Some(w) = hc.width { rn_config.width = w; }
                        if let Some(h) = hc.height { rn_config.height = h; }
                    }
                    // Apply RN-specific config
                    if let Some(rn) = &suite.config.react_native {
                        if let Some(ref bp) = rn.bundle_path { rn_config.bundle_path = bp.clone(); }
                        if let Some(ref c) = rn.component { rn_config.component = c.clone(); }
                        if let Some(p) = rn.port { rn_config.port = p; }
                        rn_config.native_mocks = rn.native_mocks.clone();
                    }
                    println!(
                        "{} Driver: {} ({}x{}, port {})",
                        "=>".cyan().bold(),
                        "react-native headless".yellow(),
                        rn_config.width,
                        rn_config.height,
                        rn_config.port,
                    );
                    Arc::new(velocity_rn_bridge::RnDriver::new(effective_platform, rn_config))
                }
                Framework::Flutter => {
                    let mut flutter_config = velocity_flutter_bridge::FlutterBridgeConfig::default();
                    // Apply YAML headless dimensions
                    if let Some(hc) = &suite.config.headless {
                        if let Some(w) = hc.width { flutter_config.width = w; }
                        if let Some(h) = hc.height { flutter_config.height = h; }
                    }
                    // Apply Flutter-specific config
                    if let Some(fc) = &suite.config.flutter {
                        if let Some(ref pp) = fc.project_path { flutter_config.project_path = pp.clone(); }
                        if let Some(ref t) = fc.target { flutter_config.target = t.clone(); }
                        if let Some(ref gd) = fc.golden_dir { flutter_config.golden_dir = Some(gd.clone()); }
                        if let Some(p) = fc.port { flutter_config.port = p; }
                    }
                    println!(
                        "{} Driver: {} ({}x{}, port {})",
                        "=>".cyan().bold(),
                        "flutter headless".yellow(),
                        flutter_config.width,
                        flutter_config.height,
                        flutter_config.port,
                    );
                    Arc::new(velocity_flutter_bridge::FlutterDriver::new(effective_platform, flutter_config))
                }
                Framework::Native => {
                    let mut headless_config = velocity_headless::HeadlessConfig::default();
                    // Apply YAML headless config if present
                    if let Some(hc) = &suite.config.headless {
                        if let Some(w) = hc.width { headless_config.width = w; }
                        if let Some(h) = hc.height { headless_config.height = h; }
                        if let Some(d) = hc.density { headless_config.density = d; }
                        if let Some(ref dir) = hc.baseline_dir { headless_config.baseline_dir = dir.clone(); }
                        if let Some(ref p) = hc.app_path { headless_config.app_path = Some(p.clone()); }
                        if let Some(ref l) = hc.initial_layout { headless_config.initial_layout = Some(l.clone()); }
                    }
                    println!(
                        "{} Driver: {} ({}x{})",
                        "=>".cyan().bold(),
                        "headless".yellow(),
                        headless_config.width,
                        headless_config.height
                    );
                    Arc::new(velocity_headless::HeadlessDriver::new(effective_platform, headless_config))
                }
            }
        }
        DriverMode::Device => match effective_platform {
            Platform::Ios => Arc::new(velocity_ios::IosDriver::new()),
            Platform::Android => Arc::new(velocity_android::AndroidDriver::new()),
        },
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
