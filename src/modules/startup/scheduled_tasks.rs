use std::process::Command;

use super::models::{
    StartupError, StartupErrorCode, StartupItem, StartupLocator, StartupScope, StartupSnapshot,
    StartupSource, StartupState,
};

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "PascalCase")]
struct TaskRecord {
    task_name: String,
    task_path: String,
    enabled: bool,
    description: Option<String>,
    user_id: Option<String>,
    command: Option<String>,
    arguments: Option<String>,
    working_directory: Option<String>,
    trigger_types: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct TaskSnapshotPayload {
    task_name: String,
    task_path: String,
    xml: String,
}

pub fn collect_items(include_raw: bool) -> Result<Vec<StartupItem>, StartupError> {
    let script = r#"
$tasks = Get-ScheduledTask | ForEach-Object {
  $triggerTypes = @($_.Triggers | ForEach-Object { $_.CimClass.CimClassName })
  if (($triggerTypes | Where-Object { $_ -in @('MSFT_TaskLogonTrigger', 'MSFT_TaskBootTrigger') }).Count -eq 0) {
    return
  }
  $action = $_.Actions | Select-Object -First 1
  [pscustomobject]@{
    TaskName = $_.TaskName
    TaskPath = $_.TaskPath
    Enabled = [bool]$_.Settings.Enabled
    Description = $_.Description
    UserId = $_.Principal.UserId
    Command = if ($action) { $action.Execute } else { $null }
    Arguments = if ($action) { $action.Arguments } else { $null }
    WorkingDirectory = if ($action) { $action.WorkingDirectory } else { $null }
    TriggerTypes = ($triggerTypes -join ';')
  }
}
$tasks | ConvertTo-Json -Depth 4 -Compress
"#;
    let records: Vec<TaskRecord> = parse_json_array(&run_powershell(script)?)?;
    let mut items = Vec::new();

    for record in records {
        let scope = classify_scope(record.user_id.as_deref());
        let mut item = StartupItem::new(
            &record.task_name,
            StartupSource::ScheduledTask,
            scope,
            StartupLocator {
                location: format!("{}{}", record.task_path, record.task_name),
                bucket: Some("scheduled_task".to_string()),
            },
        );
        item.state = if record.enabled {
            StartupState::Enabled
        } else {
            StartupState::Disabled
        };
        item.command = combine_command(record.command.clone(), record.arguments.clone());
        item.executable_path = record.command.clone();
        item.arguments = record
            .arguments
            .map(|value| vec![value])
            .unwrap_or_default();
        item.working_dir = record.working_directory.clone();
        item.target_exists = record
            .command
            .as_deref()
            .map(|value| std::path::Path::new(value).exists());
        item.requires_admin = matches!(scope, StartupScope::Machine);
        item.description = record.description.clone();

        if item.state != StartupState::Disabled && item.target_exists == Some(false) {
            item.state = StartupState::Broken;
        }

        if include_raw {
            item.raw = Some(serde_json::json!({
                "task_path": record.task_path,
                "user_id": record.user_id,
                "trigger_types": record.trigger_types,
            }));
        }

        items.push(item);
    }

    Ok(items)
}

pub fn capture_snapshot(item: &StartupItem) -> Result<StartupSnapshot, StartupError> {
    let (task_path, task_name) = split_task_locator(&item.locator.location)?;
    let script = format!(
        "$xml = Export-ScheduledTask -TaskName '{}' -TaskPath '{}'; $xml",
        ps_escape(&task_name),
        ps_escape(&task_path)
    );
    let xml = run_powershell(&script)?;
    let payload = TaskSnapshotPayload {
        task_name,
        task_path,
        xml,
    };

    Ok(StartupSnapshot {
        item: item.clone(),
        source_payload: serde_json::to_value(payload).map_err(|error| {
            StartupError::new(
                StartupErrorCode::IoError,
                format!("序列化计划任务快照失败: {error}"),
            )
        })?,
    })
}

pub fn apply_action(
    _item: &StartupItem,
    action: super::models::StartupAction,
    snapshot: &StartupSnapshot,
) -> Result<Vec<String>, StartupError> {
    let payload: TaskSnapshotPayload = serde_json::from_value(snapshot.source_payload.clone())
        .map_err(|error| {
            StartupError::new(
                StartupErrorCode::IoError,
                format!("解析计划任务快照失败: {error}"),
            )
        })?;

    let script = match action {
        super::models::StartupAction::Enable => format!(
            "Enable-ScheduledTask -TaskName '{}' -TaskPath '{}' | Out-Null",
            ps_escape(&payload.task_name),
            ps_escape(&payload.task_path)
        ),
        super::models::StartupAction::Disable => format!(
            "Disable-ScheduledTask -TaskName '{}' -TaskPath '{}' | Out-Null",
            ps_escape(&payload.task_name),
            ps_escape(&payload.task_path)
        ),
        super::models::StartupAction::Delete => format!(
            "Unregister-ScheduledTask -TaskName '{}' -TaskPath '{}' -Confirm:$false | Out-Null",
            ps_escape(&payload.task_name),
            ps_escape(&payload.task_path)
        ),
        _ => {
            return Err(StartupError::new(
                StartupErrorCode::Unsupported,
                "计划任务不支持该操作",
            ));
        }
    };

    run_powershell(&script)?;
    Ok(vec![format!(
        "{} 计划任务 {}{}",
        action.as_str(),
        payload.task_path,
        payload.task_name
    )])
}

pub fn restore_snapshot(snapshot: &StartupSnapshot) -> Result<Vec<String>, StartupError> {
    let payload: TaskSnapshotPayload = serde_json::from_value(snapshot.source_payload.clone())
        .map_err(|error| {
            StartupError::new(
                StartupErrorCode::IoError,
                format!("解析计划任务快照失败: {error}"),
            )
        })?;
    let script = format!(
        "$xml = @'\n{}\n'@; Register-ScheduledTask -TaskName '{}' -TaskPath '{}' -Xml $xml -Force | Out-Null",
        payload.xml.replace("'", "''"),
        ps_escape(&payload.task_name),
        ps_escape(&payload.task_path)
    );
    run_powershell(&script)?;
    Ok(vec![format!(
        "恢复计划任务 {}{}",
        payload.task_path, payload.task_name
    )])
}

fn run_powershell(script: &str) -> Result<String, StartupError> {
    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            script,
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
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
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
            format!("解析计划任务 JSON 失败: {error}"),
        )
    })?;
    match value {
        serde_json::Value::Array(_) => serde_json::from_value(value).map_err(|error| {
            StartupError::new(
                StartupErrorCode::IoError,
                format!("解析计划任务列表失败: {error}"),
            )
        }),
        serde_json::Value::Null => Ok(Vec::new()),
        other => serde_json::from_value::<T>(other)
            .map(|item| vec![item])
            .map_err(|error| {
                StartupError::new(
                    StartupErrorCode::IoError,
                    format!("解析计划任务单项失败: {error}"),
                )
            }),
    }
}

