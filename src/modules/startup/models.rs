use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StartupSource {
    RegistryRun,
    RegistryRunOnce,
    RegistryPolicyRun,
    StartupFolder,
    ScheduledTask,
    Service,
}

impl StartupSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::RegistryRun => "registry_run",
            Self::RegistryRunOnce => "registry_run_once",
            Self::RegistryPolicyRun => "registry_policy_run",
            Self::StartupFolder => "startup_folder",
            Self::ScheduledTask => "scheduled_task",
            Self::Service => "service",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StartupScope {
    User,
    Machine,
}

impl StartupScope {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Machine => "machine",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StartupState {
    Enabled,
    Disabled,
    Broken,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StartupAction {
    Enable,
    Disable,
    Delete,
    Rollback,
}

impl StartupAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Enable => "enable",
            Self::Disable => "disable",
            Self::Delete => "delete",
            Self::Rollback => "rollback",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StartupCapabilities {
    pub can_enable: bool,
    pub can_disable: bool,
    pub can_delete: bool,
    pub can_rollback: bool,
}

impl StartupCapabilities {
    pub fn for_source(source: StartupSource) -> Self {
        match source {
            StartupSource::RegistryRun => Self {
                can_enable: true,
                can_disable: true,
                can_delete: true,
                can_rollback: true,
            },
            StartupSource::RegistryRunOnce => Self {
                can_enable: false,
                can_disable: false,
                can_delete: true,
                can_rollback: true,
            },
            StartupSource::RegistryPolicyRun => Self {
                can_enable: true,
                can_disable: true,
                can_delete: true,
                can_rollback: true,
            },
            StartupSource::StartupFolder => Self {
                can_enable: true,
                can_disable: true,
                can_delete: true,
                can_rollback: true,
            },
            StartupSource::ScheduledTask => Self {
                can_enable: true,
                can_disable: true,
                can_delete: true,
                can_rollback: true,
            },
            StartupSource::Service => Self {
                can_enable: true,
                can_disable: true,
                can_delete: false,
                can_rollback: true,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StartupLocator {
    pub location: String,
    #[serde(default)]
    pub bucket: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StartupItem {
    pub id: String,
    pub name: String,
    pub source: StartupSource,
    pub scope: StartupScope,
    pub state: StartupState,
    pub command: Option<String>,
    pub executable_path: Option<String>,
    #[serde(default)]
    pub arguments: Vec<String>,
    pub working_dir: Option<String>,
    pub target_exists: Option<bool>,
    pub requires_admin: bool,
    pub capabilities: StartupCapabilities,
    pub locator: StartupLocator,
    #[serde(default)]
    pub warnings: Vec<String>,
    pub description: Option<String>,
    pub raw: Option<serde_json::Value>,
}

impl StartupItem {
    pub fn new(
        name: impl Into<String>,
        source: StartupSource,
        scope: StartupScope,
        locator: StartupLocator,
    ) -> Self {
        let locator_key = locator_key(source, scope, &locator);
        Self {
            id: locator_key,
            name: name.into(),
            source,
            scope,
            state: StartupState::Enabled,
            command: None,
            executable_path: None,
            arguments: Vec::new(),
            working_dir: None,
            target_exists: None,
            requires_admin: matches!(scope, StartupScope::Machine),
            capabilities: StartupCapabilities::for_source(source),
            locator,
            warnings: Vec::new(),
            description: None,
            raw: None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct StartupListQuery {
    pub source: Option<StartupSource>,
    pub scope: Option<StartupScope>,
    pub state: Option<StartupState>,
    pub search: Option<String>,
    pub limit: Option<usize>,
    pub offset: usize,
    pub sort_by: Option<String>,
    pub descending: bool,
    pub include_raw: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartupListResponse {
    pub items: Vec<StartupItem>,
    pub total: usize,
    pub applied_limit: Option<usize>,
    pub applied_offset: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartupErrorDetail {
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartupEnvelope<T> {
    pub ok: bool,
    pub data: Option<T>,
    #[serde(default)]
    pub warnings: Vec<String>,
    pub error: Option<StartupErrorDetail>,
}

impl<T> StartupEnvelope<T> {
    pub fn success(data: T) -> Self {
        Self {
            ok: true,
            data: Some(data),
            warnings: Vec::new(),
            error: None,
        }
    }

    pub fn failure(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            ok: false,
            data: None,
            warnings: Vec::new(),
            error: Some(StartupErrorDetail {
                code: code.into(),
                message: message.into(),
            }),
        }
    }

    pub fn from_startup_error(error: &StartupError) -> Self {
        Self {
            ok: false,
            data: None,
            warnings: Vec::new(),
            error: Some(error.detail()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartupActionPlan {
    pub item_id: String,
    pub action: StartupAction,
    pub apply_requested: bool,
    pub will_apply: bool,
    pub requires_admin: bool,
    pub change_id: Option<String>,
    #[serde(default)]
    pub warnings: Vec<String>,
    #[serde(default)]
    pub operations: Vec<String>,
    pub snapshot_available: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartupActionResult {
    pub item_id: Option<String>,
    pub action: StartupAction,
    pub applied: bool,
    pub change_id: Option<String>,
    #[serde(default)]
    pub warnings: Vec<String>,
    #[serde(default)]
    pub operations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartupAddPlan {
    pub item: StartupItem,
    pub apply_requested: bool,
    pub will_apply: bool,
    pub requires_admin: bool,
    #[serde(default)]
    pub warnings: Vec<String>,
    #[serde(default)]
    pub operations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartupAddResult {
    pub item: StartupItem,
    pub applied: bool,
    #[serde(default)]
    pub warnings: Vec<String>,
    #[serde(default)]
    pub operations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartupChangeLog {
    pub change_id: String,
    pub item_id: String,
    pub action: StartupAction,
    pub source: StartupSource,
    pub scope: StartupScope,
    pub created_at: String,
    pub reason: Option<String>,
    pub snapshot_json: String,
    pub restored_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartupSnapshot {
    pub item: StartupItem,
    #[serde(default)]
    pub source_payload: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisabledStartupRecord {
    pub item_id: String,
    pub source: StartupSource,
    pub scope: StartupScope,
    pub snapshot_json: String,
    pub disabled_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartupSourceDescriptor {
    pub source: StartupSource,
    pub label: String,
    pub supports_user_scope: bool,
    pub supports_machine_scope: bool,
    pub capabilities: StartupCapabilities,
    pub notes: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StartupErrorCode {
    RequiresAdmin,
    NotFound,
    Unsupported,
    InvalidSelector,
    Conflict,
    IoError,
}

impl StartupErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::RequiresAdmin => "requires_admin",
            Self::NotFound => "not_found",
            Self::Unsupported => "unsupported",
            Self::InvalidSelector => "invalid_selector",
            Self::Conflict => "conflict",
            Self::IoError => "io_error",
        }
    }
}

#[derive(Debug, Clone, Error, Serialize, Deserialize)]
#[error("{message}")]
pub struct StartupError {
    pub code: StartupErrorCode,
    pub message: String,
}

impl StartupError {
    pub fn new(code: StartupErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    pub fn detail(&self) -> StartupErrorDetail {
        StartupErrorDetail {
            code: self.code.as_str().to_string(),
            message: self.message.clone(),
        }
    }
}

pub fn locator_key(source: StartupSource, scope: StartupScope, locator: &StartupLocator) -> String {
    let raw = format!(
        "{}:{}:{}:{}",
        source.as_str(),
        scope.as_str(),
        locator.location,
        locator.bucket.clone().unwrap_or_default()
    );
    format!("startup:{}", hex_encode(raw.as_bytes()))
}

pub fn hex_encode(input: &[u8]) -> String {
    let mut output = String::with_capacity(input.len() * 2);
    for byte in input {
        output.push_str(&format!("{byte:02x}"));
    }
    output
}

pub fn hex_decode(input: &str) -> Result<Vec<u8>, StartupError> {
    if input.len() % 2 != 0 {
        return Err(StartupError::new(
            StartupErrorCode::IoError,
            "十六进制字符串长度无效",
        ));
    }

    let mut output = Vec::with_capacity(input.len() / 2);
    let bytes = input.as_bytes();

    for chunk in bytes.chunks(2) {
        let part = std::str::from_utf8(chunk).map_err(|_| {
            StartupError::new(StartupErrorCode::IoError, "十六进制字符串包含无效 UTF-8")
        })?;
        let value = u8::from_str_radix(part, 16).map_err(|_| {
            StartupError::new(StartupErrorCode::IoError, "十六进制字符串包含非法字符")
        })?;
        output.push(value);
    }

    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::{
        hex_decode, hex_encode, locator_key, StartupCapabilities, StartupEnvelope, StartupLocator,
        StartupScope, StartupSource,
    };

    #[test]
    fn locator_key_is_deterministic() {
        let locator = StartupLocator {
            location: r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run\Demo".to_string(),
            bucket: Some("run".to_string()),
        };

        let first = locator_key(StartupSource::RegistryRun, StartupScope::User, &locator);
        let second = locator_key(StartupSource::RegistryRun, StartupScope::User, &locator);

        assert_eq!(first, second);
    }

    #[test]
    fn service_capability_matrix_disables_delete() {
        let capabilities = StartupCapabilities::for_source(StartupSource::Service);

        assert!(capabilities.can_enable);
        assert!(capabilities.can_disable);
        assert!(!capabilities.can_delete);
        assert!(capabilities.can_rollback);
    }

    #[test]
    fn startup_envelope_failure_contains_fixed_error_code() {
        let envelope: StartupEnvelope<()> =
            StartupEnvelope::failure("requires_admin", "需要管理员权限");

        assert!(!envelope.ok);
        assert!(envelope.data.is_none());
        assert_eq!(
            envelope.error.map(|error| error.code),
            Some("requires_admin".to_string())
        );
    }

    #[test]
    fn hex_roundtrip_preserves_binary_payload() {
        let payload = vec![0, 15, 16, 255];
        let encoded = hex_encode(&payload);
        let decoded = hex_decode(&encoded).unwrap_or_default();

        assert_eq!(decoded, payload);
    }
}
