use serde::{Deserialize, Serialize};
use thiserror::Error;

/// 跨 application/Tauri 边界保持稳定的卸载错误代码。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UninstallErrorCode {
    AdminRequired,
    UnsupportedStandardUser,
    UnsafeInstallLocation,
    ElevationTaskMissing,
    ElevationTaskInvalid,
    ElevationTaskAccessDenied,
    ElevationLaunchFailed,
    ElevationLaunchTimeout,
    JobConflict,
    JobNotFound,
    InvalidJobState,
    TargetChanged,
    ForceTargetInvalid,
    ForceTargetNotFound,
    ConfirmationRequired,
    NoTraceSelected,
    TraceNotInPlan,
    UninstallerCancelled,
    UninstallerFailed,
    RemovalNotConfirmed,
    ResidueScanFailed,
    CleanupFailed,
}

impl UninstallErrorCode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AdminRequired => "admin_required",
            Self::UnsupportedStandardUser => "unsupported_standard_user",
            Self::UnsafeInstallLocation => "unsafe_install_location",
            Self::ElevationTaskMissing => "elevation_task_missing",
            Self::ElevationTaskInvalid => "elevation_task_invalid",
            Self::ElevationTaskAccessDenied => "elevation_task_access_denied",
            Self::ElevationLaunchFailed => "elevation_launch_failed",
            Self::ElevationLaunchTimeout => "elevation_launch_timeout",
            Self::JobConflict => "job_conflict",
            Self::JobNotFound => "job_not_found",
            Self::InvalidJobState => "invalid_job_state",
            Self::TargetChanged => "target_changed",
            Self::ForceTargetInvalid => "force_target_invalid",
            Self::ForceTargetNotFound => "force_target_not_found",
            Self::ConfirmationRequired => "confirmation_required",
            Self::NoTraceSelected => "no_trace_selected",
            Self::TraceNotInPlan => "trace_not_in_plan",
            Self::UninstallerCancelled => "uninstaller_cancelled",
            Self::UninstallerFailed => "uninstaller_failed",
            Self::RemovalNotConfirmed => "removal_not_confirmed",
            Self::ResidueScanFailed => "residue_scan_failed",
            Self::CleanupFailed => "cleanup_failed",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Error)]
#[error("{message}")]
pub struct UninstallError {
    pub code: UninstallErrorCode,
    pub message: String,
}

impl UninstallError {
    pub fn new(code: UninstallErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn invalid_state(message: impl Into<String>) -> Self {
        Self::new(UninstallErrorCode::InvalidJobState, message)
    }
}

#[cfg(test)]
mod tests {
    use super::UninstallErrorCode;

    #[test]
    fn stable_codes_are_lower_snake_case() {
        assert_eq!(UninstallErrorCode::AdminRequired.as_str(), "admin_required");
        assert_eq!(
            UninstallErrorCode::TraceNotInPlan.as_str(),
            "trace_not_in_plan"
        );
    }
}