fn classify_scope(user_id: Option<&str>) -> StartupScope {
    let Some(user_id) = user_id else {
        return StartupScope::Machine;
    };
    let trimmed = user_id.trim();
    if trimmed.is_empty()
        || matches!(trimmed, "SYSTEM" | "LOCAL SERVICE" | "NETWORK SERVICE")
        || trimmed.starts_with("S-1-5-18")
        || trimmed.starts_with("S-1-5-19")
        || trimmed.starts_with("S-1-5-20")
    {
        StartupScope::Machine
    } else {
        StartupScope::User
    }
}

fn combine_command(command: Option<String>, arguments: Option<String>) -> Option<String> {
    command.map(|value| match arguments {
        Some(arguments) if !arguments.trim().is_empty() => format!("{value} {arguments}"),
        _ => value,
    })
}

fn split_task_locator(locator: &str) -> Result<(String, String), StartupError> {
    let normalized = locator.replace('/', "\\");
    let Some(position) = normalized.rfind('\\') else {
        return Err(StartupError::new(
            StartupErrorCode::InvalidSelector,
            "计划任务定位信息无效",
        ));
    };
    let task_path = normalized[..=position].to_string();
    let task_name = normalized[position + 1..].to_string();
    Ok((task_path, task_name))
}

fn ps_escape(value: &str) -> String {
    value.replace('\'', "''")
}
