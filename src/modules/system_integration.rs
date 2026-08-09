//! Windows 服务、计划任务与驱动残留扫描和安全清理。
//!
//! 这里不根据服务/任务名称猜测归属，只接受命令行明确引用安装目录的项目。
//! 破坏性操作前会重新读取当前命令行，并在删除后再次查询系统状态。

use crate::modules::common::error::UninstallerError;
use crate::modules::common::text::{build_powershell_script, decode_windows_output};
use crate::modules::common::utils::{self, split_command_for_spawn};
use crate::modules::lister::models::InstalledProgram;
use crate::modules::scanner::models::{Confidence, Trace, TraceType};
use serde::Deserialize;
use std::collections::HashMap;
use std::process::Command;
use std::thread;
use std::time::Duration;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ServiceRecord {
    name: String,
    path_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct TaskRecord {
    task_name: String,
    task_path: String,
    command: Option<String>,
    arguments: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct DriverRecord {
    name: String,
    display_name: Option<String>,
    path_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ServiceQueryRecord {
    path_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct TaskQueryRecord {
    command: Option<String>,
    arguments: Option<String>,
}

/// 扫描明确引用安装目录的服务、计划任务和驱动。
pub fn scan_traces(program: &InstalledProgram) -> Vec<Trace> {
    let roots = target_roots(program);
    if roots.is_empty() {
        tracing::debug!(program = %program.name, "没有可用于系统集成关联的安全安装目录");
        return Vec::new();
    }

    let mut traces = Vec::new();
    match list_services() {
        Ok(records) => traces.extend(collect_service_traces(&program.name, &roots, &records)),
        Err(error) => {
            tracing::warn!(program = %program.name, "服务残留扫描失败，继续其他扫描: {error}")
        }
    }
    match list_tasks() {
        Ok(records) => traces.extend(collect_task_traces(&program.name, &roots, &records)),
        Err(error) => {
            tracing::warn!(program = %program.name, "计划任务残留扫描失败，继续其他扫描: {error}")
        }
    }
    match list_drivers() {
        Ok(records) => traces.extend(collect_driver_traces(&program.name, &roots, &records)),
        Err(error) => {
            tracing::warn!(program = %program.name, "驱动残留扫描失败，继续其他扫描: {error}")
        }
    }
    traces
}

/// 清理一个已经在扫描计划中明确关联的服务或计划任务。
///
/// 驱动痕迹永远是只读证据，不进入清理分支。
///
/// 调用方必须已经完成用户确认；本函数仍会再次核对关联路径，不能把旧快照
/// 直接当作删除授权，防止服务/任务在等待确认期间被替换成其他程序。
pub fn remove_trace(trace: &Trace) -> Result<(), UninstallerError> {
    if trace.is_critical {
        return Err(UninstallerError::CriticalSystemItem(
            "系统服务或计划任务受保护，不能删除".to_string(),
        ));
    }
    let related_path = trace.related_path.as_deref().ok_or_else(|| {
        UninstallerError::PermissionDenied("系统集成痕迹缺少关联路径，拒绝删除".to_string())
    })?;
    if !is_safe_related_root(related_path) {
        return Err(UninstallerError::CriticalSystemItem(
            "关联路径属于系统或共享目录，拒绝删除".to_string(),
        ));
    }

    match trace.trace_type {
        TraceType::Service => remove_service(trace, related_path),
        TraceType::ScheduledTask => remove_task(trace, related_path),
        _ => Err(UninstallerError::Other(
            "该痕迹不是服务或计划任务".to_string(),
        )),
    }
}

fn target_roots(program: &InstalledProgram) -> Vec<String> {
    let mut candidates = Vec::new();
    if let Some(location) = program.install_location.as_deref() {
        candidates.push(expand_environment_variables(location));
    }
    if let Some(command) = program.preferred_uninstall_string() {
        if let Ok((executable, _)) = split_command_for_spawn(command) {
            let executable = expand_environment_variables(&executable);
            if let Some(parent) = std::path::Path::new(&executable).parent() {
                candidates.push(parent.to_string_lossy().to_string());
            }
        }
    }

    let mut roots = candidates
        .into_iter()
        .map(|value| normalize_windows_path(&value))
        .filter(|value| is_safe_related_root(value))
        .collect::<Vec<_>>();
    roots.sort_by_key(|value| std::cmp::Reverse(value.len()));
    roots.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
    roots
}

fn collect_service_traces(
    program_name: &str,
    roots: &[String],
    records: &[ServiceRecord],
) -> Vec<Trace> {
    let mut matched = Vec::new();
    let mut command_counts = HashMap::<String, usize>::new();
    for record in records {
        let Some(command) = record.path_name.as_deref() else {
            continue;
        };
        let Some(root) = related_root(command, roots) else {
            continue;
        };
        let key = normalize_command(command);
        let count = command_counts.entry(key).or_default();
        *count += 1;
        matched.push((record, root));
    }

    matched
        .into_iter()
        .map(|(record, root)| {
            let command = record.path_name.as_deref().unwrap_or_default();
            let shared = command_counts
                .get(&normalize_command(command))
                .copied()
                .unwrap_or_default()
                > 1;
            let mut trace = Trace::new(
                program_name.to_string(),
                TraceType::Service,
                format!("service://{}", record.name),
            )
            .with_description(format!(
                "服务 {} -> {}{}",
                record.name,
                command,
                if shared {
                    "（可能与其他服务共享）"
                } else {
                    ""
                }
            ))
            .with_confidence(Confidence::High)
            .with_related_path(root.to_string());
            trace.is_critical = shared || command_is_system_path(command);
            trace
        })
        .collect()
}

fn collect_task_traces(program_name: &str, roots: &[String], records: &[TaskRecord]) -> Vec<Trace> {
    records
        .iter()
        .filter_map(|record| {
            let command = combine_command(record.command.as_deref(), record.arguments.as_deref())?;
            let root = related_root(&command, roots)?;
            let locator = task_locator(&record.task_path, &record.task_name)?;
            let mut trace = Trace::new(program_name.to_string(), TraceType::ScheduledTask, locator)
                .with_description(format!(
                    "计划任务 {}{} -> {}",
                    record.task_path, record.task_name, command
                ))
                .with_confidence(Confidence::High)
                .with_related_path(root.to_string());
            trace.is_critical =
                is_protected_task_path(&record.task_path) || command_is_system_path(&command);
            Some(trace)
        })
        .collect()
}

fn collect_driver_traces(
    program_name: &str,
    roots: &[String],
    records: &[DriverRecord],
) -> Vec<Trace> {
    records
        .iter()
        .filter_map(|record| {
            let command = record.path_name.as_deref()?;
            let root = related_root(command, roots)?;
            let display_name = record
                .display_name
                .as_deref()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or(&record.name);
            let mut trace = Trace::new(
                program_name.to_string(),
                TraceType::Driver,
                format!("driver://{}", record.name),
            )
            .with_description(format!(
                "驱动 {} ({}) -> {}（仅提供证据，不自动删除）",
                display_name, record.name, command
            ))
            .with_confidence(Confidence::High)
            .with_related_path(root.to_string());
            // 驱动涉及内核态代码和服务注册，当前版本只展示明确关联证据，
            // 即使用户勾选也必须保留，避免误删导致系统无法启动或设备失效。
            trace.is_critical = true;
            Some(trace)
        })
        .collect()
}

fn list_services() -> Result<Vec<ServiceRecord>, UninstallerError> {
    let script = r#"
$services = @(Get-CimInstance Win32_Service | Where-Object {
  $_.ServiceType -notmatch 'Kernel Driver|File System Driver'
} | ForEach-Object {
  [pscustomobject]@{
    Name = $_.Name
    PathName = $_.PathName
  }
})
$services | ConvertTo-Json -Depth 3 -Compress
"#;
    parse_json_array(&run_powershell(script)?, "服务")
}

fn list_tasks() -> Result<Vec<TaskRecord>, UninstallerError> {
    let script = r#"
$tasks = @(Get-ScheduledTask | ForEach-Object {
  $action = $_.Actions | Select-Object -First 1
  [pscustomobject]@{
    TaskName = $_.TaskName
    TaskPath = $_.TaskPath
    Command = if ($action) { $action.Execute } else { $null }
    Arguments = if ($action) { $action.Arguments } else { $null }
  }
})
$tasks | ConvertTo-Json -Depth 3 -Compress
"#;
    parse_json_array(&run_powershell(script)?, "计划任务")
}

fn list_drivers() -> Result<Vec<DriverRecord>, UninstallerError> {
    let script = r#"
$drivers = @(Get-CimInstance Win32_SystemDriver | ForEach-Object {
  [pscustomobject]@{
    Name = $_.Name
    DisplayName = $_.DisplayName
    PathName = $_.PathName
  }
})
$drivers | ConvertTo-Json -Depth 3 -Compress
"#;
    parse_json_array(&run_powershell(script)?, "驱动")
}

fn remove_service(trace: &Trace, related_path: &str) -> Result<(), UninstallerError> {
    let name = parse_service_locator(&trace.path)
        .ok_or_else(|| UninstallerError::Other("服务痕迹定位信息无效".to_string()))?;
    let current = query_service(&name)?
        .ok_or_else(|| UninstallerError::NotFound(format!("服务 {} 已不存在", name)))?;
    if !command_references_related_path(
        current.path_name.as_deref().unwrap_or_default(),
        related_path,
    ) {
        return Err(UninstallerError::PermissionDenied(
            "服务路径在确认后发生变化，拒绝删除".to_string(),
        ));
    }

    // 先尝试停止服务；已经停止时 sc.exe 会返回非零，这不应阻止后续删除。
    let _ = Command::new("sc.exe").args(["stop", &name]).output();
    run_native_command("sc.exe", &["delete", &name])?;
    wait_until_service_removed(&name)
}

fn remove_task(trace: &Trace, related_path: &str) -> Result<(), UninstallerError> {
    let (task_path, task_name) = parse_task_locator(&trace.path)
        .ok_or_else(|| UninstallerError::Other("计划任务痕迹定位信息无效".to_string()))?;
    if is_protected_task_path(&task_path) {
        return Err(UninstallerError::CriticalSystemItem(
            "Windows 系统计划任务受保护，拒绝删除".to_string(),
        ));
    }
    let current = query_task(&task_path, &task_name)?.ok_or_else(|| {
        UninstallerError::NotFound(format!("计划任务 {}{} 已不存在", task_path, task_name))
    })?;
    let command = combine_command(current.command.as_deref(), current.arguments.as_deref())
        .unwrap_or_default();
    if !command_references_related_path(&command, related_path) {
        return Err(UninstallerError::PermissionDenied(
            "计划任务动作在确认后发生变化，拒绝删除".to_string(),
        ));
    }

    let script = format!(
        "Unregister-ScheduledTask -TaskName '{}' -TaskPath '{}' -Confirm:$false -ErrorAction Stop | Out-Null",
        ps_escape(&task_name),
        ps_escape(&task_path),
    );
    run_powershell(&script)?;
    wait_until_task_removed(&task_path, &task_name)
}

fn query_service(name: &str) -> Result<Option<ServiceQueryRecord>, UninstallerError> {
    let script = format!(
        "$service = Get-CimInstance Win32_Service | Where-Object {{ $_.Name -eq '{}' }} | Select-Object -First 1; if ($null -eq $service) {{ $null }} else {{ [pscustomobject]@{{ PathName = $service.PathName }} | ConvertTo-Json -Compress }}",
        ps_escape(name),
    );
    parse_optional_json(&run_powershell(&script)?, "服务查询")
}

fn query_task(
    task_path: &str,
    task_name: &str,
) -> Result<Option<TaskQueryRecord>, UninstallerError> {
    let script = format!(
        "$task = Get-ScheduledTask -TaskName '{}' -TaskPath '{}' -ErrorAction SilentlyContinue; if ($null -eq $task) {{ $null }} else {{ $action = $task.Actions | Select-Object -First 1; [pscustomobject]@{{ Command = if ($action) {{ $action.Execute }} else {{ $null }}; Arguments = if ($action) {{ $action.Arguments }} else {{ $null }} }} | ConvertTo-Json -Compress }}",
        ps_escape(task_name),
        ps_escape(task_path),
    );
    parse_optional_json(&run_powershell(&script)?, "计划任务查询")
}

fn wait_until_service_removed(name: &str) -> Result<(), UninstallerError> {
    for _ in 0..15 {
        if query_service(name)?.is_none() {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(200));
    }
    Err(UninstallerError::Other(format!(
        "服务 {} 删除后仍可被系统查询到",
        name
    )))
}

fn wait_until_task_removed(task_path: &str, task_name: &str) -> Result<(), UninstallerError> {
    for _ in 0..15 {
        if query_task(task_path, task_name)?.is_none() {
            return Ok(());
        }
        thread::sleep(Duration::from_millis(200));
    }
    Err(UninstallerError::Other(format!(
        "计划任务 {}{} 删除后仍可被系统查询到",
        task_path, task_name
    )))
}

fn run_native_command(program: &str, args: &[&str]) -> Result<(), UninstallerError> {
    let output = Command::new(program).args(args).output().map_err(|error| {
        UninstallerError::FileSystem(std::io::Error::other(format!(
            "执行 {program} 失败: {error}"
        )))
    })?;
    if output.status.success() {
        return Ok(());
    }
    Err(UninstallerError::Other(format!(
        "{} 返回失败状态: {}",
        program,
        decode_windows_output(&output.stderr).trim()
    )))
}

fn run_powershell(script: &str) -> Result<String, UninstallerError> {
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
            UninstallerError::FileSystem(std::io::Error::other(format!(
                "执行 PowerShell 失败: {error}"
            )))
        })?;
    if !output.status.success() {
        return Err(UninstallerError::Other(
            decode_windows_output(&output.stderr).trim().to_string(),
        ));
    }
    Ok(decode_windows_output(&output.stdout).trim().to_string())
}

fn parse_json_array<T>(json: &str, label: &str) -> Result<Vec<T>, UninstallerError>
where
    T: serde::de::DeserializeOwned,
{
    if json.trim().is_empty() || json.trim().eq_ignore_ascii_case("null") {
        return Ok(Vec::new());
    }
    let value: serde_json::Value = serde_json::from_str(json)
        .map_err(|error| UninstallerError::Serde(format!("解析{label} JSON 失败: {error}")))?;
    match value {
        serde_json::Value::Array(_) => serde_json::from_value(value)
            .map_err(|error| UninstallerError::Serde(format!("解析{label}列表失败: {error}"))),
        other => serde_json::from_value::<T>(other)
            .map(|item| vec![item])
            .map_err(|error| UninstallerError::Serde(format!("解析{label}单项失败: {error}"))),
    }
}

fn parse_optional_json<T>(json: &str, label: &str) -> Result<Option<T>, UninstallerError>
where
    T: serde::de::DeserializeOwned,
{
    if json.trim().is_empty() || json.trim().eq_ignore_ascii_case("null") {
        return Ok(None);
    }
    serde_json::from_str(json)
        .map(Some)
        .map_err(|error| UninstallerError::Serde(format!("解析{label} JSON 失败: {error}")))
}

fn related_root<'a>(command: &str, roots: &'a [String]) -> Option<&'a str> {
    roots
        .iter()
        .find(|root| command_references_related_path(command, root))
        .map(String::as_str)
}

