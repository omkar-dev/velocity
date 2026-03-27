use clap::Args;
use colored::Colorize;
use velocity_core::{parse_suite, selector_lint};

#[derive(Args)]
pub struct LintArgs {
    /// Path to the test suite YAML config
    #[arg(short, long, default_value = "velocity.yaml")]
    config: String,
}

pub fn execute(args: LintArgs) -> anyhow::Result<i32> {
    println!(
        "{} Linting selectors in {}",
        "=>".cyan().bold(),
        args.config.bold()
    );

    let suite = match parse_suite(&args.config) {
        Ok(s) => s,
        Err(e) => {
            println!();
            println!("  {} Parse error: {e}", "\u{2717}".red().bold());
            return Ok(2);
        }
    };

    let diagnostics = selector_lint::lint_suite(&suite);

    if diagnostics.is_empty() {
        println!("{} No selector issues found", "\u{2713}".green().bold());
        return Ok(0);
    }

    let warnings = diagnostics
        .iter()
        .filter(|d| d.severity == selector_lint::LintSeverity::Warning)
        .count();
    let errors = diagnostics
        .iter()
        .filter(|d| d.severity == selector_lint::LintSeverity::Error)
        .count();

    for d in &diagnostics {
        let severity_str = match d.severity {
            selector_lint::LintSeverity::Warning => "warning".yellow().bold(),
            selector_lint::LintSeverity::Error => "error".red().bold(),
        };
        println!(
            "  {} [{}] {}:{} \u{2014} {}",
            severity_str, d.rule, d.test_name, d.step_index, d.message
        );
        println!("    selector: {}", d.selector.dimmed());
    }

    println!();
    println!(
        "{} {} warnings, {} errors in {} selectors",
        "=>".cyan().bold(),
        warnings,
        errors,
        diagnostics.len()
    );

    if errors > 0 {
        Ok(1)
    } else {
        Ok(0)
    }
}
