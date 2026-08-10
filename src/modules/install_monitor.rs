//! 安装监控：以受限范围保存安装前后快照，并把差异转换为可审查的卸载证据。
//!
//! 监控不是全盘监视器。范围只来自程序安装目录、程序名对应的用户数据目录、
//! 卸载注册表键、精确推导的程序注册表键和用户明确追加的路径。所有读取都在
//! Tauri 的后台任务中执行；遇到权限、链接、规模或注册表枚举错误时保留已读
//! 结果并写入 warnings，不把不完整快照伪装成无变化。

use crate::modules::common::error::UninstallerError;
use crate::modules::common::utils;
use crate::modules::lister::models::InstalledProgram;
use crate::modules::lister::storage;
use crate::modules::scanner::models::{Confidence, Trace, TraceType};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::hash::Hasher;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;
use sysinfo::System;
use uuid::Uuid;
use walkdir::WalkDir;
use winreg::enums::*;
use winreg::RegKey;

const MONITOR_DIR_NAME: &str = "install-monitor";
const MANIFEST_FILE_NAME: &str = "manifest.json";
const EXPORT_DIR_NAME: &str = "exports";
const MAX_FILE_ENTRIES: usize = 120_000;
const MAX_FILE_BYTES: u64 = 4 * 1024 * 1024 * 1024;
const MAX_HASH_BYTES: u64 = 64 * 1024 * 1024;
const MAX_REGISTRY_NODES: usize = 30_000;
const MAX_REGISTRY_DEPTH: usize = 64;
const MAX_REGISTRY_BYTES: u64 = 128 * 1024 * 1024;
const MAX_ROOTS: usize = 32;
static MONITOR_SESSION_WRITE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn session_write_guard() -> std::sync::MutexGuard<'static, ()> {
    MONITOR_SESSION_WRITE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MonitorChangeKind {
    Added,
    Removed,
    Modified,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MonitorItemKind {
    File,
    Directory,
    RegistryKey,
    RegistryValue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InstallMonitorStatus {
    Waiting,
    Completed,
    Failed,
    Cancelled,
    Expired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MonitorActivityKind {
    Install,
    #[default]
    Update,
    NormalRun,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MonitorEvidenceKind {
    Process,
    File,
    Registry,
    Service,
    ScheduledTask,
    Driver,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorEvidenceEvent {
    pub id: String,
    pub occurred_at: DateTime<Utc>,
    pub source: String,
    pub target: String,
    pub kind: MonitorEvidenceKind,
    pub operation: String,
    pub confidence: MonitorConfidence,
    pub parent_event_id: Option<String>,
    pub process_id: Option<u32>,
    pub parent_process_id: Option<u32>,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorProcessEventInput {
    pub executable: String,
    pub process_id: u32,
    pub parent_process_id: Option<u32>,
    pub parent_event_id: Option<String>,
    pub operation: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MonitorConfidence {
    High,
    Medium,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MonitorExportFormat {
    Json,
    Csv,
}

impl MonitorExportFormat {
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "json" => Some(Self::Json),
            "csv" => Some(Self::Csv),
            _ => None,
        }
    }

    fn extension(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Csv => "csv",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorRootInfo {
    pub path: String,
    pub source: String,
    pub enabled: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MonitorScope {
    pub file_roots: Vec<String>,
    pub registry_roots: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallMonitorPlan {
    pub program_id: String,
    pub program_name: String,
    pub scope: MonitorScope,
    pub file_roots: Vec<MonitorRootInfo>,
    pub registry_roots: Vec<MonitorRootInfo>,
    pub requires_admin: bool,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallMonitorStartRequest {
    pub program: InstalledProgram,
    #[serde(default)]
    pub extra_file_roots: Vec<String>,
    #[serde(default)]
    pub extra_registry_roots: Vec<String>,
    #[serde(default)]
    pub activity_kind: MonitorActivityKind,
    /// 最长会话时间；默认 24 小时，范围 5 分钟到 7 天。
    #[serde(default)]
    pub expires_after_minutes: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorFileRecord {
    pub root_path: String,
    pub relative_path: String,
    pub kind: MonitorItemKind,
    pub size: u64,
    pub modified_at: Option<i64>,
    /// 小文件保存内容指纹；大文件仅使用大小和修改时间，避免安装监控阻塞过久。
    pub content_hash: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorRegistryRecord {
    pub key_path: String,
    pub value_name: Option<String>,
    pub value_type: Option<u32>,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MonitorSnapshot {
    pub captured_at: DateTime<Utc>,
    pub files: Vec<MonitorFileRecord>,
    pub registry: Vec<MonitorRegistryRecord>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorSnapshotSummary {
    pub captured_at: DateTime<Utc>,
    pub file_count: usize,
    pub registry_count: usize,
    pub bytes: u64,
    pub warning_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorChange {
    pub id: String,
    pub kind: MonitorChangeKind,
    pub item_kind: MonitorItemKind,
    pub trace_type: TraceType,
    pub path: String,
    pub size_before: Option<u64>,
    pub size_after: Option<u64>,
    pub confidence: MonitorConfidence,
    pub description: String,
    pub evidence: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallMonitorSession {
    pub id: String,
    pub program: InstalledProgram,
    pub scope: MonitorScope,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub status: InstallMonitorStatus,
    #[serde(default)]
    pub activity_kind: MonitorActivityKind,
    #[serde(default)]
    pub expires_at: Option<DateTime<Utc>>,
    pub before: MonitorSnapshot,
    pub after: Option<MonitorSnapshot>,
    pub before_summary: MonitorSnapshotSummary,
    pub after_summary: Option<MonitorSnapshotSummary>,
    pub changes: Vec<MonitorChange>,
    #[serde(default)]
    pub evidence_events: Vec<MonitorEvidenceEvent>,
    #[serde(default)]
    pub system_before: Vec<Trace>,
    #[serde(default)]
    pub system_after: Vec<Trace>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallMonitorSessionInfo {
    pub id: String,
    pub program_id: String,
    pub program_name: String,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub status: InstallMonitorStatus,
    pub activity_kind: MonitorActivityKind,
    pub expires_at: Option<DateTime<Utc>>,
    pub changes_count: usize,
    pub added_count: usize,
    pub removed_count: usize,
    pub modified_count: usize,
    pub warning_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitorExport {
    pub path: String,
    pub format: MonitorExportFormat,
    pub changes_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RegistryHive {
    LocalMachine,
    CurrentUser,
    ClassesRoot,
    Users,
}

impl RegistryHive {
    fn root_key(self) -> RegKey {
        let hkey = match self {
            Self::LocalMachine => HKEY_LOCAL_MACHINE,
            Self::CurrentUser => HKEY_CURRENT_USER,
            Self::ClassesRoot => HKEY_CLASSES_ROOT,
            Self::Users => HKEY_USERS,
        };
        RegKey::predef(hkey)
    }

    fn prefix(self) -> &'static str {
        match self {
            Self::LocalMachine => "HKLM",
            Self::CurrentUser => "HKCU",
            Self::ClassesRoot => "HKCR",
            Self::Users => "HKU",
        }
    }
}

/// 根据已安装程序建立默认监控范围。追加范围仍会经过关键路径和根键校验。
pub fn build_plan(
    program: &InstalledProgram,
    extra_file_roots: &[String],
    extra_registry_roots: &[String],
) -> InstallMonitorPlan {
    let mut file_roots = Vec::new();
    let mut registry_roots = Vec::new();
    let mut warnings = Vec::new();

    if let Some(location) = program.install_location.as_deref() {
        add_file_root(&mut file_roots, location, "程序安装目录", &mut warnings);
    }

    if let Some(home) = dirs::home_dir() {
        for base in ["Roaming", "Local", "LocalLow"] {
            for segment in appdata_segments(program) {
                add_file_root(
                    &mut file_roots,
                    &home
                        .join("AppData")
                        .join(base)
                        .join(segment)
                        .to_string_lossy(),
                    &format!("AppData\\{base}"),
                    &mut warnings,
                );
            }
        }
    } else {
        warnings.push("无法定位当前用户 AppData，未加入默认用户数据范围".to_string());
    }

    for root in extra_file_roots {
        add_file_root(&mut file_roots, root, "用户追加目录", &mut warnings);
    }

    if let Some(path) = program.uninstall_registry_key_path.as_deref() {
        add_registry_root(&mut registry_roots, path, "卸载注册表键", &mut warnings);
    }
    for candidate in registry_candidates(program) {
        add_registry_root(
            &mut registry_roots,
            &candidate,
            "程序关联注册表键",
            &mut warnings,
        );
    }
    for root in extra_registry_roots {
        add_registry_root(&mut registry_roots, root, "用户追加注册表键", &mut warnings);
    }

    let enabled_file_roots = file_roots
        .iter()
        .filter(|root| root.enabled)
        .map(|root| root.path.clone())
        .collect();
    let enabled_registry_roots = registry_roots
        .iter()
        .filter(|root| root.enabled)
        .map(|root| root.path.clone())
        .collect();
    let requires_admin = file_roots
        .iter()
        .chain(registry_roots.iter())
        .filter(|root| root.enabled)
        .any(|root| root_requires_admin(&root.path));

    InstallMonitorPlan {
        program_id: program.id.clone(),
        program_name: program.name.clone(),
        scope: MonitorScope {
            file_roots: enabled_file_roots,
            registry_roots: enabled_registry_roots,
        },
        file_roots,
        registry_roots,
        requires_admin,
        warnings,
    }
}

/// 开始监控并保存安装前快照。没有管理员权限时仍读取可访问范围，并明确记录限制。
pub fn start_monitor(
    request: InstallMonitorStartRequest,
) -> Result<InstallMonitorSessionInfo, UninstallerError> {
    let plan = build_plan(
        &request.program,
        &request.extra_file_roots,
        &request.extra_registry_roots,
    );
    let before = capture_scope(&plan.scope)?;
    let id = Uuid::new_v4().to_string();
    let mut warnings = plan.warnings;
    warnings.extend(before.warnings.iter().cloned());
    if plan.requires_admin {
        warnings
            .push("范围包含系统级目录或注册表；未提权时只能记录当前账户可访问的项目".to_string());
    }
    let created_at = Utc::now();
    let expires_after = request
        .expires_after_minutes
        .unwrap_or(24 * 60)
        .clamp(5, 7 * 24 * 60);
    let system_before = crate::modules::system_integration::scan_traces(&request.program);
    let session = InstallMonitorSession {
        id,
        program: request.program,
        scope: plan.scope,
        created_at,
        completed_at: None,
        status: InstallMonitorStatus::Waiting,
        activity_kind: request.activity_kind,
        expires_at: Some(created_at + Duration::minutes(i64::from(expires_after))),
        before_summary: snapshot_summary(&before),
        after_summary: None,
        before,
        after: None,
        changes: Vec::new(),
        evidence_events: Vec::new(),
        system_before,
        system_after: Vec::new(),
        warnings,
    };
    write_session(&session)?;
    Ok(session_info(&session))
}

/// 捕获安装后快照并生成差异；会话完成后仍可多次读取和导出，不重复改写差异。
pub fn complete_monitor(session_id: &str) -> Result<InstallMonitorSession, UninstallerError> {
    let _guard = session_write_guard();
    let mut session = load_session(session_id)?;
    if session.status != InstallMonitorStatus::Waiting {
        return Err(UninstallerError::Other(
            "该安装监控会话已经完成".to_string(),
        ));
    }
    if session
        .expires_at
        .is_some_and(|expires_at| expires_at < Utc::now())
    {
        session.status = InstallMonitorStatus::Expired;
        session.completed_at = Some(Utc::now());
        session
            .warnings
            .push("会话超过有效期，未采集结束快照。".to_string());
        write_session(&session)?;
        return Err(UninstallerError::Other("安装监控会话已过期。".to_string()));
    }
    let after = capture_scope(&session.scope)?;
    session.changes = diff_snapshots(&session.program, &session.before, &after);
    session.system_after = crate::modules::system_integration::scan_traces(&session.program);
    session
        .evidence_events
        .extend(change_events(&session.changes, after.captured_at));
    session.evidence_events.extend(system_change_events(
        &session.system_before,
        &session.system_after,
        after.captured_at,
    ));
    session.warnings.extend(after.warnings.iter().cloned());
    session.after_summary = Some(snapshot_summary(&after));
    session.after = Some(after);
    session.completed_at = Some(Utc::now());
    session.status = InstallMonitorStatus::Completed;
    write_session(&session)?;
    Ok(session)
}

/// 停止等待中的会话。取消只更新清单，不采集结束快照。
pub fn cancel_monitor(session_id: &str) -> Result<InstallMonitorSession, UninstallerError> {
    let _guard = session_write_guard();
    let mut session = load_session(session_id)?;
    if session.status != InstallMonitorStatus::Waiting {
        return Err(UninstallerError::Other(
            "只有等待中的监控会话可以停止。".to_string(),
        ));
    }
    session.status = InstallMonitorStatus::Cancelled;
    session.completed_at = Some(Utc::now());
    session
        .warnings
        .push("会话由用户停止，未生成结束差异。".to_string());
    write_session(&session)?;
    Ok(session)
}

/// 删除已结束的会话；等待中的会话必须先停止，避免误删正在使用的基线。
pub fn delete_monitor(session_id: &str) -> Result<bool, UninstallerError> {
    let _guard = session_write_guard();
    let session = load_session(session_id)?;
    if session.status == InstallMonitorStatus::Waiting {
        return Err(UninstallerError::Other(
            "请先停止监控会话再删除。".to_string(),
        ));
    }
    let directory = session_directory(session_id)?;
    let metadata = fs::symlink_metadata(&directory)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(UninstallerError::PermissionDenied(
            "监控会话目录不安全。".to_string(),
        ));
    }
    fs::remove_dir_all(directory)?;
    Ok(true)
}

pub fn expire_monitor_sessions(now: DateTime<Utc>) -> Result<usize, UninstallerError> {
    let _guard = session_write_guard();
    let mut expired = 0usize;
    for info in list_sessions()? {
        if info.status == InstallMonitorStatus::Waiting
            && info.expires_at.is_some_and(|value| value <= now)
        {
            let mut session = load_session(&info.id)?;
            session.status = InstallMonitorStatus::Expired;
            session.completed_at = Some(now);
            session
                .warnings
                .push("会话达到有效期，已自动停止。".to_string());
            write_session(&session)?;
            expired += 1;
        }
    }
    Ok(expired)
}

pub fn expire_monitor_sessions_now() -> Result<usize, UninstallerError> {
    expire_monitor_sessions(Utc::now())
}

pub fn record_process_event(
    session_id: &str,
    input: MonitorProcessEventInput,
) -> Result<MonitorEvidenceEvent, UninstallerError> {
    let _guard = session_write_guard();
    let mut session = load_session(session_id)?;
    if session.status != InstallMonitorStatus::Waiting {
        return Err(UninstallerError::Other(
            "只有等待中的会话可以追加进程事件。".to_string(),
        ));
    }
    if input.executable.trim().is_empty() || input.operation.trim().is_empty() {
        return Err(UninstallerError::Other(
            "进程事件缺少目标或操作。".to_string(),
        ));
    }
    let event = MonitorEvidenceEvent {
        id: Uuid::new_v4().to_string(),
        occurred_at: Utc::now(),
        source: "process_observer".to_string(),
        target: input.executable,
        kind: MonitorEvidenceKind::Process,
        operation: input.operation,
        confidence: MonitorConfidence::High,
        parent_event_id: input.parent_event_id,
        process_id: Some(input.process_id),
        parent_process_id: input.parent_process_id,
        note: "由安装会话进程观察器记录；父子关系保留原始 PID。".to_string(),
    };
    session.evidence_events.push(event.clone());
    write_session(&session)?;
    Ok(event)
}

/// 对等待中的会话执行一次只读进程采样。Tauri 层周期调用本函数，确保短生命周期
/// 安装器也能留下 PID/父 PID；启发式安装器只记为中置信度，不自动转为删除证据。
pub fn observe_monitor_processes(session_id: &str) -> Result<bool, UninstallerError> {
    let _guard = session_write_guard();
    let mut session = load_session(session_id)?;
    if session.status != InstallMonitorStatus::Waiting {
        return Ok(false);
    }
    if session.expires_at.is_some_and(|value| value <= Utc::now()) {
        session.status = InstallMonitorStatus::Expired;
        session.completed_at = Some(Utc::now());
        session
            .warnings
            .push("会话达到有效期，进程观察已停止。".to_string());
        write_session(&session)?;
        return Ok(false);
    }
    let known = session
        .evidence_events
        .iter()
        .filter_map(|event| event.process_id)
        .collect::<HashSet<_>>();
    let parent_events = session
        .evidence_events
        .iter()
        .filter_map(|event| event.process_id.map(|pid| (pid, event.id.clone())))
        .collect::<HashMap<_, _>>();
    let created_epoch = session.created_at.timestamp().max(0) as u64;
    let roots = session
        .scope
        .file_roots
        .iter()
        .map(|root| root.replace('/', "\\").to_ascii_lowercase())
        .collect::<Vec<_>>();
    let program_terms = [
        session.program.name.as_str(),
        session.program.publisher.as_deref().unwrap_or_default(),
    ]
    .into_iter()
    .map(|value| value.to_ascii_lowercase())
    .filter(|value| value.len() >= 3)
    .collect::<Vec<_>>();
    let mut system = System::new_all();
    system.refresh_all();
    let observed_at = Utc::now();
    for (pid, process) in system.processes() {
        let pid = pid.as_u32();
        if known.contains(&pid) || process.start_time() + 2 < created_epoch {
            continue;
        }
        let executable = process
            .exe()
            .map(|path| path.to_string_lossy().to_string())
            .unwrap_or_else(|| process.name().to_string_lossy().to_string());
        let command = process
            .cmd()
            .iter()
            .map(|part| part.to_string_lossy())
            .collect::<Vec<_>>()
            .join(" ");
        let haystack = format!("{} {}", executable, command).to_ascii_lowercase();
        let exact_root = roots
            .iter()
            .any(|root| haystack.starts_with(root) || haystack.contains(&format!("\"{root}")));
        let installer_keyword = ["setup", "install", "update", "msiexec", "unins"]
            .iter()
            .any(|term| haystack.contains(term));
        let program_match = program_terms.iter().any(|term| haystack.contains(term));
        if !exact_root && !(installer_keyword && program_match) {
            continue;
        }
        let parent_pid = process.parent().map(|value| value.as_u32());
        session.evidence_events.push(MonitorEvidenceEvent {
            id: Uuid::new_v4().to_string(),
            occurred_at: observed_at,
            source: "periodic_process_observer".to_string(),
            target: executable,
            kind: MonitorEvidenceKind::Process,
            operation: "observed".to_string(),
            confidence: if exact_root {
                MonitorConfidence::High
            } else {
                MonitorConfidence::Medium
            },
            parent_event_id: parent_pid.and_then(|value| parent_events.get(&value).cloned()),
            process_id: Some(pid),
            parent_process_id: parent_pid,
            note: if exact_root {
                "进程命令位于监控根目录。"
            } else {
                "安装器关键词与程序标识同时匹配；仅作为中置信度时间线证据。"
            }
            .to_string(),
        });
    }
    write_session(&session)?;
    Ok(true)
}

pub fn list_sessions() -> Result<Vec<InstallMonitorSessionInfo>, UninstallerError> {
    let root = monitor_root()?;
    if !root.exists() {
        return Ok(Vec::new());
    }
    let mut sessions = Vec::new();
    for entry in fs::read_dir(root)? {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                tracing::warn!("读取安装监控目录项失败: {error}");
                continue;
            }
        };
        let directory_metadata = match fs::symlink_metadata(entry.path()) {
            Ok(metadata) => metadata,
            Err(error) => {
                tracing::warn!("读取安装监控目录项元数据失败: {error}");
                continue;
            }
        };
        if directory_metadata.file_type().is_symlink() || !directory_metadata.is_dir() {
            continue;
        }
        let manifest = entry.path().join(MANIFEST_FILE_NAME);
        let manifest_metadata = match fs::symlink_metadata(&manifest) {
            Ok(metadata) => metadata,
            Err(error) => {
                tracing::warn!("读取安装监控清单元数据失败: {error}");
                continue;
            }
        };
        if manifest_metadata.file_type().is_symlink() || !manifest_metadata.is_file() {
            continue;
        }
        match read_json_file::<InstallMonitorSession>(&manifest) {
            Ok(session) => sessions.push(session_info(&session)),
            Err(error) => tracing::warn!("解析安装监控清单失败: {error}"),
        }
    }
    sessions.sort_by(|left, right| right.created_at.cmp(&left.created_at));
    Ok(sessions)
}

pub fn get_session(session_id: &str) -> Result<InstallMonitorSession, UninstallerError> {
    load_session(session_id)
}

/// 将安装监控新增或修改的项目转换为标准卸载残留证据；删除项不纳入，避免误删用户数据。
pub fn traces_for_session(session_id: &str) -> Result<Vec<Trace>, UninstallerError> {
    let session = load_session(session_id)?;
    Ok(session_to_traces(&session))
}

pub fn export_session(
    session_id: &str,
    format: MonitorExportFormat,
) -> Result<MonitorExport, UninstallerError> {
    let session = load_session(session_id)?;
    let export_root = storage::get_storage_root_dir()?.join(EXPORT_DIR_NAME);
    fs::create_dir_all(&export_root)?;
    let path = export_root.join(format!(
        "install-monitor-{}.{}",
        session.id,
        format.extension()
    ));
    let content = match format {
        MonitorExportFormat::Json => serde_json::to_vec_pretty(&ExportDocument::from(&session))
            .map_err(|error| UninstallerError::Serde(error.to_string()))?,
        MonitorExportFormat::Csv => csv_export(&session).into_bytes(),
    };
    fs::write(&path, content)?;
    Ok(MonitorExport {
        path: path.to_string_lossy().into_owned(),
        format,
        changes_count: session.changes.len(),
    })
}

fn capture_scope(scope: &MonitorScope) -> Result<MonitorSnapshot, UninstallerError> {
    let mut snapshot = MonitorSnapshot {
        captured_at: Utc::now(),
        ..MonitorSnapshot::default()
    };
    let mut total_bytes = 0u64;
    for root in &scope.file_roots {
        capture_file_root(
            root,
            &mut snapshot.files,
            &mut snapshot.warnings,
            &mut total_bytes,
        )?;
    }
    let mut registry_nodes = 0usize;
    let mut registry_bytes = 0u64;
    for root in &scope.registry_roots {
        if let Err(error) = capture_registry_root(
            root,
            &mut snapshot.registry,
            &mut snapshot.warnings,
            &mut registry_nodes,
            &mut registry_bytes,
        ) {
            snapshot
                .warnings
                .push(format!("注册表范围 {root} 读取失败: {error}"));
        }
    }
    Ok(snapshot)
}

fn capture_file_root(
    root: &str,
    records: &mut Vec<MonitorFileRecord>,
    warnings: &mut Vec<String>,
    total_bytes: &mut u64,
) -> Result<(), UninstallerError> {
    let path = Path::new(root);
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            warnings.push(format!("文件范围暂不存在，安装后仍会尝试读取: {root}"));
            return Ok(());
        }
        Err(error) => return Err(UninstallerError::FileSystem(error)),
    };
    if metadata.file_type().is_symlink() {
        warnings.push(format!("跳过符号链接文件范围: {root}"));
        return Ok(());
    }

    for entry in WalkDir::new(path).follow_links(false) {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                warnings.push(format!("遍历文件范围 {root} 时跳过项目: {error}"));
                continue;
            }
        };
        let entry_metadata = match fs::symlink_metadata(entry.path()) {
            Ok(metadata) => metadata,
            Err(error) => {
                warnings.push(format!(
                    "读取文件项目 {} 失败: {error}",
                    entry.path().display()
                ));
                continue;
            }
        };
        if entry_metadata.file_type().is_symlink() {
            warnings.push(format!(
                "跳过文件范围内的符号链接: {}",
                entry.path().display()
            ));
            continue;
        }
        if records.len() >= MAX_FILE_ENTRIES {
            warnings.push("文件监控项目数达到安全上限，后续项目未读取".to_string());
            break;
        }
        let relative = match entry.path().strip_prefix(path) {
            Ok(relative) if relative.as_os_str().is_empty() => ".".to_string(),
            Ok(relative) => relative.to_string_lossy().replace('/', "\\"),
            Err(error) => {
                warnings.push(format!("计算监控相对路径失败: {error}"));
                continue;
            }
        };
        let kind = if entry_metadata.is_dir() {
            MonitorItemKind::Directory
        } else if entry_metadata.is_file() {
            MonitorItemKind::File
        } else {
            continue;
        };
        let size = if entry_metadata.is_file() {
            let size = entry_metadata.len();
            let total = total_bytes.saturating_add(size);
            if total > MAX_FILE_BYTES {
                warnings.push("文件监控数据量达到安全上限，后续项目未读取".to_string());
                break;
            }
            *total_bytes = total;
            size
        } else {
            0
        };
        let content_hash = if kind == MonitorItemKind::File && size <= MAX_HASH_BYTES {
            match hash_file(entry.path()) {
                Ok(hash) => Some(hash),
                Err(error) => {
                    warnings.push(format!(
                        "读取文件内容指纹失败 {}: {error}",
                        entry.path().display()
                    ));
                    None
                }
            }
        } else {
            None
        };
        records.push(MonitorFileRecord {
            root_path: root.to_string(),
            relative_path: relative,
            kind,
            size,
            modified_at: modified_at(&entry_metadata),
            content_hash,
        });
    }
    Ok(())
}

fn hash_file(path: &Path) -> Result<u64, UninstallerError> {
    let mut file = File::open(path)?;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    let mut buffer = [0_u8; 32 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.write(&buffer[..read]);
    }
    Ok(hasher.finish())
}

fn modified_at(metadata: &fs::Metadata) -> Option<i64> {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .and_then(|duration| i64::try_from(duration.as_millis()).ok())
}

fn capture_registry_root(
    path: &str,
    records: &mut Vec<MonitorRegistryRecord>,
    warnings: &mut Vec<String>,
    nodes: &mut usize,
    bytes: &mut u64,
) -> Result<(), UninstallerError> {
    let (hive, key_path) = parse_registry_key_path(path)?;
    let key = match hive.root_key().open_subkey_with_flags(&key_path, KEY_READ) {
        Ok(key) => key,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            warnings.push(format!("注册表范围暂不存在，安装后仍会尝试读取: {path}"));
            return Ok(());
        }
        Err(error) => return Err(UninstallerError::Registry(error.to_string())),
    };
    capture_registry_node(
        &key,
        &format!("{}\\{}", hive.prefix(), key_path),
        records,
        warnings,
        nodes,
        bytes,
        0,
    )
}

fn capture_registry_node(
    key: &RegKey,
    key_path: &str,
    records: &mut Vec<MonitorRegistryRecord>,
    warnings: &mut Vec<String>,
    nodes: &mut usize,
    bytes: &mut u64,
    depth: usize,
) -> Result<(), UninstallerError> {
    if depth > MAX_REGISTRY_DEPTH || *nodes >= MAX_REGISTRY_NODES {
        warnings.push("注册表监控范围达到安全上限，后续项目未读取".to_string());
        return Ok(());
    }
    *nodes += 1;
    records.push(MonitorRegistryRecord {
        key_path: key_path.to_string(),
        value_name: None,
        value_type: None,
        bytes: Vec::new(),
    });

    for result in key.enum_values() {
        let (value_name, value) =
            result.map_err(|error| UninstallerError::Registry(error.to_string()))?;
        let value_bytes = value.bytes.clone();
        let total = bytes.saturating_add(value_bytes.len() as u64);
        if total > MAX_REGISTRY_BYTES {
            warnings.push("注册表监控数据量达到安全上限，后续值未读取".to_string());
            break;
        }
        *bytes = total;
        records.push(MonitorRegistryRecord {
            key_path: key_path.to_string(),
            value_name: Some(value_name),
            value_type: Some(value.vtype.clone() as isize as u32),
            bytes: value_bytes,
        });
    }

    for result in key.enum_keys() {
        let child_name = result.map_err(|error| UninstallerError::Registry(error.to_string()))?;
        let child = key
            .open_subkey_with_flags(&child_name, KEY_READ)
            .map_err(|error| UninstallerError::Registry(error.to_string()))?;
        capture_registry_node(
            &child,
            &format!("{key_path}\\{child_name}"),
            records,
            warnings,
            nodes,
            bytes,
            depth + 1,
        )?;
    }
    Ok(())
}

fn diff_snapshots(
    program: &InstalledProgram,
    before: &MonitorSnapshot,
    after: &MonitorSnapshot,
) -> Vec<MonitorChange> {
    let mut changes = Vec::new();
    let before_files = before
        .files
        .iter()
        .map(|record| (file_identity(record), record))
        .collect::<HashMap<_, _>>();
    let after_files = after
        .files
        .iter()
        .map(|record| (file_identity(record), record))
        .collect::<HashMap<_, _>>();

    for (identity, record) in &after_files {
        if !before_files.contains_key(identity) {
            changes.push(file_change(
                program,
                MonitorChangeKind::Added,
                None,
                Some(record),
            ));
        } else if let Some(previous) = before_files.get(identity) {
            if record.kind == MonitorItemKind::File
                && previous.kind == MonitorItemKind::File
                && !file_records_equal(previous, record)
            {
                changes.push(file_change(
                    program,
                    MonitorChangeKind::Modified,
                    Some(previous),
                    Some(record),
                ));
            }
        }
    }
    for (identity, record) in &before_files {
        if !after_files.contains_key(identity) {
            changes.push(file_change(
                program,
                MonitorChangeKind::Removed,
                Some(record),
                None,
            ));
        }
    }

    let before_registry = before
        .registry
        .iter()
        .map(|record| (registry_identity(record), record))
        .collect::<HashMap<_, _>>();
    let after_registry = after
        .registry
        .iter()
        .map(|record| (registry_identity(record), record))
        .collect::<HashMap<_, _>>();
    for (identity, record) in &after_registry {
        if !before_registry.contains_key(identity) {
            changes.push(registry_change(
                program,
                MonitorChangeKind::Added,
                None,
                Some(record),
            ));
        } else if let Some(previous) = before_registry.get(identity) {
            if previous.bytes != record.bytes || previous.value_type != record.value_type {
                changes.push(registry_change(
                    program,
                    MonitorChangeKind::Modified,
                    Some(previous),
                    Some(record),
                ));
            }
        }
    }
    for (identity, record) in &before_registry {
        if !after_registry.contains_key(identity) {
            changes.push(registry_change(
                program,
                MonitorChangeKind::Removed,
                Some(record),
                None,
            ));
        }
    }
    changes.sort_by(|left, right| left.path.to_lowercase().cmp(&right.path.to_lowercase()));
    changes
}

fn change_events(
    changes: &[MonitorChange],
    occurred_at: DateTime<Utc>,
) -> Vec<MonitorEvidenceEvent> {
    changes
        .iter()
        .map(|change| MonitorEvidenceEvent {
            id: Uuid::new_v4().to_string(),
            occurred_at,
            source: "before_after_snapshot".to_string(),
            target: change.path.clone(),
            kind: if matches!(
                change.item_kind,
                MonitorItemKind::RegistryKey | MonitorItemKind::RegistryValue
            ) {
                MonitorEvidenceKind::Registry
            } else {
                MonitorEvidenceKind::File
            },
            operation: match change.kind {
                MonitorChangeKind::Added => "added",
                MonitorChangeKind::Removed => "removed",
                MonitorChangeKind::Modified => "modified",
            }
            .to_string(),
            confidence: change.confidence,
            parent_event_id: None,
            process_id: None,
            parent_process_id: None,
            note: change.evidence.clone(),
        })
        .collect()
}

fn system_change_events(
    before: &[Trace],
    after: &[Trace],
    occurred_at: DateTime<Utc>,
) -> Vec<MonitorEvidenceEvent> {
    let before_keys = before.iter().map(system_identity).collect::<HashSet<_>>();
    let after_keys = after.iter().map(system_identity).collect::<HashSet<_>>();
    let mut events = Vec::new();
    for trace in after {
        if !before_keys.contains(&system_identity(trace)) {
            events.push(system_event(trace, "added", occurred_at));
        }
    }
    for trace in before {
        if !after_keys.contains(&system_identity(trace)) {
            events.push(system_event(trace, "removed", occurred_at));
        }
    }
    events
}

fn system_identity(trace: &Trace) -> String {
    format!("{}|{}", trace.trace_type, trace.path.to_ascii_lowercase())
}

fn system_event(
    trace: &Trace,
    operation: &str,
    occurred_at: DateTime<Utc>,
) -> MonitorEvidenceEvent {
    let kind = match trace.trace_type {
        TraceType::Service => MonitorEvidenceKind::Service,
        TraceType::ScheduledTask => MonitorEvidenceKind::ScheduledTask,
        TraceType::Driver => MonitorEvidenceKind::Driver,
        _ => MonitorEvidenceKind::File,
    };
    MonitorEvidenceEvent {
        id: Uuid::new_v4().to_string(),
        occurred_at,
        source: "system_integration_snapshot".to_string(),
        target: trace.path.clone(),
        kind,
        operation: operation.to_string(),
        confidence: match trace.confidence {
            Confidence::High => MonitorConfidence::High,
            Confidence::Medium | Confidence::Low => MonitorConfidence::Medium,
        },
        parent_event_id: None,
        process_id: None,
        parent_process_id: None,
        note: trace.description.clone(),
    }
}

fn file_identity(record: &MonitorFileRecord) -> String {
    format!(
        "file|{}|{}",
        record.root_path.to_lowercase(),
        record.relative_path.to_lowercase()
    )
}

fn registry_identity(record: &MonitorRegistryRecord) -> String {
    format!(
        "registry|{}|{}",
        record.key_path.to_lowercase(),
        record
            .value_name
            .as_deref()
            .unwrap_or("<key>")
            .to_lowercase()
    )
}

fn file_records_equal(left: &MonitorFileRecord, right: &MonitorFileRecord) -> bool {
    if left.kind != right.kind || left.size != right.size {
        return false;
    }
    match (left.content_hash, right.content_hash) {
        (Some(left), Some(right)) => left == right,
        _ => left.modified_at == right.modified_at,
    }
}

fn file_change(
    program: &InstalledProgram,
    kind: MonitorChangeKind,
    before: Option<&MonitorFileRecord>,
    after: Option<&MonitorFileRecord>,
) -> MonitorChange {
    let record = after.or(before);
    let Some(record) = record else {
        return MonitorChange {
            id: Uuid::new_v4().to_string(),
            kind,
            item_kind: MonitorItemKind::File,
            trace_type: TraceType::File,
            path: String::new(),
            size_before: None,
            size_after: None,
            confidence: MonitorConfidence::Medium,
            description: "无法确定的文件变化".to_string(),
            evidence: "快照记录缺少路径".to_string(),
        };
    };
    let path = file_record_path(record);
    let item_kind = record.kind;
    let trace_type = if path.to_ascii_lowercase().contains("\\appdata\\") {
        TraceType::AppData
    } else if path.to_ascii_lowercase().ends_with(".lnk") {
        TraceType::Shortcut
    } else {
        TraceType::File
    };
    let description = match kind {
        MonitorChangeKind::Added => "安装后新增文件项目",
        MonitorChangeKind::Removed => "安装后消失的原有文件项目",
        MonitorChangeKind::Modified => "安装后修改的文件项目",
    };
    MonitorChange {
        id: Uuid::new_v4().to_string(),
        kind,
        item_kind,
        trace_type,
        path,
        size_before: before.map(|item| item.size),
        size_after: after.map(|item| item.size),
        confidence: if program.install_location.is_some() {
            MonitorConfidence::High
        } else {
            MonitorConfidence::Medium
        },
        description: description.to_string(),
        evidence: match kind {
            MonitorChangeKind::Added => "安装后首次出现在受限监控范围".to_string(),
            MonitorChangeKind::Removed => "安装前存在、安装后不再存在".to_string(),
            MonitorChangeKind::Modified => "大小、修改时间或内容指纹发生变化".to_string(),
        },
    }
}

fn registry_change(
    _program: &InstalledProgram,
    kind: MonitorChangeKind,
    before: Option<&MonitorRegistryRecord>,
    after: Option<&MonitorRegistryRecord>,
) -> MonitorChange {
    let record = after.or(before);
    let Some(record) = record else {
        return MonitorChange {
            id: Uuid::new_v4().to_string(),
            kind,
            item_kind: MonitorItemKind::RegistryValue,
            trace_type: TraceType::RegistryValue,
            path: String::new(),
            size_before: None,
            size_after: None,
            confidence: MonitorConfidence::Medium,
            description: "无法确定的注册表变化".to_string(),
            evidence: "快照记录缺少路径".to_string(),
        };
    };
    let item_kind = if record.value_name.is_some() {
        MonitorItemKind::RegistryValue
    } else {
        MonitorItemKind::RegistryKey
    };
    let path = match record.value_name.as_deref() {
        Some(value_name) if !value_name.is_empty() => format!("{}\\{value_name}", record.key_path),
        Some(_) => format!("{}\\(默认)", record.key_path),
        None => record.key_path.clone(),
    };
    MonitorChange {
        id: Uuid::new_v4().to_string(),
        kind,
        item_kind,
        trace_type: if item_kind == MonitorItemKind::RegistryKey {
            TraceType::RegistryKey
        } else {
            TraceType::RegistryValue
        },
        path,
        size_before: before.map(|item| item.bytes.len() as u64),
        size_after: after.map(|item| item.bytes.len() as u64),
        confidence: MonitorConfidence::High,
        description: match kind {
            MonitorChangeKind::Added => "安装后新增注册表项目".to_string(),
            MonitorChangeKind::Removed => "安装后消失的原有注册表项目".to_string(),
            MonitorChangeKind::Modified => "安装后修改的注册表值".to_string(),
        },
        evidence: match kind {
            MonitorChangeKind::Added => "安装后首次出现在受限注册表范围".to_string(),
            MonitorChangeKind::Removed => "安装前存在、安装后不再存在".to_string(),
            MonitorChangeKind::Modified => "原始类型或原始字节发生变化".to_string(),
        },
    }
}

fn session_to_traces(session: &InstallMonitorSession) -> Vec<Trace> {
    session
        .changes
        .iter()
        .filter(|change| change.kind != MonitorChangeKind::Removed)
        .map(|change| {
            let mut trace = Trace::new(
                session.program.name.clone(),
                change.trace_type,
                change.path.clone(),
            )
            .with_description(format!("{}；{}", change.description, change.evidence))
            .with_confidence(match change.confidence {
                MonitorConfidence::High => Confidence::High,
                MonitorConfidence::Medium => Confidence::Medium,
            });
            if let Some(size) = change.size_after {
                trace = trace.with_size(size);
            }
            trace
        })
        .collect()
}

fn file_record_path(record: &MonitorFileRecord) -> String {
    if record.relative_path == "." {
        record.root_path.clone()
    } else {
        Path::new(&record.root_path)
            .join(&record.relative_path)
            .to_string_lossy()
            .into_owned()
    }
}

fn snapshot_summary(snapshot: &MonitorSnapshot) -> MonitorSnapshotSummary {
    MonitorSnapshotSummary {
        captured_at: snapshot.captured_at,
        file_count: snapshot.files.len(),
        registry_count: snapshot.registry.len(),
        bytes: snapshot
            .files
            .iter()
            .map(|record| record.size)
            .chain(
                snapshot
                    .registry
                    .iter()
                    .map(|record| record.bytes.len() as u64),
            )
            .sum(),
        warning_count: snapshot.warnings.len(),
    }
}

fn session_info(session: &InstallMonitorSession) -> InstallMonitorSessionInfo {
    InstallMonitorSessionInfo {
        id: session.id.clone(),
        program_id: session.program.id.clone(),
        program_name: session.program.name.clone(),
        created_at: session.created_at,
        completed_at: session.completed_at,
        status: session.status,
        activity_kind: session.activity_kind,
        expires_at: session.expires_at,
        changes_count: session.changes.len(),
        added_count: session
            .changes
            .iter()
            .filter(|change| change.kind == MonitorChangeKind::Added)
            .count(),
        removed_count: session
            .changes
            .iter()
            .filter(|change| change.kind == MonitorChangeKind::Removed)
            .count(),
        modified_count: session
            .changes
            .iter()
            .filter(|change| change.kind == MonitorChangeKind::Modified)
            .count(),
        warning_count: session.warnings.len(),
    }
}

#[derive(Debug, Serialize)]
struct ExportDocument<'a> {
    id: &'a str,
    program_name: &'a str,
    created_at: DateTime<Utc>,
    completed_at: Option<DateTime<Utc>>,
    status: InstallMonitorStatus,
    activity_kind: MonitorActivityKind,
    expires_at: Option<DateTime<Utc>>,
    changes: &'a [MonitorChange],
    evidence_events: &'a [MonitorEvidenceEvent],
    warnings: &'a [String],
}

impl<'a> From<&'a InstallMonitorSession> for ExportDocument<'a> {
    fn from(session: &'a InstallMonitorSession) -> Self {
        Self {
            id: &session.id,
            program_name: &session.program.name,
            created_at: session.created_at,
            completed_at: session.completed_at,
            status: session.status,
            activity_kind: session.activity_kind,
            expires_at: session.expires_at,
            changes: &session.changes,
            evidence_events: &session.evidence_events,
            warnings: &session.warnings,
        }
    }
}

fn csv_export(session: &InstallMonitorSession) -> String {
    let mut csv = String::from(
        "change_kind,item_kind,trace_type,path,size_before,size_after,confidence,description,evidence\n",
    );
    for change in &session.changes {
        let row = [
            format!("{:?}", change.kind).to_ascii_lowercase(),
            format!("{:?}", change.item_kind).to_ascii_lowercase(),
            change.trace_type.to_string(),
            change.path.clone(),
            change
                .size_before
                .map(|value| value.to_string())
                .unwrap_or_default(),
            change
                .size_after
                .map(|value| value.to_string())
                .unwrap_or_default(),
            format!("{:?}", change.confidence).to_ascii_lowercase(),
            change.description.clone(),
            change.evidence.clone(),
        ];
        csv.push_str(
            &row.into_iter()
                .map(|field| csv_escape(&field))
                .collect::<Vec<_>>()
                .join(","),
        );
        csv.push('\n');
    }
    csv
}

fn csv_escape(value: &str) -> String {
    if value.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", value.replace('"', "\"\""))
    } else {
        value.to_string()
    }
}

fn appdata_segments(program: &InstalledProgram) -> Vec<String> {
    let mut segments = Vec::new();
    for value in [
        program.name.as_str(),
        program.publisher.as_deref().unwrap_or_default(),
    ] {
        let value = value.trim();
        if !value.is_empty() && is_safe_segment(value) {
            segments.push(value.to_string());
            let compact = value
                .chars()
                .filter(|character| character.is_ascii_alphanumeric())
                .collect::<String>();
            if compact.len() >= 8 && compact != value {
                segments.push(compact);
            }
        }
    }
    deduplicate_strings(segments)
}

fn registry_candidates(program: &InstalledProgram) -> Vec<String> {
    let name = program.name.trim();
    if !is_safe_segment(name) {
        return Vec::new();
    }
    let mut segments = vec![name.to_string()];
    if let Some(publisher) = program.publisher.as_deref().map(str::trim) {
        if is_safe_segment(publisher) {
            segments.push(format!("{publisher}\\{name}"));
        }
    }
    let mut paths = Vec::new();
    for segment in segments {
        paths.push(format!("HKCU\\Software\\{segment}"));
        paths.push(format!("HKLM\\Software\\{segment}"));
    }
    deduplicate_strings(paths)
}

fn add_file_root(
    roots: &mut Vec<MonitorRootInfo>,
    value: &str,
    source: &str,
    warnings: &mut Vec<String>,
) {
    let path = value.trim();
    if path.is_empty() {
        return;
    }
    let key = path.to_ascii_lowercase();
    if roots
        .iter()
        .any(|root| root.path.to_ascii_lowercase() == key)
    {
        return;
    }
    if utils::is_system_critical_path(path) {
        warnings.push(format!("拒绝监控关键系统目录: {path}"));
        roots.push(MonitorRootInfo {
            path: path.to_string(),
            source: source.to_string(),
            enabled: false,
            reason: Some("关键系统路径不允许进入安装监控".to_string()),
        });
        return;
    }
    if roots.len() >= MAX_ROOTS {
        warnings.push("安装监控目录范围达到安全上限".to_string());
        return;
    }
    roots.push(MonitorRootInfo {
        path: path.to_string(),
        source: source.to_string(),
        enabled: true,
        reason: None,
    });
}

fn add_registry_root(
    roots: &mut Vec<MonitorRootInfo>,
    value: &str,
    source: &str,
    warnings: &mut Vec<String>,
) {
    let path = value.trim();
    if path.is_empty() {
        return;
    }
    if parse_registry_key_path(path).is_err() {
        warnings.push(format!("忽略无效注册表监控范围: {path}"));
        roots.push(MonitorRootInfo {
            path: path.to_string(),
            source: source.to_string(),
            enabled: false,
            reason: Some("注册表根键或路径格式无效".to_string()),
        });
        return;
    }
    if roots.len() >= MAX_ROOTS {
        warnings.push("安装监控注册表范围达到安全上限".to_string());
        return;
    }
    let key = path.to_ascii_lowercase();
    if roots
        .iter()
        .any(|root| root.path.to_ascii_lowercase() == key)
    {
        return;
    }
    roots.push(MonitorRootInfo {
        path: path.to_string(),
        source: source.to_string(),
        enabled: true,
        reason: None,
    });
}

fn parse_registry_key_path(path: &str) -> Result<(RegistryHive, String), UninstallerError> {
    let trimmed = path.trim();
    let lower = trimmed.to_ascii_lowercase();
    let prefixes = [
        ("hkey_local_machine\\", RegistryHive::LocalMachine),
        ("hklm\\", RegistryHive::LocalMachine),
        ("hkey_current_user\\", RegistryHive::CurrentUser),
        ("hkcu\\", RegistryHive::CurrentUser),
        ("hkey_classes_root\\", RegistryHive::ClassesRoot),
        ("hkcr\\", RegistryHive::ClassesRoot),
        ("hkey_users\\", RegistryHive::Users),
        ("hku\\", RegistryHive::Users),
    ];
    for (prefix, hive) in prefixes {
        if lower.starts_with(prefix) {
            let key_path = trimmed[prefix.len()..].to_string();
            let normalized_path = format!("{}\\{}", hive.prefix(), key_path);
            if key_path.is_empty() || utils::is_critical_registry_path(&normalized_path) {
                return Err(UninstallerError::CriticalSystemItem(
                    "关键或空注册表路径不允许监控".to_string(),
                ));
            }
            return Ok((hive, key_path));
        }
    }
    Err(UninstallerError::Registry("不支持的注册表根键".to_string()))
}

fn root_requires_admin(path: &str) -> bool {
    let upper = path.to_ascii_uppercase();
    if upper.starts_with("HKLM\\")
        || upper.starts_with("HKEY_LOCAL_MACHINE\\")
        || upper.starts_with("HKCR\\")
        || upper.starts_with("HKEY_CLASSES_ROOT\\")
        || upper.starts_with("HKU\\")
        || upper.starts_with("HKEY_USERS\\")
    {
        return true;
    }
    [
        "ProgramFiles",
        "ProgramFiles(x86)",
        "ProgramW6432",
        "ProgramData",
    ]
    .iter()
    .filter_map(|name| std::env::var(name).ok())
    .any(|root| upper.starts_with(&root.to_ascii_uppercase()))
}

fn is_safe_segment(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && !value.contains(['\\', '/', ':'])
        && value != "."
        && value != ".."
}

fn deduplicate_strings(values: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    values
        .into_iter()
        .filter(|value| seen.insert(value.to_ascii_lowercase()))
        .collect()
}

fn monitor_root() -> Result<PathBuf, UninstallerError> {
    let root = storage::get_storage_root_dir()?.join(MONITOR_DIR_NAME);
    fs::create_dir_all(&root)?;
    Ok(root)
}

fn session_directory(session_id: &str) -> Result<PathBuf, UninstallerError> {
    Uuid::parse_str(session_id)
        .map_err(|_| UninstallerError::PermissionDenied("无效的安装监控会话 ID".to_string()))?;
    Ok(monitor_root()?.join(session_id))
}

fn write_session(session: &InstallMonitorSession) -> Result<(), UninstallerError> {
    let directory = session_directory(&session.id)?;
    fs::create_dir_all(&directory)?;
    let manifest = directory.join(MANIFEST_FILE_NAME);
    if let Ok(metadata) = fs::symlink_metadata(&manifest) {
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(UninstallerError::PermissionDenied(
                "安装监控清单不是普通文件，拒绝覆盖".to_string(),
            ));
        }
    }
    let content = serde_json::to_vec_pretty(session)
        .map_err(|error| UninstallerError::Serde(error.to_string()))?;
    fs::write(manifest, content)?;
    Ok(())
}

fn load_session(session_id: &str) -> Result<InstallMonitorSession, UninstallerError> {
    let directory = session_directory(session_id)?;
    let directory_metadata = match fs::symlink_metadata(&directory) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Err(UninstallerError::NotFound("安装监控会话不存在".to_string()))
        }
        Err(error) => return Err(UninstallerError::FileSystem(error)),
    };
    if directory_metadata.file_type().is_symlink() || !directory_metadata.is_dir() {
        return Err(UninstallerError::PermissionDenied(
            "安装监控会话目录无效".to_string(),
        ));
    }
    let manifest = directory.join(MANIFEST_FILE_NAME);
    let metadata = fs::symlink_metadata(&manifest)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(UninstallerError::PermissionDenied(
            "安装监控清单无效".to_string(),
        ));
    }
    read_json_file(&manifest)
}

fn read_json_file<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, UninstallerError> {
    let content = fs::read_to_string(path)?;
    serde_json::from_str(&content)
        .map_err(|error| UninstallerError::Serde(format!("解析安装监控清单失败: {error}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::lister::models::InstallSource;
    use crate::modules::lister::storage::TEST_STORAGE_ENV_LOCK;

    fn temporary_root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "rust-yu-install-monitor-test-{label}-{}",
            Uuid::new_v4()
        ))
    }

    fn with_storage_root<T>(label: &str, test: impl FnOnce(&Path) -> T) -> T {
        let _guard = TEST_STORAGE_ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let root = temporary_root(label);
        let previous = std::env::var_os("RUST_YU_STORAGE_DIR");
        std::env::set_var("RUST_YU_STORAGE_DIR", &root);
        let result = test(&root);
        match previous {
            Some(value) => std::env::set_var("RUST_YU_STORAGE_DIR", value),
            None => std::env::remove_var("RUST_YU_STORAGE_DIR"),
        }
        let _ = fs::remove_dir_all(root);
        result
    }

    #[test]
    fn file_snapshot_detects_added_modified_and_removed_items() {
        with_storage_root("diff", |root| {
            let monitored = root.join("watched");
            fs::create_dir_all(&monitored).expect("测试目录应可创建");
            let changed = monitored.join("changed.txt");
            let removed = monitored.join("removed.txt");
            fs::write(&changed, b"before").expect("测试文件应可写入");
            fs::write(&removed, b"removed").expect("测试文件应可写入");
            let mut program =
                InstalledProgram::new("Demo App".to_string(), InstallSource::Registry);
            program.install_location = Some(monitored.to_string_lossy().into_owned());
            let request = InstallMonitorStartRequest {
                program: program.clone(),
                extra_file_roots: vec![monitored.to_string_lossy().into_owned()],
                extra_registry_roots: Vec::new(),
                activity_kind: MonitorActivityKind::Update,
                expires_after_minutes: Some(60),
            };
            let info = start_monitor(request).expect("开始监控不应失败");
            fs::write(&changed, b"after-").expect("测试文件应可写入");
            fs::write(monitored.join("added.txt"), b"added").expect("测试文件应可写入");
            fs::remove_file(&removed).expect("测试文件应可删除");
            let session = complete_monitor(&info.id).expect("完成监控不应失败");
            assert!(session
                .changes
                .iter()
                .any(|change| change.kind == MonitorChangeKind::Added
                    && change.path.ends_with("added.txt")));
            assert!(session
                .changes
                .iter()
                .any(|change| change.kind == MonitorChangeKind::Modified
                    && change.path.ends_with("changed.txt")));
            assert!(session
                .changes
                .iter()
                .any(|change| change.kind == MonitorChangeKind::Removed
                    && change.path.ends_with("removed.txt")));
            assert_eq!(traces_for_session(&info.id).expect("应能转换证据").len(), 2);
        });
    }

    #[test]
    fn plan_rejects_critical_roots_and_exports_only_changes() {
        let mut program = InstalledProgram::new("Demo App".to_string(), InstallSource::Registry);
        program.install_location = Some(r"C:\Windows\System32".to_string());
        let plan = build_plan(&program, &[r"C:\Windows\System32".to_string()], &[]);
        assert!(plan.file_roots.iter().any(|root| !root.enabled));
        assert!(plan
            .warnings
            .iter()
            .any(|warning| warning.contains("关键系统")));
    }

    #[cfg(windows)]
    #[test]
    fn registry_snapshot_detects_value_changes_without_writing_back() {
        use winreg::enums::{HKEY_CURRENT_USER, KEY_READ};
        use winreg::RegKey;

        with_storage_root("registry", |_| {
            let root = RegKey::predef(HKEY_CURRENT_USER);
            let key_path = format!("Software\\RustYu\\InstallMonitorTest\\{}", Uuid::new_v4());
            let (key, _) = root.create_subkey(&key_path).expect("测试注册表键应可创建");
            key.set_value("Version", &1u32).expect("测试值应可写入");
            let scope = MonitorScope {
                file_roots: Vec::new(),
                registry_roots: vec![format!("HKCU\\{key_path}")],
            };
            let before = capture_scope(&scope).expect("注册表前快照不应失败");
            key.set_value("Version", &2u32).expect("测试值应可修改");
            key.set_value("NewValue", &"installed")
                .expect("测试值应可写入");
            let after = capture_scope(&scope).expect("注册表后快照不应失败");
            let program = InstalledProgram::new("Demo".to_string(), InstallSource::Registry);
            let changes = diff_snapshots(&program, &before, &after);
            assert!(changes.iter().any(|change| {
                change.kind == MonitorChangeKind::Modified && change.path.ends_with("Version")
            }));
            assert!(changes.iter().any(|change| {
                change.kind == MonitorChangeKind::Added && change.path.ends_with("NewValue")
            }));
            let reopened = root
                .open_subkey_with_flags(&key_path, KEY_READ)
                .expect("测试注册表键应仍存在");
            let version: u32 = reopened.get_value("Version").expect("测试值应仍可读");
            assert_eq!(version, 2);
            let _ = root.delete_subkey_all(&key_path);
        });
    }
}
