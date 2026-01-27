use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationIssue {
    pub line: usize,
    pub severity: Severity,
    pub message: String,
    pub maestro_construct: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileMigrationResult {
    pub source_file: String,
    pub output_file: Option<String>,
    pub success: bool,
    pub issues: Vec<MigrationIssue>,
    pub steps_migrated: usize,
    pub steps_skipped: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationReport {
    pub files_total: usize,
    pub files_migrated: usize,
    pub files_failed: usize,
    pub total_warnings: usize,
    pub total_errors: usize,
    pub results: Vec<FileMigrationResult>,
}

impl MigrationReport {
    pub fn new() -> Self {
        Self {
            files_total: 0,
            files_migrated: 0,
            files_failed: 0,
            total_warnings: 0,
            total_errors: 0,
            results: Vec::new(),
        }
    }

    pub fn add_result(&mut self, result: FileMigrationResult) {
        self.files_total += 1;
        if result.success {
            self.files_migrated += 1;
        } else {
            self.files_failed += 1;
        }
        for issue in &result.issues {
            match issue.severity {
                Severity::Warning => self.total_warnings += 1,
                Severity::Error => self.total_errors += 1,
                Severity::Info => {}
            }
        }
        self.results.push(result);
    }
}

impl Default for MigrationReport {
    fn default() -> Self {
        Self::new()
    }
}

pub fn generate_report_json(report: &MigrationReport) -> String {
    serde_json::to_string_pretty(report).unwrap_or_else(|e| {
        format!("{{\"error\": \"Failed to serialize report: {e}\"}}")
    })
}
