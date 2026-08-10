//! 卸载残留的可恢复备份会话。
//!
//! 备份只覆盖文件、目录和注册表键/值。服务、计划任务和驱动仍由系统集成
//! 模块单独处理，不在这里伪造一个无法完整恢复的备份。所有清理前备份都
//! 写入 AppData 下的独立会话目录，恢复时拒绝覆盖用户在清理后重新创建的
//! 路径或注册表值。

use crate::modules::cleaner::safety;
use crate::modules::common::error::UninstallerError;
use crate::modules::common::utils;
use crate::modules::lister::storage;
use crate::modules::scanner::models::{Trace, TraceType};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Component, Path, PathBuf};
use uuid::Uuid;
use walkdir::WalkDir;
use winreg::enums::*;
use winreg::{RegKey, RegValue};

const BACKUP_DIR_NAME: &str = "backups";
const MANIFEST_FILE_NAME: &str = "manifest.json";
const MAX_BACKUP_ENTRIES: usize = 100_000;
const MAX_BACKUP_BYTES: u64 = 8 * 1024 * 1024 * 1024;
const MAX_SESSION_BYTES: u64 = 16 * 1024 * 1024 * 1024;
const MAX_REGISTRY_NODES: usize = 20_000;
const MAX_REGISTRY_DEPTH: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackupItemKind {
    File,
    Directory,
    RegistryKey,
    RegistryValue,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackupItemState {
    Ready,
    Missing,
    BackupFailed,
    DeleteSucceeded,
    DeleteFailed,
    Restored,
    RestoreFailed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackupSessionStatus {
    Prepared,
    PartiallyCleaned,
    Restored,
    RestoreFailed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupPlanItem {
    pub trace_id: String,
    pub path: String,
    pub trace_type: TraceType,
    pub kind: Option<BackupItemKind>,
    pub exists: bool,
    pub estimated_bytes: u64,
    pub eligible: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BackupPlan {
    pub items: Vec<BackupPlanItem>,
    pub total_bytes: u64,
    pub eligible_count: usize,
    pub unsupported_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupItem {
    pub trace_id: String,
    pub original_path: String,
    pub trace_type: TraceType,
    pub kind: BackupItemKind,
    pub payload: Option<String>,
    pub bytes: u64,
    #[serde(default)]
    pub fingerprint: Option<String>,
    pub state: BackupItemState,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupSession {
    pub id: String,
    pub created_at: DateTime<Utc>,
    pub reason: String,
    pub status: BackupSessionStatus,
    pub items: Vec<BackupItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupSessionInfo {
    pub id: String,
    pub created_at: DateTime<Utc>,
    pub reason: String,
    pub status: BackupSessionStatus,
    pub item_count: usize,
    pub restorable_count: usize,
    pub failed_count: usize,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupRestoreResult {
    pub session_id: String,
    pub success: bool,
    pub restored_count: usize,
    pub failed_count: usize,
    pub session: BackupSession,
}

#[derive(Debug, Clone)]
pub struct BackupPreparation {
    session_id: String,
    ready_trace_ids: HashSet<String>,
}

impl BackupPreparation {
    pub fn session_id(&self) -> &str {
        &self.session_id
    }

    pub fn is_ready(&self, trace_id: &str) -> bool {
        self.ready_trace_ids.contains(trace_id)
    }

    /// 在真正删除前重新确认目标仍与备份快照一致，避免备份计划和删除之间
    /// 目标被其他进程替换后，清理器在没有对应快照的情况下继续删除。
    pub fn validate_trace(&self, trace: &Trace) -> Result<(), UninstallerError> {
        if !self.is_ready(&trace.id) {
            return Err(UninstallerError::Other(
                "该项目没有可用的删除前备份".to_string(),
            ));
        }

        let session = load_session(&self.session_id)?;
        let item = session
            .items
            .iter()
            .find(|item| item.trace_id == trace.id)
            .ok_or_else(|| UninstallerError::NotFound("备份项目不存在".to_string()))?;
        let current = inspect_trace(trace)?;

        let matches_snapshot = match item.state {
            BackupItemState::Missing => !current.exists,
            BackupItemState::Ready => {
                current.exists
                    && current.kind == item.kind
                    && current.bytes == item.bytes
                    && item.fingerprint.is_some()
                    && current.fingerprint == item.fingerprint
            }
            _ => false,
        };
        if matches_snapshot {
            Ok(())
        } else {
            Err(UninstallerError::Other(
                "目标在备份后发生变化，已跳过删除以保护新内容".to_string(),
            ))
        }
    }
}

#[derive(Debug, Clone)]
struct PlanInspection {
    kind: BackupItemKind,
    exists: bool,
    bytes: u64,
    fingerprint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RegistryKeyPayload {
    hive: RegistryHive,
    key_path: String,
    root: RegistryNode,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RegistryValuePayload {
    hive: RegistryHive,
    key_path: String,
    value_name: String,
    value: RawRegistryValue,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RegistryNode {
    name: String,
    values: Vec<RawRegistryEntry>,
    children: Vec<RegistryNode>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RawRegistryEntry {
    name: String,
    value: RawRegistryValue,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct RawRegistryValue {
    value_type: u32,
    bytes: Vec<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
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
}

/// 判断一个残留是否需要文件/注册表备份。
pub fn requires_backup(trace: &Trace) -> bool {
    matches!(
        trace.trace_type,
        TraceType::File
            | TraceType::AppData
            | TraceType::Shortcut
            | TraceType::RegistryKey
            | TraceType::RegistryValue
    )
}

/// 生成清理前的只读备份计划。
///
/// 该函数与实际备份使用同一套路径检查、目录遍历上限和注册表读取逻辑，
/// 因而 dry-run 展示的项目不会在执行阶段静默换成另一批目标。
pub fn plan_for_traces(traces: &[Trace]) -> BackupPlan {
    let mut plan = BackupPlan::default();

    for trace in traces {
        let item = if !requires_backup(trace) {
            BackupPlanItem {
                trace_id: trace.id.clone(),
                path: trace.path.clone(),
                trace_type: trace.trace_type,
                kind: None,
                exists: false,
                estimated_bytes: 0,
                eligible: false,
                reason: Some("该系统集成痕迹当前没有可验证的文件/注册表备份".to_string()),
            }
        } else if let Err(error) = safety::pre_delete_check(trace) {
            BackupPlanItem {
                trace_id: trace.id.clone(),
                path: trace.path.clone(),
                trace_type: trace.trace_type,
                kind: None,
                exists: false,
                estimated_bytes: 0,
                eligible: false,
                reason: Some(error.to_string()),
            }
        } else {
            match inspect_trace(trace) {
                Ok(inspection) => {
                    let total = plan.total_bytes.saturating_add(inspection.bytes);
                    if total > MAX_SESSION_BYTES {
                        BackupPlanItem {
                            trace_id: trace.id.clone(),
                            path: trace.path.clone(),
                            trace_type: trace.trace_type,
                            kind: Some(inspection.kind),
                            exists: inspection.exists,
                            estimated_bytes: inspection.bytes,
                            eligible: false,
                            reason: Some("本次备份总量超过安全上限".to_string()),
                        }
                    } else {
                        plan.total_bytes = total;
                        plan.eligible_count += 1;
                        BackupPlanItem {
                            trace_id: trace.id.clone(),
                            path: trace.path.clone(),
                            trace_type: trace.trace_type,
                            kind: Some(inspection.kind),
                            exists: inspection.exists,
                            estimated_bytes: inspection.bytes,
                            eligible: true,
                            reason: None,
                        }
                    }
                }
                Err(error) => BackupPlanItem {
                    trace_id: trace.id.clone(),
                    path: trace.path.clone(),
                    trace_type: trace.trace_type,
                    kind: None,
                    exists: false,
                    estimated_bytes: 0,
                    eligible: false,
                    reason: Some(error.to_string()),
                },
            }
        };

        if !item.eligible && !requires_backup(trace) {
            plan.unsupported_count += 1;
        }
        plan.items.push(item);
    }

    plan
}

/// 为本次清理生成并持久化备份会话。
///
/// 返回的准备对象只把已经成功写入持久化载荷的项目标记为 ready；调用方
/// 必须拒绝删除其他项目。路径不存在时记录 Missing，和现有清理器的幂等
/// 语义一致，但不会伪造一个需要恢复的文件。
pub fn prepare_for_traces(
    traces: &[Trace],
    reason: &str,
) -> Result<Option<BackupPreparation>, UninstallerError> {
    let plan = plan_for_traces(traces);
    let supported = plan
        .items
        .iter()
        .filter(|item| requires_backup_type(item.trace_type))
        .collect::<Vec<_>>();
    if supported.is_empty() {
        return Ok(None);
    }

    let id = Uuid::new_v4().to_string();
    let session_dir = create_session_directory(&id)?;
    let mut session = BackupSession {
        id: id.clone(),
        created_at: Utc::now(),
        reason: reason.to_string(),
        status: BackupSessionStatus::Prepared,
        items: Vec::with_capacity(supported.len()),
    };
    let mut ready_trace_ids = HashSet::new();

    for (index, plan_item) in supported.iter().enumerate() {
        let kind = plan_item
            .kind
            .unwrap_or_else(|| default_backup_kind(plan_item.trace_type));
        let mut item = BackupItem {
            trace_id: plan_item.trace_id.clone(),
            original_path: plan_item.path.clone(),
            trace_type: plan_item.trace_type,
            kind,
            payload: None,
            bytes: plan_item.estimated_bytes,
            fingerprint: None,
            state: BackupItemState::BackupFailed,
            error: plan_item.reason.clone(),
        };

        if plan_item.eligible {
            match inspect_path(plan_item.trace_type, &plan_item.path) {
                Ok(current)
                    if current.kind == kind
                        && current.exists == plan_item.exists
                        && current.bytes == plan_item.estimated_bytes =>
                {
                    item.fingerprint = current.fingerprint;
                }
                Ok(_) => {
                    item.error = Some("目标在备份前发生变化，拒绝生成不匹配的快照".to_string());
                    session.items.push(item);
                    write_session(&session)?;
                    continue;
                }
                Err(error) => {
                    item.error = Some(error.to_string());
                    session.items.push(item);
                    write_session(&session)?;
                    continue;
                }
            }

            if !plan_item.exists {
                item.state = BackupItemState::Missing;
                item.error = None;
                ready_trace_ids.insert(item.trace_id.clone());
            } else {
                match write_payload(&session_dir, index, &item) {
                    Ok(payload) => {
                        item.payload = Some(payload);
                        item.state = BackupItemState::Ready;
                        item.error = None;
                        ready_trace_ids.insert(item.trace_id.clone());
                    }
                    Err(error) => {
                        item.error = Some(error.to_string());
                    }
                }
            }
        }

        session.items.push(item);
        write_session(&session)?;
    }

    Ok(Some(BackupPreparation {
        session_id: id,
        ready_trace_ids,
    }))
}

/// 把单个清理结果写回备份会话，保证报告能准确指出备份和删除是否都成功。
pub fn record_cleanup_result(
    session_id: &str,
    trace_id: &str,
    success: bool,
    error: Option<String>,
) -> Result<(), UninstallerError> {
    let mut session = load_session(session_id)?;
    let item = session
        .items
        .iter_mut()
        .find(|item| item.trace_id == trace_id)
        .ok_or_else(|| UninstallerError::NotFound("备份会话中不存在该清理项目".to_string()))?;
    // 目标在备份时已经不存在时，清理器的幂等成功不应把它伪装成可恢复
    // 项目，否则恢复中心会尝试读取不存在的载荷。
    if item.state != BackupItemState::Missing {
        item.state = if success {
            BackupItemState::DeleteSucceeded
        } else {
            BackupItemState::DeleteFailed
        };
    }
    item.error = error;
    write_session(&session)
}

/// 删除完成后的独立校验。调用方只有在目标确实不存在时才能把结果标记为成功。
pub fn verify_trace_removed(trace: &Trace) -> Result<bool, UninstallerError> {
    crate::modules::cleaner::safety::pre_delete_check(trace)?;
    Ok(!inspect_trace(trace)?.exists)
}

/// 列出恢复中心可用的备份会话。
pub fn list_sessions() -> Result<Vec<BackupSessionInfo>, UninstallerError> {
    let root = backup_root()?;
    if !root.exists() {
        return Ok(Vec::new());
    }

    let mut sessions = Vec::new();
    for entry in fs::read_dir(root)? {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                tracing::warn!("读取备份会话目录项失败: {error}");
                continue;
            }
        };
        let metadata = match fs::symlink_metadata(entry.path()) {
            Ok(metadata) => metadata,
            Err(error) => {
                tracing::warn!("读取备份会话元数据失败: {error}");
                continue;
            }
        };
        if metadata.file_type().is_symlink() || !metadata.is_dir() {
            continue;
        }
        let manifest = entry.path().join(MANIFEST_FILE_NAME);
        let manifest_metadata = match fs::symlink_metadata(&manifest) {
            Ok(metadata) => metadata,
            Err(error) => {
                tracing::warn!("读取备份清单元数据失败: {error}");
                continue;
            }
        };
        if manifest_metadata.file_type().is_symlink() || !manifest_metadata.is_file() {
            tracing::warn!("跳过无效备份清单: {}", manifest.display());
            continue;
        }
        let content = match fs::read_to_string(manifest) {
            Ok(content) => content,
            Err(error) => {
                tracing::warn!("读取备份清单失败: {error}");
                continue;
            }
        };
        let session = match serde_json::from_str::<BackupSession>(&content) {
            Ok(session) => session,
            Err(error) => {
                tracing::warn!("解析备份清单失败: {error}");
                continue;
            }
        };
        sessions.push(session_info(&session));
    }
    sessions.sort_by(|left, right| right.created_at.cmp(&left.created_at));
    Ok(sessions)
}

pub fn get_session(session_id: &str) -> Result<BackupSession, UninstallerError> {
    load_session(session_id)
}

/// 恢复一个会话；已经验证恢复的项目会跳过，失败项目可重复调用本函数重试。
pub fn restore_session(session_id: &str) -> Result<BackupRestoreResult, UninstallerError> {
    let mut session = load_session(session_id)?;
    let session_dir = session_directory(&session.id)?;
    let mut restored_count = 0usize;
    let mut failed_count = 0usize;

    for index in 0..session.items.len() {
        {
            let item = &mut session.items[index];
            if matches!(
                item.state,
                BackupItemState::Missing | BackupItemState::Restored
            ) {
                if item.state == BackupItemState::Restored && !verify_item(&session_dir, item)? {
                    item.state = BackupItemState::RestoreFailed;
                    item.error = Some("恢复后的目标校验失败".to_string());
                    failed_count += 1;
                } else if item.state == BackupItemState::Restored {
                    restored_count += 1;
                }
            } else if item.state == BackupItemState::BackupFailed {
                failed_count += 1;
            } else {
                match restore_item(&session_dir, item).and_then(|()| {
                    if verify_item(&session_dir, item)? {
                        Ok(())
                    } else {
                        Err(UninstallerError::Other("恢复后的目标校验失败".to_string()))
                    }
                }) {
                    Ok(()) => {
                        item.state = BackupItemState::Restored;
                        item.error = None;
                        restored_count += 1;
                    }
                    Err(error) => {
                        item.state = BackupItemState::RestoreFailed;
                        item.error = Some(error.to_string());
                        failed_count += 1;
                    }
                }
            }
        }
        write_session(&session)?;
    }

    session.status = session_status(&session);
    write_session(&session)?;
    Ok(BackupRestoreResult {
        session_id: session.id.clone(),
        success: failed_count == 0,
        restored_count,
        failed_count,
        session,
    })
}

fn inspect_trace(trace: &Trace) -> Result<PlanInspection, UninstallerError> {
    inspect_path(trace.trace_type, &trace.path)
}

fn inspect_path(trace_type: TraceType, path: &str) -> Result<PlanInspection, UninstallerError> {
    match trace_type {
        TraceType::File | TraceType::AppData | TraceType::Shortcut => inspect_filesystem_path(path),
        TraceType::RegistryKey => inspect_registry_key_path(path),
        TraceType::RegistryValue => inspect_registry_value_path(path),
        _ => Err(UninstallerError::Other("不支持的备份类型".to_string())),
    }
}

fn inspect_filesystem_path(path: &str) -> Result<PlanInspection, UninstallerError> {
    let source = Path::new(path);
    let metadata = match fs::symlink_metadata(source) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(PlanInspection {
                kind: BackupItemKind::File,
                exists: false,
                bytes: 0,
                fingerprint: None,
            })
        }
        Err(error) => return Err(UninstallerError::FileSystem(error)),
    };
    if metadata.file_type().is_symlink() {
        return Err(UninstallerError::PermissionDenied(
            "备份不会跟随符号链接或目录联接".to_string(),
        ));
    }
    if metadata.is_dir() {
        let stats = filesystem_stats(source)?;
        Ok(PlanInspection {
            kind: BackupItemKind::Directory,
            exists: true,
            bytes: stats.bytes,
            fingerprint: Some(filesystem_fingerprint(source)?),
        })
    } else if metadata.is_file() {
        if metadata.len() > MAX_BACKUP_BYTES {
            return Err(UninstallerError::Other(
                "备份文件大小超过安全上限".to_string(),
            ));
        }
        Ok(PlanInspection {
            kind: BackupItemKind::File,
            exists: true,
            bytes: metadata.len(),
            fingerprint: Some(filesystem_fingerprint(source)?),
        })
    } else {
        Err(UninstallerError::Other(
            "备份目标不是普通文件或目录".to_string(),
        ))
    }
}

fn filesystem_stats(path: &Path) -> Result<TreeStats, UninstallerError> {
    let mut stats = TreeStats::default();
    for entry in WalkDir::new(path).follow_links(false) {
        let entry =
            entry.map_err(|error| UninstallerError::Other(format!("遍历备份目录失败: {error}")))?;
        stats.entries += 1;
        if stats.entries > MAX_BACKUP_ENTRIES {
            return Err(UninstallerError::Other(
                "备份目录项目数超过安全上限".to_string(),
            ));
        }
        if entry.file_type().is_symlink() {
            return Err(UninstallerError::PermissionDenied(
                "备份不会跟随符号链接或目录联接".to_string(),
            ));
        }
        if entry.file_type().is_file() {
            let len = entry
                .metadata()
                .map_err(|error| UninstallerError::FileSystem(error.into()))?
                .len();
            stats.bytes = stats
                .bytes
                .checked_add(len)
                .ok_or_else(|| UninstallerError::Other("备份大小计算溢出".to_string()))?;
            if stats.bytes > MAX_BACKUP_BYTES {
                return Err(UninstallerError::Other("备份大小超过安全上限".to_string()));
            }
        }
    }
    Ok(stats)
}

fn filesystem_fingerprint(path: &Path) -> Result<String, UninstallerError> {
    let mut hasher = Sha256::new();
    if path.is_file() {
        hash_file(path, &mut hasher)?;
    } else {
        let mut entries = WalkDir::new(path)
            .follow_links(false)
            .into_iter()
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| UninstallerError::Other(format!("遍历指纹目录失败: {error}")))?;
        entries.sort_by(|left, right| left.path().cmp(right.path()));
        for entry in entries {
            if entry.file_type().is_symlink() {
                return Err(UninstallerError::PermissionDenied(
                    "指纹计算不会跟随符号链接。".to_string(),
                ));
            }
            let relative = entry.path().strip_prefix(path).unwrap_or(entry.path());
            hasher.update(relative.to_string_lossy().as_bytes());
            hasher.update(if entry.file_type().is_dir() {
                b"D"
            } else {
                b"F"
            });
            if entry.file_type().is_file() {
                hash_file(entry.path(), &mut hasher)?;
            }
        }
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn hash_file(path: &Path, hasher: &mut Sha256) -> Result<(), UninstallerError> {
    let mut file = File::open(path)?;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(())
}

fn hash_serializable<T: Serialize>(value: &T) -> Result<String, UninstallerError> {
    let bytes =
        serde_json::to_vec(value).map_err(|error| UninstallerError::Serde(error.to_string()))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn hash_bytes_with_type(value_type: u32, bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value_type.to_le_bytes());
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

#[derive(Debug, Default, Clone, Copy)]
struct TreeStats {
    entries: usize,
    bytes: u64,
}

fn inspect_registry_key_path(path: &str) -> Result<PlanInspection, UninstallerError> {
    let (hive, key_path) = parse_registry_key_path(path)?;
    let root = hive.root_key();
    let key = match root.open_subkey_with_flags(&key_path, KEY_READ) {
        Ok(key) => key,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(PlanInspection {
                kind: BackupItemKind::RegistryKey,
                exists: false,
                bytes: 0,
                fingerprint: None,
            })
        }
        Err(error) => return Err(UninstallerError::Registry(error.to_string())),
    };
    let mut count = 0usize;
    let node = capture_registry_node(&key, &key_path, 0, &mut count)?;
    Ok(PlanInspection {
        kind: BackupItemKind::RegistryKey,
        exists: true,
        bytes: registry_node_bytes(&node),
        fingerprint: Some(hash_serializable(&node)?),
    })
}

fn inspect_registry_value_path(path: &str) -> Result<PlanInspection, UninstallerError> {
    let (hive, key_path, value_name) = parse_registry_value_path(path)?;
    let root = hive.root_key();
    let key = match root.open_subkey_with_flags(&key_path, KEY_READ) {
        Ok(key) => key,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(PlanInspection {
                kind: BackupItemKind::RegistryValue,
                exists: false,
                bytes: 0,
                fingerprint: None,
            })
        }
        Err(error) => return Err(UninstallerError::Registry(error.to_string())),
    };
    match key.get_raw_value(&value_name) {
        Ok(value) => Ok(PlanInspection {
            kind: BackupItemKind::RegistryValue,
            exists: true,
            bytes: value.bytes.len() as u64,
            fingerprint: Some(hash_bytes_with_type(
                value.vtype.clone() as isize as u32,
                &value.bytes,
            )),
        }),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(PlanInspection {
            kind: BackupItemKind::RegistryValue,
            exists: false,
            bytes: 0,
            fingerprint: None,
        }),
        Err(error) => Err(UninstallerError::Registry(error.to_string())),
    }
}

fn write_payload(
    session_dir: &Path,
    index: usize,
    item: &BackupItem,
) -> Result<String, UninstallerError> {
    let item_dir = session_dir.join("items").join(format!("{index:05}"));
    fs::create_dir_all(&item_dir)?;
    match item.kind {
        BackupItemKind::File | BackupItemKind::Directory => {
            let source = Path::new(&item.original_path);
            let payload = item_dir.join("payload");
            if item.kind == BackupItemKind::File {
                copy_file(source, &payload)?;
            } else {
                copy_directory(source, &payload)?;
            }
            Ok(format!("items/{index:05}/payload"))
        }
        BackupItemKind::RegistryKey => {
            let payload = capture_registry_key_payload(&item.original_path)?;
            let relative = format!("items/{index:05}/registry.json");
            write_json_payload(&session_dir.join(&relative), &payload)?;
            Ok(relative)
        }
        BackupItemKind::RegistryValue => {
            let payload = capture_registry_value_payload(&item.original_path)?;
            let relative = format!("items/{index:05}/registry.json");
            write_json_payload(&session_dir.join(&relative), &payload)?;
            Ok(relative)
        }
    }
}

fn copy_file(source: &Path, destination: &Path) -> Result<(), UninstallerError> {
    let metadata = fs::symlink_metadata(source)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(UninstallerError::Other("备份源不是普通文件".to_string()));
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(source, destination)?;
    Ok(())
}

fn copy_directory(source: &Path, destination: &Path) -> Result<(), UninstallerError> {
    let stats = filesystem_stats(source)?;
    if stats.bytes > MAX_BACKUP_BYTES {
        return Err(UninstallerError::Other(
            "备份目录大小超过安全上限".to_string(),
        ));
    }
    fs::create_dir_all(destination)?;
    for entry in WalkDir::new(source).follow_links(false) {
        let entry =
            entry.map_err(|error| UninstallerError::Other(format!("复制备份目录失败: {error}")))?;
        if entry.file_type().is_symlink() {
            return Err(UninstallerError::PermissionDenied(
                "备份不会跟随符号链接或目录联接".to_string(),
            ));
        }
        let relative = entry
            .path()
            .strip_prefix(source)
            .map_err(|error| UninstallerError::Other(format!("计算备份相对路径失败: {error}")))?;
        if relative.as_os_str().is_empty() {
            continue;
        }
        let target = destination.join(relative);
        if entry.file_type().is_dir() {
            fs::create_dir_all(target)?;
        } else if entry.file_type().is_file() {
            copy_file(entry.path(), &target)?;
        }
    }
    Ok(())
}

fn restore_item(session_dir: &Path, item: &BackupItem) -> Result<(), UninstallerError> {
    let payload = item
        .payload
        .as_deref()
        .ok_or_else(|| UninstallerError::Other("备份项目缺少载荷文件".to_string()))?;
    let payload_path = safe_payload_path(session_dir, payload)?;
    match item.kind {
        BackupItemKind::File => restore_file(&payload_path, Path::new(&item.original_path)),
        BackupItemKind::Directory => {
            restore_directory(&payload_path, Path::new(&item.original_path))
        }
        BackupItemKind::RegistryKey => restore_registry_key(&payload_path),
        BackupItemKind::RegistryValue => restore_registry_value(&payload_path),
    }
}

fn restore_file(payload: &Path, destination: &Path) -> Result<(), UninstallerError> {
    reject_existing_target(destination)?;
    ensure_safe_parent(destination)?;
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    copy_file_create_new(payload, destination)
}

fn restore_directory(payload: &Path, destination: &Path) -> Result<(), UninstallerError> {
    reject_existing_target(destination)?;
    ensure_safe_parent(destination)?;
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    copy_directory_create_new(payload, destination)
}

fn copy_file_create_new(source: &Path, destination: &Path) -> Result<(), UninstallerError> {
    let mut input = File::open(source)?;
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(destination)?;
    io::copy(&mut input, &mut output)?;
    output.flush()?;
    Ok(())
}

fn copy_directory_create_new(source: &Path, destination: &Path) -> Result<(), UninstallerError> {
    fs::create_dir(destination)?;
    for entry in WalkDir::new(source).follow_links(false) {
        let entry =
            entry.map_err(|error| UninstallerError::Other(format!("恢复目录失败: {error}")))?;
        if entry.file_type().is_symlink() {
            return Err(UninstallerError::PermissionDenied(
                "恢复不会跟随备份载荷中的符号链接".to_string(),
            ));
        }
        let relative = entry
            .path()
            .strip_prefix(source)
            .map_err(|error| UninstallerError::Other(format!("计算恢复相对路径失败: {error}")))?;
        if relative.as_os_str().is_empty() {
            continue;
        }
        let target = destination.join(relative);
        if entry.file_type().is_dir() {
            reject_existing_target(&target)?;
            fs::create_dir(target)?;
        } else if entry.file_type().is_file() {
            reject_existing_target(&target)?;
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            copy_file_create_new(entry.path(), &target)?;
        }
    }
    Ok(())
}

fn reject_existing_target(path: &Path) -> Result<(), UninstallerError> {
    match fs::symlink_metadata(path) {
        Ok(_) => Err(UninstallerError::Other(format!(
            "恢复目标已存在，为避免覆盖用户新内容而跳过: {}",
            path.display()
        ))),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(UninstallerError::FileSystem(error)),
    }
}

fn ensure_safe_parent(path: &Path) -> Result<(), UninstallerError> {
    let mut current = path.parent();
    while let Some(parent) = current {
        match fs::symlink_metadata(parent) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() {
                    return Err(UninstallerError::PermissionDenied(
                        "恢复路径的父目录包含符号链接或目录联接".to_string(),
                    ));
                }
                if !metadata.is_dir() {
                    return Err(UninstallerError::Other(
                        "恢复路径的父项不是目录".to_string(),
                    ));
                }
                current = parent.parent();
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                current = parent.parent();
            }
            Err(error) => return Err(UninstallerError::FileSystem(error)),
        }
    }
    Ok(())
}

fn verify_item(session_dir: &Path, item: &BackupItem) -> Result<bool, UninstallerError> {
    let payload = item
        .payload
        .as_deref()
        .ok_or_else(|| UninstallerError::Other("备份项目缺少载荷文件".to_string()))?;
    let payload_path = safe_payload_path(session_dir, payload)?;
    match item.kind {
        BackupItemKind::File => files_equal(&payload_path, Path::new(&item.original_path)),
        BackupItemKind::Directory => {
            verify_directory_contents(&payload_path, Path::new(&item.original_path))
        }
        BackupItemKind::RegistryKey => {
            let payload = read_json_payload::<RegistryKeyPayload>(session_dir, item)?;
            verify_registry_key_payload(&payload)
        }
        BackupItemKind::RegistryValue => {
            let payload = read_json_payload::<RegistryValuePayload>(session_dir, item)?;
            verify_registry_value_payload(&payload)
        }
    }
}

fn files_equal(left: &Path, right: &Path) -> Result<bool, UninstallerError> {
    let left_metadata = match fs::symlink_metadata(left) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(UninstallerError::FileSystem(error)),
    };
    let right_metadata = match fs::symlink_metadata(right) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(UninstallerError::FileSystem(error)),
    };
    if left_metadata.file_type().is_symlink()
        || right_metadata.file_type().is_symlink()
        || !left_metadata.is_file()
        || !right_metadata.is_file()
        || left_metadata.len() != right_metadata.len()
    {
        return Ok(false);
    }

    let mut left_file = File::open(left)?;
    let mut right_file = File::open(right)?;
    let mut left_buffer = [0_u8; 16 * 1024];
    let mut right_buffer = [0_u8; 16 * 1024];
    loop {
        let left_read = left_file.read(&mut left_buffer)?;
        let right_read = right_file.read(&mut right_buffer)?;
        if left_read != right_read {
            return Ok(false);
        }
        if left_read == 0 {
            return Ok(true);
        }
        if left_buffer[..left_read] != right_buffer[..right_read] {
            return Ok(false);
        }
    }
}

fn verify_directory_contents(payload: &Path, destination: &Path) -> Result<bool, UninstallerError> {
    let destination_metadata = match fs::symlink_metadata(destination) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(UninstallerError::FileSystem(error)),
    };
    if destination_metadata.file_type().is_symlink() || !destination_metadata.is_dir() {
        return Ok(false);
    }

    for entry in WalkDir::new(payload).follow_links(false) {
        let entry =
            entry.map_err(|error| UninstallerError::Other(format!("校验恢复目录失败: {error}")))?;
        if entry.file_type().is_symlink() {
            return Err(UninstallerError::PermissionDenied(
                "恢复载荷包含符号链接，拒绝校验".to_string(),
            ));
        }
        let relative = entry
            .path()
            .strip_prefix(payload)
            .map_err(|error| UninstallerError::Other(format!("计算校验相对路径失败: {error}")))?;
        if relative.as_os_str().is_empty() {
            continue;
        }
        let target = destination.join(relative);
        if entry.file_type().is_dir() {
            let metadata = match fs::symlink_metadata(&target) {
                Ok(metadata) => metadata,
                Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
                Err(error) => return Err(UninstallerError::FileSystem(error)),
            };
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Ok(false);
            }
        } else if entry.file_type().is_file() && !files_equal(entry.path(), &target)? {
            return Ok(false);
        }
    }
    Ok(true)
}

fn read_json_payload<T: serde::de::DeserializeOwned>(
    session_dir: &Path,
    item: &BackupItem,
) -> Result<T, UninstallerError> {
    let payload = item
        .payload
        .as_deref()
        .ok_or_else(|| UninstallerError::Other("备份项目缺少注册表载荷".to_string()))?;
    let path = safe_payload_path(session_dir, payload)?;
    let content = fs::read_to_string(path)?;
    serde_json::from_str(&content)
        .map_err(|error| UninstallerError::Serde(format!("解析注册表备份失败: {error}")))
}

fn capture_registry_key_payload(path: &str) -> Result<RegistryKeyPayload, UninstallerError> {
    let (hive, key_path) = parse_registry_key_path(path)?;
    let key = hive
        .root_key()
        .open_subkey_with_flags(&key_path, KEY_READ)
        .map_err(|error| UninstallerError::Registry(error.to_string()))?;
    let mut count = 0usize;
    let root = capture_registry_node(&key, &key_path, 0, &mut count)?;
    Ok(RegistryKeyPayload {
        hive,
        key_path,
        root,
    })
}

fn capture_registry_value_payload(path: &str) -> Result<RegistryValuePayload, UninstallerError> {
    let (hive, key_path, value_name) = parse_registry_value_path(path)?;
    let key = hive
        .root_key()
        .open_subkey_with_flags(&key_path, KEY_READ)
        .map_err(|error| UninstallerError::Registry(error.to_string()))?;
    let value = key
        .get_raw_value(&value_name)
        .map_err(|error| UninstallerError::Registry(error.to_string()))?;
    Ok(RegistryValuePayload {
        hive,
        key_path,
        value_name,
        value: raw_registry_value(&value),
    })
}

fn capture_registry_node(
    key: &RegKey,
    name: &str,
    depth: usize,
    count: &mut usize,
) -> Result<RegistryNode, UninstallerError> {
    if depth > MAX_REGISTRY_DEPTH {
        return Err(UninstallerError::Other(
            "注册表备份深度超过安全上限".to_string(),
        ));
    }
    *count += 1;
    if *count > MAX_REGISTRY_NODES {
        return Err(UninstallerError::Other(
            "注册表备份键数超过安全上限".to_string(),
        ));
    }

    let mut values = Vec::new();
    for result in key.enum_values() {
        let (value_name, value) =
            result.map_err(|error| UninstallerError::Registry(error.to_string()))?;
        values.push(RawRegistryEntry {
            name: value_name,
            value: raw_registry_value(&value),
        });
    }

    let mut children = Vec::new();
    for result in key.enum_keys() {
        let child_name = result.map_err(|error| UninstallerError::Registry(error.to_string()))?;
        let child = key
            .open_subkey_with_flags(&child_name, KEY_READ)
            .map_err(|error| UninstallerError::Registry(error.to_string()))?;
        children.push(capture_registry_node(
            &child,
            &child_name,
            depth + 1,
            count,
        )?);
    }

    Ok(RegistryNode {
        name: name.to_string(),
        values,
        children,
    })
}

fn raw_registry_value(value: &RegValue) -> RawRegistryValue {
    RawRegistryValue {
        value_type: value.vtype.clone() as isize as u32,
        bytes: value.bytes.clone(),
    }
}

fn registry_node_bytes(node: &RegistryNode) -> u64 {
    node.values
        .iter()
        .map(|entry| entry.value.bytes.len() as u64)
        .chain(node.children.iter().map(registry_node_bytes))
        .sum()
}

fn restore_registry_key(payload_path: &Path) -> Result<(), UninstallerError> {
    let payload = read_json_file::<RegistryKeyPayload>(payload_path)?;
    let root = payload.hive.root_key();
    match root.open_subkey_with_flags(&payload.key_path, KEY_READ | KEY_WRITE) {
        Ok(existing) => return restore_registry_node(&existing, &payload.root),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(UninstallerError::Registry(error.to_string())),
    }
    let (key, _) = root
        .create_subkey(&payload.key_path)
        .map_err(|error| UninstallerError::Registry(error.to_string()))?;
    restore_registry_node(&key, &payload.root)
}

fn restore_registry_node(key: &RegKey, node: &RegistryNode) -> Result<(), UninstallerError> {
    for entry in &node.values {
        let value = reg_value(&entry.value)?;
        match key.get_raw_value(&entry.name) {
            Ok(existing) if raw_registry_value(&existing) == entry.value => {}
            Ok(_) => {
                return Err(UninstallerError::Other(format!(
                    "恢复注册表值 {} 时发现新内容，为避免覆盖而跳过",
                    entry.name
                )))
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => key
                .set_raw_value(&entry.name, &value)
                .map_err(|error| UninstallerError::Registry(error.to_string()))?,
            Err(error) => return Err(UninstallerError::Registry(error.to_string())),
        }
    }
    for child in &node.children {
        let child_key = match key.open_subkey_with_flags(&child.name, KEY_READ | KEY_WRITE) {
            Ok(child_key) => child_key,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                key.create_subkey(&child.name)
                    .map_err(|error| UninstallerError::Registry(error.to_string()))?
                    .0
            }
            Err(error) => return Err(UninstallerError::Registry(error.to_string())),
        };
        restore_registry_node(&child_key, child)?;
    }
    Ok(())
}

fn restore_registry_value(payload_path: &Path) -> Result<(), UninstallerError> {
    let payload = read_json_file::<RegistryValuePayload>(payload_path)?;
    let value = reg_value(&payload.value)?;
    let root = payload.hive.root_key();
    let key = match root.open_subkey_with_flags(&payload.key_path, KEY_READ | KEY_WRITE) {
        Ok(key) => key,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            root.create_subkey(&payload.key_path)
                .map_err(|error| UninstallerError::Registry(error.to_string()))?
                .0
        }
        Err(error) => return Err(UninstallerError::Registry(error.to_string())),
    };
    match key.get_raw_value(&payload.value_name) {
        Ok(existing) => {
            if raw_registry_value(&existing) == payload.value {
                Ok(())
            } else {
                Err(UninstallerError::Other(
                    "恢复注册表值目标已存在且内容不同，为避免覆盖而跳过".to_string(),
                ))
            }
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => key
            .set_raw_value(&payload.value_name, &value)
            .map_err(|error| UninstallerError::Registry(error.to_string())),
        Err(error) => Err(UninstallerError::Registry(error.to_string())),
    }
}

fn verify_registry_key_payload(payload: &RegistryKeyPayload) -> Result<bool, UninstallerError> {
    let key = match payload
        .hive
        .root_key()
        .open_subkey_with_flags(&payload.key_path, KEY_READ)
    {
        Ok(key) => key,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(UninstallerError::Registry(error.to_string())),
    };
    verify_registry_node(&key, &payload.root)
}

fn verify_registry_value_payload(payload: &RegistryValuePayload) -> Result<bool, UninstallerError> {
    let key = match payload
        .hive
        .root_key()
        .open_subkey_with_flags(&payload.key_path, KEY_READ)
    {
        Ok(key) => key,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(UninstallerError::Registry(error.to_string())),
    };
    match key.get_raw_value(&payload.value_name) {
        Ok(value) => Ok(raw_registry_value(&value) == payload.value),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(UninstallerError::Registry(error.to_string())),
    }
}

fn verify_registry_node(key: &RegKey, node: &RegistryNode) -> Result<bool, UninstallerError> {
    for entry in &node.values {
        let value = match key.get_raw_value(&entry.name) {
            Ok(value) => value,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(UninstallerError::Registry(error.to_string())),
        };
        if raw_registry_value(&value) != entry.value {
            return Ok(false);
        }
    }
    for child in &node.children {
        let child_key = match key.open_subkey_with_flags(&child.name, KEY_READ) {
            Ok(key) => key,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(false),
            Err(error) => return Err(UninstallerError::Registry(error.to_string())),
        };
        if !verify_registry_node(&child_key, child)? {
            return Ok(false);
        }
    }
    Ok(true)
}

fn reg_value(value: &RawRegistryValue) -> Result<RegValue, UninstallerError> {
    let vtype = match value.value_type {
        value if value == REG_NONE as u32 => REG_NONE,
        value if value == REG_SZ as u32 => REG_SZ,
        value if value == REG_EXPAND_SZ as u32 => REG_EXPAND_SZ,
        value if value == REG_BINARY as u32 => REG_BINARY,
        value if value == REG_DWORD as u32 => REG_DWORD,
        value if value == REG_DWORD_BIG_ENDIAN as u32 => REG_DWORD_BIG_ENDIAN,
        value if value == REG_LINK as u32 => REG_LINK,
        value if value == REG_MULTI_SZ as u32 => REG_MULTI_SZ,
        value if value == REG_RESOURCE_LIST as u32 => REG_RESOURCE_LIST,
        value if value == REG_FULL_RESOURCE_DESCRIPTOR as u32 => REG_FULL_RESOURCE_DESCRIPTOR,
        value if value == REG_RESOURCE_REQUIREMENTS_LIST as u32 => REG_RESOURCE_REQUIREMENTS_LIST,
        value if value == REG_QWORD as u32 => REG_QWORD,
        _ => {
            return Err(UninstallerError::Registry(
                "备份中的注册表类型不受支持".to_string(),
            ))
        }
    };
    Ok(RegValue {
        vtype,
        bytes: value.bytes.clone(),
    })
}

fn parse_registry_key_path(path: &str) -> Result<(RegistryHive, String), UninstallerError> {
    let (hive, key_path) = parse_registry_prefix(path)?;
    if key_path.is_empty() {
        return Err(UninstallerError::Registry("注册表键路径为空".to_string()));
    }
    if utils::is_critical_registry_path(path) {
        return Err(UninstallerError::CriticalSystemItem(
            "关键系统注册表路径不允许备份或删除".to_string(),
        ));
    }
    Ok((hive, key_path.to_string()))
}

fn parse_registry_value_path(
    path: &str,
) -> Result<(RegistryHive, String, String), UninstallerError> {
    let (hive, rest) = parse_registry_prefix(path)?;
    let separator = rest
        .rfind('\\')
        .ok_or_else(|| UninstallerError::Registry("注册表值路径缺少键路径".to_string()))?;
    let key_path = &rest[..separator];
    let value_name = &rest[separator + 1..];
    if key_path.is_empty() {
        return Err(UninstallerError::Registry("注册表值键路径为空".to_string()));
    }
    if utils::is_critical_registry_path(path) {
        return Err(UninstallerError::CriticalSystemItem(
            "关键系统注册表路径不允许备份或删除".to_string(),
        ));
    }
    Ok((hive, key_path.to_string(), value_name.to_string()))
}

fn parse_registry_prefix(path: &str) -> Result<(RegistryHive, &str), UninstallerError> {
    let path = path.trim();
    let lower = path.to_ascii_lowercase();
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
            return Ok((hive, &path[prefix.len()..]));
        }
    }
    Err(UninstallerError::Registry("不支持的注册表根键".to_string()))
}

fn requires_backup_type(trace_type: TraceType) -> bool {
    matches!(
        trace_type,
        TraceType::File
            | TraceType::AppData
            | TraceType::Shortcut
            | TraceType::RegistryKey
            | TraceType::RegistryValue
    )
}

fn default_backup_kind(trace_type: TraceType) -> BackupItemKind {
    match trace_type {
        TraceType::RegistryKey => BackupItemKind::RegistryKey,
        TraceType::RegistryValue => BackupItemKind::RegistryValue,
        _ => BackupItemKind::File,
    }
}

fn write_json_payload<T: Serialize>(path: &Path, value: &T) -> Result<(), UninstallerError> {
    let content = serde_json::to_vec_pretty(value)
        .map_err(|error| UninstallerError::Serde(error.to_string()))?;
    fs::write(path, content)?;
    Ok(())
}

fn read_json_file<T: serde::de::DeserializeOwned>(path: &Path) -> Result<T, UninstallerError> {
    let content = fs::read_to_string(path)?;
    serde_json::from_str(&content)
        .map_err(|error| UninstallerError::Serde(format!("解析备份载荷失败: {error}")))
}

fn safe_payload_path(session_dir: &Path, relative: &str) -> Result<PathBuf, UninstallerError> {
    let relative_path = Path::new(relative);
    if relative_path.is_absolute()
        || relative_path
            .components()
            .any(|component| matches!(component, Component::ParentDir | Component::RootDir))
    {
        return Err(UninstallerError::PermissionDenied(
            "备份载荷路径越过会话目录，拒绝读取".to_string(),
        ));
    }
    let path = session_dir.join(relative_path);
    match fs::symlink_metadata(session_dir) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err(UninstallerError::PermissionDenied(
                "备份会话目录是符号链接，拒绝读取载荷".to_string(),
            ))
        }
        Ok(metadata) if !metadata.is_dir() => {
            return Err(UninstallerError::Other("备份会话目录无效".to_string()))
        }
        Err(error) => return Err(UninstallerError::FileSystem(error)),
        _ => {}
    }
    let canonical_session = fs::canonicalize(session_dir)?;
    let canonical_parent = fs::canonicalize(
        path.parent()
            .ok_or_else(|| UninstallerError::Other("备份载荷缺少父目录".to_string()))?,
    )?;
    if !canonical_parent.starts_with(&canonical_session) {
        return Err(UninstallerError::PermissionDenied(
            "备份载荷不在当前会话目录中".to_string(),
        ));
    }
    if let Ok(metadata) = fs::symlink_metadata(&path) {
        if metadata.file_type().is_symlink() {
            return Err(UninstallerError::PermissionDenied(
                "备份载荷不能是符号链接，拒绝跟随读取".to_string(),
            ));
        }
    }
    Ok(path)
}