/// 判断当前命令是否仍然引用预期安装目录，要求目录边界完整匹配。
pub fn command_references_related_path(command: &str, related_path: &str) -> bool {
    let command = normalize_command(command);
    let related_path = normalize_windows_path(&expand_environment_variables(related_path));
    if command.is_empty() || related_path.is_empty() {
        return false;
    }
    let related_path = related_path.trim_end_matches('\\');
    command.match_indices(related_path).any(|(start, _)| {
        let before_ok = start == 0
            || command[..start]
                .chars()
                .next_back()
                .is_some_and(is_command_boundary);
        let end = start + related_path.len();
        let after_ok = end == command.len()
            || command[end..]
                .chars()
                .next()
                .is_some_and(is_path_continuation_or_boundary);
        before_ok && after_ok
    })
}

fn is_command_boundary(character: char) -> bool {
    matches!(
        character,
        ' ' | '\t' | '"' | '\'' | '=' | '(' | '[' | ',' | ';'
    )
}

fn is_path_continuation_or_boundary(character: char) -> bool {
    matches!(
        character,
        '\\' | '/' | ' ' | '\t' | '"' | '\'' | ')' | ']' | ',' | ';'
    )
}

fn normalize_command(value: &str) -> String {
    normalize_windows_path(&expand_environment_variables(value))
}

