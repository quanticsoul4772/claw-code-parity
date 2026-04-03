use std::fmt;

/// Typed error for tool execution, replacing stringly-typed `Result<String, String>`.
#[derive(Debug)]
pub enum ToolExecutionError {
    /// File system or process I/O failure.
    Io(std::io::Error),
    /// JSON serialization or deserialization failure.
    Json(serde_json::Error),
    /// Input validation failure (missing fields, invalid values, constraint violations).
    Validation(String),
    /// Requested tool does not exist.
    ToolNotFound(String),
    /// External operation failure (network, subprocess, etc.).
    External(String),
}

impl fmt::Display for ToolExecutionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "{error}"),
            Self::Json(error) => write!(f, "{error}"),
            Self::Validation(message) => write!(f, "{message}"),
            Self::ToolNotFound(name) => write!(f, "unsupported tool: {name}"),
            Self::External(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for ToolExecutionError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Json(error) => Some(error),
            _ => None,
        }
    }
}

impl From<std::io::Error> for ToolExecutionError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for ToolExecutionError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

impl From<String> for ToolExecutionError {
    fn from(message: String) -> Self {
        Self::Validation(message)
    }
}

impl ToolExecutionError {
    /// Check if the error message contains a substring. Useful for test assertions.
    #[must_use]
    pub fn contains(&self, needle: &str) -> bool {
        self.to_string().contains(needle)
    }
}