fn backup_root() -> Result<PathBuf, UninstallerError> {
    let root = storage::get_storage_root_dir()?.join(BACKUP_DIR_NAME);
    fs::create_dir_all(&root)?;
    Ok(root)
}

fn create_session_directory(session_id: &str) -> Result<PathBuf, UninstallerError> {
    let directory = session_directory(session_id)?;
    fs::create_dir_all(directory.join("items"))?;
    Ok(directory)
}

fn session_directory(session_id: &str) -> Result<PathBuf, UninstallerError> {
    Uuid::parse_str(session_id)
        .map_err(|_| UninstallerError::PermissionDenied("无效的备份会话 ID".to_string()))?;
    Ok(backup_root()?.join(session_id))
}

fn load_session(session_id: &str) -> Result<BackupSession, UninstallerError> {
    let directory = session_directory(session_id)?;
    let directory_metadata = match fs::symlink_metadata(&directory) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Err(UninstallerError::NotFound("备份会话不存在".to_string()))
        }
        Err(error) => return Err(UninstallerError::FileSystem(error)),
    };
    if directory_metadata.file_type().is_symlink() || !directory_metadata.is_dir() {
        return Err(UninstallerError::PermissionDenied(
            "备份会话目录无效".to_string(),
        ));
    }
    let path = directory.join(MANIFEST_FILE_NAME);
    let manifest_metadata = match fs::symlink_metadata(&path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Err(UninstallerError::NotFound("备份会话不存在".to_string()))
        }
        Err(error) => return Err(UninstallerError::FileSystem(error)),
    };
    if manifest_metadata.file_type().is_symlink() || !manifest_metadata.is_file() {
        return Err(UninstallerError::PermissionDenied(
            "备份会话清单无效".to_string(),
        ));
    }
    read_json_file(&path)
}