fn normalize_windows_path(value: &str) -> String {
    let mut normalized = value.trim().replace('/', "\\").to_ascii_lowercase();
    while normalized.contains("\\\\") {
        normalized = normalized.replace("\\\\", "\\");
    }
    if let Some(stripped) = normalized.strip_prefix(r"\\?\") {
        normalized = stripped.to_string();
    }
    normalized.trim_end_matches('\\').to_string()
}

fn expand_environment_variables(value: &str) -> String {
    let mut expanded = value.to_string();
    let mut cursor = 0;
    while let Some(relative_start) = expanded[cursor..].find('%') {
        let start = cursor + relative_start;
        let Some(relative_end) = expanded[start + 1..].find('%') else {
            break;
        };
        let end = start + 1 + relative_end;
        let variable = &expanded[start + 1..end];
        let Some(replacement) = std::env::var_os(variable) else {
            cursor = end + 1;
            continue;
        };
        expanded.replace_range(start..=end, &replacement.to_string_lossy());
        cursor = start + replacement.len();
    }
    expanded
}

fn is_safe_related_root(path: &str) -> bool {
    let normalized = normalize_windows_path(path);
    if normalized.len() < 3
        || !normalized
            .as_bytes()
            .get(1)
            .is_some_and(|value| *value == b':')
    {
        return false;
    }
    if utils::is_system_critical_path(&normalized) {
        return false;
    }
    if is_windows_directory(&normalized) || is_broad_shared_root(&normalized) {
        return false;
    }
    !command_is_system_path(&normalized)
}

