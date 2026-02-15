use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct CommandError {
    pub message: String,
}

impl CommandError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl std::fmt::Display for CommandError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for CommandError {}

impl From<String> for CommandError {
    fn from(message: String) -> Self {
        Self::new(message)
    }
}

impl From<&str> for CommandError {
    fn from(message: &str) -> Self {
        Self::new(message)
    }
}

impl From<rust_yu_lib::UninstallerError> for CommandError {
    fn from(error: rust_yu_lib::UninstallerError) -> Self {
        Self::new(error.to_string())
    }
}