fn write_session(session: &BackupSession) -> Result<(), UninstallerError> {
    let mut snapshot = session.clone();
    snapshot.status = session_status(&snapshot);
    let path = session_directory(&snapshot.id)?.join(MANIFEST_FILE_NAME);
    if let Ok(metadata) = fs::symlink_metadata(&path) {
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err(UninstallerError::PermissionDenied(
                "备份会话清单不是普通文件，拒绝覆盖".to_string(),
            ));
        }
    }
    write_json_payload(&path, &snapshot)
}

fn session_status(session: &BackupSession) -> BackupSessionStatus {
    if session
        .items
        .iter()
        .any(|item| item.state == BackupItemState::RestoreFailed)
    {
        return BackupSessionStatus::RestoreFailed;
    }
    if !session.items.is_empty()
        && session.items.iter().all(|item| {
            matches!(
                item.state,
                BackupItemState::Restored | BackupItemState::Missing
            )
        })
    {
        return BackupSessionStatus::Restored;
    }
    if session.items.iter().any(|item| {
        matches!(
            item.state,
            BackupItemState::DeleteSucceeded | BackupItemState::DeleteFailed
        )
    }) {
        BackupSessionStatus::PartiallyCleaned
    } else {
        BackupSessionStatus::Prepared
    }
}

