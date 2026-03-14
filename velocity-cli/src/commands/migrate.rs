use clap::Subcommand;
use colored::Colorize;
use velocity_migrate::{MaestroMigrator, Severity};

#[derive(Subcommand)]
pub enum MigrateCommand {
    /// Migrate Maestro YAML tests to Velocity format
    Maestro {
        /// Input directory containing Maestro YAML files
        dir: String,

        /// Output directory for Velocity YAML files
        #[arg(short, long, default_value = "./velocity-migrated")]
        output: String,

        /// Output migration report as JSON
        #[arg(long)]
        report_json: bool,
    },
}

pub fn execute(command: MigrateCommand) -> anyhow::Result<i32> {
    match command {
        MigrateCommand::Maestro {
            dir,
            output,
            report_json,
        } => {
            println!(
                "{} Migrating Maestro tests from {}",
                "=>".cyan().bold(),
                dir.bold()
            );

            let migrator = MaestroMigrator::new();
            let report = migrator.migrate_directory(&dir, &output)?;

            if report_json {
                let json = velocity_migrate::generate_report_json(&report);
                println!("{json}");
                return Ok(if report.files_failed > 0 { 1 } else { 0 });
            }

            println!();
            println!("{}", "━".repeat(60).dimmed());
            println!(
                "  {} {} total, {} migrated, {} failed",
                "Migration Results:".bold(),
                report.files_total,
                report.files_migrated.to_string().green().bold(),
                report.files_failed.to_string().red().bold()
            );
            println!("{}", "━".repeat(60).dimmed());

            for result in &report.results {
                let icon = if result.success {
                    "✓".green()
                } else {
                    "✗".red()
                };
                println!(
                    "  {icon} {} ({} steps migrated, {} skipped)",
                    result.source_file, result.steps_migrated, result.steps_skipped
                );

                for issue in &result.issues {
                    let severity_str = match issue.severity {
                        Severity::Info => "info".dimmed(),
                        Severity::Warning => "warn".yellow(),
                        Severity::Error => "error".red(),
                    };
                    println!(
                        "    {} [line {}] {}: {}",
                        "↳".dimmed(),
                        issue.line,
                        severity_str,
                        issue.message
                    );
                }
            }

            if report.total_warnings > 0 {
                println!();
                println!(
                    "  {} {} warnings — review migrated files for TODO items",
                    "⚠".yellow(),
                    report.total_warnings
                );
            }

            if report.files_migrated > 0 {
                println!();
                println!(
                    "{} Output written to {}",
                    "=>".green().bold(),
                    output.underline()
                );
            }

            println!();
            Ok(if report.files_failed > 0 { 1 } else { 0 })
        }
    }
}