fn is_windows_directory(path: &str) -> bool {
    let normalized = normalize_windows_path(path);
    [
        std::env::var("WINDIR").ok(),
        std::env::var("SystemRoot").ok(),
    ]
    .into_iter()
    .flatten()
    .map(|root| normalize_windows_path(&root))
    .any(|root| normalized == root || normalized.starts_with(&(root + "\\")))
}

fn is_broad_shared_root(path: &str) -> bool {
    let normalized = normalize_windows_path(path);
    [
        r"\program files",
        r"\program files (x86)",
        r"\programdata",
        r"\users",
        r"\users\public",
        r"\appdata",
        r"\appdata\local",
        r"\appdata\locallow",
        r"\appdata\roaming",
        r"\common files",
    ]
    .iter()
    .any(|suffix| normalized.ends_with(suffix))
}

fn command_is_system_path(command: &str) -> bool {
    let normalized = normalize_command(command);
    [
        std::env::var("WINDIR").ok(),
        std::env::var("SystemRoot").ok(),
    ]
    .into_iter()
    .flatten()
    .any(|root| command_references_related_path(&normalized, &root))
}

fn is_protected_task_path(task_path: &str) -> bool {
    normalize_windows_path(task_path).starts_with(r"\microsoft\windows\")
}

fn combine_command(command: Option<&str>, arguments: Option<&str>) -> Option<String> {
    let command = command?.trim();
    if command.is_empty() {
        return None;
    }
    Some(
        match arguments.map(str::trim).filter(|value| !value.is_empty()) {
            Some(arguments) => format!("{command} {arguments}"),
            None => command.to_string(),
        },
    )
}

fn task_locator(task_path: &str, task_name: &str) -> Option<String> {
    if task_name.trim().is_empty() {
        return None;
    }
    let path = if task_path.trim().is_empty() {
        "\\".to_string()
    } else {
        let normalized = task_path.replace('/', "\\");
        if normalized.ends_with('\\') {
            normalized
        } else {
            format!("{normalized}\\")
        }
    };
    Some(format!("task://{path}{task_name}"))
}

fn parse_service_locator(locator: &str) -> Option<String> {
    let name = locator.strip_prefix("service://")?.trim();
    (!name.is_empty()).then_some(name.to_string())
}

fn parse_task_locator(locator: &str) -> Option<(String, String)> {
    let value = locator.strip_prefix("task://")?.replace('/', "\\");
    let position = value.rfind('\\')?;
    let task_path = if position == 0 {
        "\\".to_string()
    } else {
        value[..=position].to_string()
    };
    let task_name = value[position + 1..].to_string();
    if task_name.is_empty() {
        return None;
    }
    Some((task_path, task_name))
}

fn ps_escape(value: &str) -> String {
    value.replace('\'', "''")
}

#[cfg(test)]
mod tests {
    use super::{
        collect_driver_traces, collect_service_traces, collect_task_traces,
        command_references_related_path, is_broad_shared_root, is_protected_task_path,
        is_safe_related_root, parse_task_locator, DriverRecord, ServiceRecord, TaskRecord,
    };
    use crate::modules::lister::models::{InstallSource, InstalledProgram};

    #[test]
    fn command_matching_requires_a_complete_install_directory_boundary() {
        assert!(command_references_related_path(
            r#""C:\Program Files\Demo App\agent.exe" --run"#,
            r"C:\Program Files\Demo App"
        ));
        assert!(!command_references_related_path(
            r#"C:\Program Files\Demo App2\agent.exe"#,
            r"C:\Program Files\Demo App"
        ));
    }

    #[test]
    fn service_scan_requires_explicit_path_and_marks_shared_commands_critical() {
        let records = vec![
            ServiceRecord {
                name: "DemoA".to_string(),
                path_name: Some(r#""C:\Program Files\Demo App\agent.exe" --service"#.to_string()),
            },
            ServiceRecord {
                name: "DemoB".to_string(),
                path_name: Some(r#""C:\Program Files\Demo App\agent.exe" --service"#.to_string()),
            },
            ServiceRecord {
                name: "NameOnly".to_string(),
                path_name: Some("svchost.exe -k Demo".to_string()),
            },
        ];
        let traces = collect_service_traces(
            "Demo App",
            &[r"c:\program files\demo app".to_string()],
            &records,
        );

        assert_eq!(traces.len(), 2);
        assert!(traces.iter().all(|trace| trace.is_critical));
        assert!(traces.iter().all(|trace| trace.related_path.is_some()));
    }

    #[test]
    fn task_scan_keeps_system_tasks_protected_and_skips_name_only_matches() {
        let records = vec![
            TaskRecord {
                task_name: "DemoUpdater".to_string(),
                task_path: r"\Vendor\".to_string(),
                command: Some(r"C:\Program Files\Demo App\updater.exe".to_string()),
                arguments: None,
            },
            TaskRecord {
                task_name: "WindowsTask".to_string(),
                task_path: r"\Microsoft\Windows\UpdateOrchestrator\".to_string(),
                command: Some(r"C:\Program Files\Demo App\updater.exe".to_string()),
                arguments: None,
            },
            TaskRecord {
                task_name: "NameOnly".to_string(),
                task_path: r"\Vendor\".to_string(),
                command: Some("updater.exe".to_string()),
                arguments: None,
            },
        ];
        let traces = collect_task_traces(
            "Demo App",
            &[r"c:\program files\demo app".to_string()],
            &records,
        );

        assert_eq!(traces.len(), 2);
        assert!(!traces[0].is_critical);
        assert!(traces[1].is_critical);
        assert!(is_protected_task_path(r"\Microsoft\Windows\Defrag\"));
        assert_eq!(
            parse_task_locator("task://\\Vendor\\DemoUpdater"),
            Some((r"\Vendor\".to_string(), "DemoUpdater".to_string(),))
        );
    }

    #[test]
    fn driver_scan_is_evidence_only_and_shared_roots_are_rejected() {
        let traces = collect_driver_traces(
            "Demo App",
            &[r"c:\program files\demo app".to_string()],
            &[DriverRecord {
                name: "DemoDriver".to_string(),
                display_name: Some("Demo Driver".to_string()),
                path_name: Some(r#"C:\Program Files\Demo App\DemoDriver.sys"#.to_string()),
            }],
        );

        assert_eq!(traces.len(), 1);
        assert_eq!(traces[0].trace_type, super::TraceType::Driver);
        assert!(traces[0].is_critical);
        assert!(is_broad_shared_root(r"C:\Program Files"));
        assert!(!is_safe_related_root(r"C:\Program Files"));
    }

    #[test]
    #[ignore = "需要 Windows PowerShell，仅执行只读系统查询"]
    fn live_windows_queries_are_read_only_and_parseable() {
        let mut program = InstalledProgram::new("Rust Yu".to_string(), InstallSource::Registry);
        program.install_location = Some(r"C:\Program Files\Rust Yu".to_string());

        let traces = super::scan_traces(&program);

        assert!(traces.iter().all(|trace| trace.related_path.is_some()));
        assert!(traces
            .iter()
            .filter(|trace| trace.trace_type == super::TraceType::Driver)
            .all(|trace| trace.is_critical));
    }
}