fn session_info(session: &BackupSession) -> BackupSessionInfo {
    let restorable_count = session
        .items
        .iter()
        .filter(|item| {
            matches!(
                item.state,
                BackupItemState::Ready
                    | BackupItemState::DeleteSucceeded
                    | BackupItemState::DeleteFailed
                    | BackupItemState::RestoreFailed
            )
        })
        .count();
    let failed_count = session
        .items
        .iter()
        .filter(|item| {
            matches!(
                item.state,
                BackupItemState::BackupFailed
                    | BackupItemState::DeleteFailed
                    | BackupItemState::RestoreFailed
            )
        })
        .count();
    BackupSessionInfo {
        id: session.id.clone(),
        created_at: session.created_at,
        reason: session.reason.clone(),
        status: session.status,
        item_count: session.items.len(),
        restorable_count,
        failed_count,
        bytes: session.items.iter().map(|item| item.bytes).sum(),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        list_sessions, plan_for_traces, prepare_for_traces, record_cleanup_result, restore_session,
        BackupItemKind, BackupItemState, BackupSessionStatus,
    };
    use crate::modules::lister::storage::TEST_STORAGE_ENV_LOCK;
    use crate::modules::scanner::models::{Trace, TraceType};
    use std::fs;
    use std::path::PathBuf;

    fn temporary_root(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "rust-yu-backup-test-{label}-{}",
            uuid::Uuid::new_v4()
        ))
    }

    fn with_storage_root<T>(label: &str, test: impl FnOnce(&PathBuf) -> T) -> T {
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
    fn dry_run_marks_unsupported_system_items_without_mixing_them_into_backup() {
        let file_trace = Trace::new(
            "Demo".to_string(),
            TraceType::File,
            r"C:\does-not-exist\demo.dll".to_string(),
        );
        let service_trace = Trace::new(
            "Demo".to_string(),
            TraceType::Service,
            "service://Demo".to_string(),
        );
        let plan = plan_for_traces(&[file_trace, service_trace]);

        assert_eq!(plan.items.len(), 2);
        assert_eq!(plan.eligible_count, 1);
        assert_eq!(plan.unsupported_count, 1);
        assert_eq!(plan.items[0].kind, Some(BackupItemKind::File));
        assert!(plan.items[0].eligible);
        assert!(!plan.items[1].eligible);
    }

    #[test]
    fn file_backup_restore_roundtrip_is_verified_and_non_overwriting() {
        with_storage_root("roundtrip", |storage_root| {
            let target = temporary_root("payload").join("Demo App");
            fs::create_dir_all(&target).expect("测试目录应可创建");
            let file = target.join("leftover.txt");
            fs::write(&file, b"original").expect("测试文件应可写入");
            let trace = Trace::new(
                "Demo".to_string(),
                TraceType::File,
                target.to_string_lossy().to_string(),
            );

            let plan = plan_for_traces(std::slice::from_ref(&trace));
            let preparation = prepare_for_traces(std::slice::from_ref(&trace), "test")
                .expect("备份准备不应失败")
                .expect("应创建备份会话");
            assert!(preparation.is_ready(&trace.id));
            let sessions = list_sessions().expect("应能列出备份会话");
            assert_eq!(sessions.len(), 1);
            assert_eq!(sessions[0].bytes, plan.total_bytes);

            fs::remove_dir_all(&target).expect("测试目录应可删除");
            let restored = restore_session(preparation.session_id()).expect("恢复不应失败");
            assert!(restored.success);
            assert_eq!(restored.session.status, BackupSessionStatus::Restored);
            assert_eq!(fs::read(&file).ok().as_deref(), Some(&b"original"[..]));

            fs::write(&file, b"changed!").expect("测试文件应可覆盖");
            let retry = restore_session(preparation.session_id()).expect("重试应返回结果");
            assert!(!retry.success);
            assert!(retry
                .session
                .items
                .iter()
                .any(|item| item.state == BackupItemState::RestoreFailed));
            let _ = fs::remove_dir_all(target.parent().unwrap_or_else(|| storage_root.as_path()));
        });
    }

    #[cfg(windows)]
    #[test]
    fn registry_key_and_value_backup_restore_roundtrip() {
        use winreg::enums::{HKEY_CURRENT_USER, KEY_READ};
        use winreg::RegKey;

        with_storage_root("registry", |_| {
            let root = RegKey::predef(HKEY_CURRENT_USER);
            let base_path = format!("Software\\RustYu\\BackupTest\\{}", uuid::Uuid::new_v4());
            let tree_path = format!("{base_path}\\Tree");
            let value_path = format!("{base_path}\\ValueOnly\\Payload");
            let (tree, _) = root
                .create_subkey(&tree_path)
                .expect("测试注册表键应可创建");
            tree.set_value("Text", &"original")
                .expect("测试字符串值应可写入");
            let (nested, _) = tree.create_subkey("Nested").expect("测试子键应可创建");
            nested
                .set_value("Number", &42u32)
                .expect("测试数字值应可写入");
            let value_key_path = format!("{base_path}\\ValueOnly");
            let (value_key, _) = root
                .create_subkey(&value_key_path)
                .expect("测试值键应可创建");
            value_key
                .set_value("Payload", &"value-only")
                .expect("测试值应可写入");

            let tree_trace = Trace::new(
                "Demo".to_string(),
                TraceType::RegistryKey,
                format!("HKCU\\{tree_path}"),
            );
            let value_trace = Trace::new(
                "Demo".to_string(),
                TraceType::RegistryValue,
                format!("HKCU\\{value_path}"),
            );
            let traces = [tree_trace.clone(), value_trace.clone()];
            let preparation = prepare_for_traces(&traces, "test registry")
                .expect("注册表备份准备不应失败")
                .expect("应创建注册表备份会话");
            assert!(preparation.is_ready(&tree_trace.id));
            assert!(preparation.is_ready(&value_trace.id));
            record_cleanup_result(preparation.session_id(), &tree_trace.id, true, None)
                .expect("应记录注册表键清理结果");
            record_cleanup_result(preparation.session_id(), &value_trace.id, true, None)
                .expect("应记录注册表值清理结果");
            root.delete_subkey_all(&tree_path)
                .expect("测试注册表键应可删除");
            root.delete_subkey_all(&value_key_path)
                .expect("测试注册表值键应可删除");

            let restored = restore_session(preparation.session_id()).expect("注册表恢复不应失败");
            assert!(restored.success);
            let tree = root
                .open_subkey_with_flags(&tree_path, KEY_READ)
                .expect("注册表树应恢复");
            let text: String = tree.get_value("Text").expect("字符串值应恢复");
            assert_eq!(text, "original");
            let nested = tree
                .open_subkey_with_flags("Nested", KEY_READ)
                .expect("子键应恢复");
            let number: u32 = nested.get_value("Number").expect("数字值应恢复");
            assert_eq!(number, 42);
            let value_key = root
                .open_subkey_with_flags(&value_key_path, KEY_READ)
                .expect("值键应恢复");
            let value: String = value_key.get_value("Payload").expect("单值应恢复");
            assert_eq!(value, "value-only");

            let _ = root.delete_subkey_all(&base_path);
        });
    }
}
