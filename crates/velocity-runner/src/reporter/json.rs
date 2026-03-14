use velocity_common::{Result, SuiteResult, VelocityError};

/// Write a JSON report for the suite result to the given file path.
pub fn write_json(result: &SuiteResult, path: &str) -> Result<()> {
    if let Some(parent) = std::path::Path::new(path).parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            VelocityError::Config(format!("failed to create report directory: {e}"))
        })?;
    }

    let json = serde_json::to_string_pretty(result)
        .map_err(|e| VelocityError::Config(format!("failed to serialize suite result: {e}")))?;

    std::fs::write(path, json).map_err(|e| {
        VelocityError::Config(format!("failed to write JSON report at {path}: {e}"))
    })?;

    Ok(())
}
