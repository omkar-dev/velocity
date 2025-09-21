use std::path::PathBuf;

/// Classification of errors for automatic retry decisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    // Transient — auto-retry with backoff
    ConnectionLost,
    SessionExpired,
    ElementStale,
    DeviceNotReady,

    // Permanent — fail fast
    ElementNotFound,
    InvalidSelector,
    AssertionFailed,
    PlatformNotSupported,
    ConfigError,

    // Ambiguous — retry once, then escalate
    Timeout,
    Unknown,
}

impl ErrorKind {
    pub fn is_transient(self) -> bool {
        matches!(
            self,
            Self::ConnectionLost | Self::SessionExpired | Self::ElementStale | Self::DeviceNotReady
        )
    }

    pub fn is_permanent(self) -> bool {
        matches!(
            self,
            Self::ElementNotFound
                | Self::InvalidSelector
                | Self::AssertionFailed
                | Self::PlatformNotSupported
                | Self::ConfigError
        )
    }

    pub fn max_retries(self) -> u32 {
        match self {
            _ if self.is_transient() => 3,
            Self::Timeout | Self::Unknown => 1,
            _ => 0,
        }
    }
}

#[derive(thiserror::Error, Debug)]
pub enum VelocityError {
    // Configuration Errors (exit code 2)
    #[error("YAML parse error at {file}:{line}:{col}: {message}")]
    YamlParse {
        file: PathBuf,
        line: usize,
        col: usize,
        message: String,
    },

    #[error("Unknown flow reference '{flow_id}' in test '{test_name}'")]
    UnknownFlowRef {
        flow_id: String,
        test_name: String,
    },

    #[error("Invalid selector in step {step_index} of '{test_name}': {reason}")]
    InvalidSelector {
        test_name: String,
        step_index: usize,
        reason: String,
    },

    #[error("Missing required environment variable(s): {}", vars.join(", "))]
    MissingEnvVars { vars: Vec<String> },

    #[error("Configuration error: {0}")]
    Config(String),

    // Device Errors (exit code 3)
    #[error("Device '{id}' not found. Available: {}", available.join(", "))]
    DeviceNotFound { id: String, available: Vec<String> },

    #[error("Failed to boot device '{id}': {reason}")]
    DeviceBootFailed { id: String, reason: String },

    #[error("WDA health check failed after {timeout_s}s on device '{device_id}'")]
    WdaUnhealthy { device_id: String, timeout_s: u64 },

    #[error("WDA session lost during test '{test_name}' (restart attempt {attempt}/{max})")]
    WdaSessionLost {
        test_name: String,
        attempt: u32,
        max: u32,
    },

    #[error("ADB connection lost to device '{device_id}': {reason}")]
    AdbConnectionLost { device_id: String, reason: String },

    // Test Failures (exit code 1)
    #[error("Element not found: {selector} after {timeout_ms}ms")]
    ElementNotFound {
        selector: String,
        timeout_ms: u64,
        screenshot: Option<PathBuf>,
        hierarchy_snapshot: Option<String>,
    },

    #[error("Assertion failed: expected {expected:?}, got {actual:?}")]
    AssertionFailed {
        expected: String,
        actual: String,
        selector: String,
        screenshot: Option<PathBuf>,
    },

    #[error(
        "Sync timeout: UI did not stabilize within {timeout_ms}ms \
         ({stable_count}/{required} stable frames)"
    )]
    SyncTimeout {
        timeout_ms: u64,
        stable_count: u32,
        required: u32,
    },

    #[error("Step timeout: step {step_index} in '{test_name}' exceeded {timeout_ms}ms")]
    StepTimeout {
        test_name: String,
        step_index: usize,
        timeout_ms: u64,
    },

    // Timeout (exit code 4)
    #[error("Suite timeout: exceeded {timeout_ms}ms ({completed}/{total} tests completed)")]
    SuiteTimeout {
        timeout_ms: u64,
        completed: usize,
        total: usize,
    },

    // Internal Errors (exit code 5)
    #[error("Internal error: {0}")]
    Internal(#[from] anyhow::Error),
}

impl VelocityError {
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::ElementNotFound { .. }
            | Self::AssertionFailed { .. }
            | Self::SyncTimeout { .. }
            | Self::StepTimeout { .. } => 1,

            Self::YamlParse { .. }
            | Self::UnknownFlowRef { .. }
            | Self::InvalidSelector { .. }
            | Self::MissingEnvVars { .. }
            | Self::Config(_) => 2,

            Self::DeviceNotFound { .. }
            | Self::DeviceBootFailed { .. }
            | Self::WdaUnhealthy { .. }
            | Self::WdaSessionLost { .. }
            | Self::AdbConnectionLost { .. } => 3,

            Self::SuiteTimeout { .. } => 4,

            Self::Internal(_) => 5,
        }
    }
}

impl VelocityError {
    pub fn kind(&self) -> ErrorKind {
        match self {
            Self::AdbConnectionLost { .. } => ErrorKind::ConnectionLost,
            Self::WdaSessionLost { .. } => ErrorKind::SessionExpired,
            Self::WdaUnhealthy { .. } => ErrorKind::DeviceNotReady,
            Self::DeviceBootFailed { .. } => ErrorKind::DeviceNotReady,
            Self::DeviceNotFound { .. } => ErrorKind::DeviceNotReady,

            Self::ElementNotFound { .. } => ErrorKind::ElementNotFound,
            Self::InvalidSelector { .. } => ErrorKind::InvalidSelector,
            Self::AssertionFailed { .. } => ErrorKind::AssertionFailed,

            Self::SyncTimeout { .. } | Self::StepTimeout { .. } => ErrorKind::Timeout,
            Self::SuiteTimeout { .. } => ErrorKind::Timeout,

            Self::YamlParse { .. }
            | Self::UnknownFlowRef { .. }
            | Self::MissingEnvVars { .. }
            | Self::Config(_) => ErrorKind::ConfigError,

            Self::Internal(_) => ErrorKind::Unknown,
        }
    }

    pub fn is_transient(&self) -> bool {
        self.kind().is_transient()
    }
}

pub type Result<T> = std::result::Result<T, VelocityError>;
