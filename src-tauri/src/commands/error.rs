use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct CommandError {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    pub message: String,
}

impl CommandError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            code: None,
            message: message.into(),
        }
    }

    pub fn with_code(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: Some(code.into()),
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
        match error {
            rust_yu_lib::UninstallerError::PermissionDenied(message) => {
                Self::with_code("admin_required", message)
            }
            other => Self::new(other.to_string()),
        }
    }
}

impl From<rust_yu_lib::startup::models::StartupError> for CommandError {
    fn from(error: rust_yu_lib::startup::models::StartupError) -> Self {
        Self::with_code(error.code.as_str(), error.message)
    }
}

#[cfg(test)]
mod tests {
    use super::CommandError;

    #[test]
    fn permission_denied_preserves_structured_admin_required_code() {
        let error = CommandError::from(rust_yu_lib::UninstallerError::PermissionDenied(
            "请以管理员身份重新运行".to_string(),
        ));

        assert_eq!(error.code.as_deref(), Some("admin_required"));
        assert_eq!(error.message, "请以管理员身份重新运行");
    }

    #[test]
    fn non_permission_errors_keep_unstructured_error_code_empty() {
        let error = CommandError::from(rust_yu_lib::UninstallerError::NotFound(
            "missing".to_string(),
        ));

        assert_eq!(error.code, None);
        assert_eq!(error.message, "未找到: missing");
    }
}
