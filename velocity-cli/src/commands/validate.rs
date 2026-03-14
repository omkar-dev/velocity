use clap::Args;
use colored::Colorize;
use velocity_core::{parse_suite, resolve_flows, validate_suite};

#[derive(Args)]
pub struct ValidateArgs {
    /// Path to the YAML config file to validate
    config: String,
}

pub fn execute(args: ValidateArgs) -> anyhow::Result<i32> {
    println!("{} Validating {}", "=>".cyan().bold(), args.config.bold());

    let suite = match parse_suite(&args.config) {
        Ok(s) => s,
        Err(e) => {
            println!();
            println!("  {} Parse error: {e}", "✗".red().bold());
            return Ok(2);
        }
    };

    if let Err(e) = validate_suite(&suite) {
        println!();
        println!("  {} Validation error: {e}", "✗".red().bold());
        return Ok(2);
    }

    if let Err(e) = resolve_flows(&suite) {
        println!();
        println!("  {} Flow resolution error: {e}", "✗".red().bold());
        return Ok(2);
    }

    println!();
    println!("  {} Configuration is valid", "✓".green().bold());
    println!();
    println!("  App ID:    {}", suite.app_id.cyan());
    println!(
        "  Platform:  {}",
        suite
            .config
            .platform
            .map_or("auto-detect".to_string(), |p| p.to_string())
            .cyan()
    );
    println!("  Flows:     {}", suite.flows.len());
    println!("  Tests:     {}", suite.tests.len());

    let total_steps: usize = suite.tests.iter().map(|t| t.steps.len()).sum();
    println!("  Steps:     {total_steps}");

    let all_tags: Vec<&str> = suite
        .tests
        .iter()
        .flat_map(|t| t.tags.iter().map(|s| s.as_str()))
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    if !all_tags.is_empty() {
        println!("  Tags:      {}", all_tags.join(", ").dimmed());
    }

    println!();
    Ok(0)
}
