use std::process::Command;

use crate::modules::common::text::{build_powershell_script, decode_windows_output};
use crate::modules::common::utils::split_command_for_spawn;

use super::models::{
    StartupCapabilities, StartupError, StartupErrorCode, StartupItem, StartupLocator, StartupScope,
    StartupSnapshot, StartupSource, StartupState,
};

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ServiceRecord {
    name: String,
    display_name: String,
    start_mode: String,
    state: Option<String>,
    path_name: Option<String>,
    description: Option<String>,
    start_name: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct ServiceSnapshotPayload {
    name: String,
    start_mode: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ServiceSnapshotRecord {
    name: String,
    start_mode: String,
}

pub fn collect_items(include_raw: bool) -> Result<Vec<StartupItem>, StartupError> {
    let script = r#"
$services = Get-CimInstance Win32_Service | Where-Object {
  $_.ServiceType -notmatch 'Kernel Driver|File System Driver' -and $_.StartMode -in @('Auto', 'Disabled')
} | ForEach-Object {
  [pscustomobject]@{
    Name = $_.Name
    DisplayName = $_.DisplayName
    StartMode = $_.StartMode
    State = $_.State
    PathName = $_.PathName
    Description = $_.Description
    StartName = $_.StartName
  }
}
$services | ConvertTo-Json -Depth 4 -Compress
"#;
    let records: Vec<ServiceRecord> = parse_json_array(&run_powershell(script)?)?;
    let mut items = Vec::new();

    for record in records {
        let mut item = StartupItem::new(
            &record.display_name,
            StartupSource::Service,
            StartupScope::Machine,
            StartupLocator {
                location: record.name.clone(),
                bucket: Some("service".to_string()),
            },
        );
        item.state = if record.start_mode.eq_ignore_ascii_case("Disabled") {
            StartupState::Disabled
        } else {
            StartupState::Enabled
        };
        item.command = record.path_name.clone();
        if let Some(command) = record.path_name.as_deref() {
            if let Ok((executable, arguments)) = split_command_for_spawn(command) {
                item.executable_path = Some(executable.clone());
                item.arguments = arguments;
                item.working_dir = std::path::Path::new(&executable)
                    .parent()
                    .map(|value| value.to_string_lossy().to_string());
                item.target_exists = if executable.contains('\\') || executable.contains('/') {
                    Some(std::path::Path::new(&executable).exists())
                } else {
                    None
                };
            }
        }
        item.requires_admin = true;
        item.description = record.description.clone();
        if item
            .executable_path
            .as_deref()
            .is_some_and(is_windows_system_executable)
        {
            // Windows 目录内的服务通常是系统组件；没有可靠归属信息时宁可只读，也不允许误禁用。
            item.capabilities = StartupCapabilities {
                can_enable: false,
                can_disable: false,
                can_delete: false,
                can_rollback: false,
            };
            item.warnings
                .push("Windows 系统服务受保护，仅供查看".to_string());
        }
        if item.state != StartupState::Disabled && item.target_exists == Some(false) {
            item.state = StartupState::Broken;
        }
        if include_raw {
            item.raw = Some(serde_json::json!({
                "service_name": record.name,
                "start_mode": record.start_mode,
                "state": record.state,
                "start_name": record.start_name,
            }));
        }

        items.push(item);
    }

    Ok(items)
}

fn is_windows_system_executable(executable: &str) -> bool {
    let executable = executable.replace('/', "\\").to_ascii_lowercase();
    [std::env::var("WINDIR"), std::env::var("SystemRoot")]
        .into_iter()
        .flatten()
        .map(|root| root.replace('/', "\\").to_ascii_lowercase())
        .any(|root| executable == root || executable.starts_with(&format!(r"{root}\")))
}

pub fn capture_snapshot(item: &StartupItem) -> Result<StartupSnapshot, StartupError> {
    let service_name = item.locator.location.clone();
    let script = format!(
        "Get-CimInstance Win32_Service -Filter \"Name='{}'\" | Select-Object Name, StartMode | ConvertTo-Json -Compress",
        service_name.replace('\'', "''")
    );
    let record: ServiceSnapshotRecord =
        serde_json::from_str(&run_powershell(&script)?).map_err(|error| {
            StartupError::new(
                StartupErrorCode::IoError,
                format!("解析服务快照失败: {error}"),
            )
        })?;
    let payload = ServiceSnapshotPayload {
        name: record.name,
        start_mode: record.start_mode,
    };

    Ok(StartupSnapshot {
        item: item.clone(),
        source_payload: serde_json::to_value(payload).map_err(|error| {
            StartupError::new(
                StartupErrorCode::IoError,
                format!("序列化服务快照失败: {error}"),
            )
        })?,
    })
}

pub fn apply_action(
    _item: &StartupItem,
    action: super::models::StartupAction,
    snapshot: &StartupSnapshot,
) -> Result<Vec<String>, StartupError> {
    let payload: ServiceSnapshotPayload = serde_json::from_value(snapshot.source_payload.clone())
        .map_err(|error| {
        StartupError::new(
            StartupErrorCode::IoError,
            format!("解析服务快照失败: {error}"),
        )
    })?;

    let startup_type = match action {
        super::models::StartupAction::Enable => "Automatic",
        super::models::StartupAction::Disable => "Disabled",
        super::models::StartupAction::Delete => {
            return Err(StartupError::new(
                StartupErrorCode::Unsupported,
                "服务删除未纳入 v1",
            ));
        }
        _ => {
            return Err(StartupError::new(
                StartupErrorCode::Unsupported,
                "服务不支持该操作",
            ));
        }
    };

    let script = format!(
        "Set-Service -Name '{}' -StartupType {}",
        payload.name.replace('\'', "''"),
        startup_type
    );
    run_powershell(&script)?;

    Ok(vec![format!(
        "将服务 {} 设置为 {}",
        payload.name, startup_type
    )])
}

pub fn restore_snapshot(snapshot: &StartupSnapshot) -> Result<Vec<String>, StartupError> {
    let payload: ServiceSnapshotPayload = serde_json::from_value(snapshot.source_payload.clone())
        .map_err(|error| {
        StartupError::new(
            StartupErrorCode::IoError,
            format!("解析服务快照失败: {error}"),
        )
    })?;
    let startup_type = match payload.start_mode.as_str() {
        "Disabled" => "Disabled",
        _ => "Automatic",
    };
    let script = format!(
        "Set-Service -Name '{}' -StartupType {}",
        payload.name.replace('\'', "''"),
        startup_type
    );
    run_powershell(&script)?;

    Ok(vec![format!("恢复服务 {} 的启动类型", payload.name)])
}

fn run_powershell(script: &str) -> Result<String, StartupError> {
    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            &build_powershell_script(script),
        ])
        .output()
        .map_err(|error| {
            StartupError::new(
                StartupErrorCode::IoError,
                format!("执行 PowerShell 失败: {error}"),
            )
        })?;

    if !output.status.success() {
        return Err(StartupError::new(
            StartupErrorCode::IoError,
            decode_windows_output(&output.stderr).trim().to_string(),
        ));
    }

    Ok(decode_windows_output(&output.stdout).trim().to_string())
}

fn parse_json_array<T>(json: &str) -> Result<Vec<T>, StartupError>
where
    T: serde::de::DeserializeOwned,
{
    if json.trim().is_empty() {
        return Ok(Vec::new());
    }
    let value: serde_json::Value = serde_json::from_str(json).map_err(|error| {
        StartupError::new(
            StartupErrorCode::IoError,
            format!("解析服务 JSON 失败: {error}"),
        )
    })?;
    match value {
        serde_json::Value::Array(_) => serde_json::from_value(value).map_err(|error| {
            StartupError::new(
                StartupErrorCode::IoError,
                format!("解析服务列表失败: {error}"),
            )
        }),
        serde_json::Value::Null => Ok(Vec::new()),
        other => serde_json::from_value::<T>(other)
            .map(|item| vec![item])
            .map_err(|error| {
                StartupError::new(
                    StartupErrorCode::IoError,
                    format!("解析服务单项失败: {error}"),
                )
            }),
    }
}

#[cfg(test)]
mod tests {
    use super::is_windows_system_executable;

    #[test]
    fn protects_executables_inside_windows_directory() {
        if let Some(windows_dir) =
            std::env::var_os("WINDIR").or_else(|| std::env::var_os("SystemRoot"))
        {
            let system_executable = std::path::Path::new(&windows_dir)
                .join("System32")
                .join("svchost.exe")
                .to_string_lossy()
                .to_string();
            assert!(is_windows_system_executable(&system_executable));
        }
        assert!(!is_windows_system_executable(
            r"C:\Program Files\Vendor\service.exe"
        ));
    }
}
