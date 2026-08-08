use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ElevationErrorCode {
    UnsupportedStandardUser,
    UnsafeInstallLocation,
    ElevationTaskMissing,
    ElevationTaskInvalid,
    ElevationTaskAccessDenied,
    ElevationLaunchFailed,
    ElevationLaunchTimeout,
    ComInitializationFailed,
}

impl ElevationErrorCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::UnsupportedStandardUser => "unsupported_standard_user",
            Self::UnsafeInstallLocation => "unsafe_install_location",
            Self::ElevationTaskMissing => "elevation_task_missing",
            Self::ElevationTaskInvalid => "elevation_task_invalid",
            Self::ElevationTaskAccessDenied => "elevation_task_access_denied",
            Self::ElevationLaunchFailed => "elevation_launch_failed",
            Self::ElevationLaunchTimeout => "elevation_launch_timeout",
            Self::ComInitializationFailed => "com_initialization_failed",
        }
    }
}

#[derive(Debug, Clone, Serialize, Error)]
#[error("{message}")]
pub struct ElevationError {
    pub code: ElevationErrorCode,
    pub message: String,
}

impl ElevationError {
    pub fn new(code: ElevationErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}
