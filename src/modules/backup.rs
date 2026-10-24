//! 卸载残留的可恢复备份会话。
//!
//! 备份只覆盖文件、目录和注册表键/值。服务、计划任务和驱动仍由系统集成
//! 模块单独处理，不在这里伪造一个无法完整恢复的备份。所有清理前备份都
//! 写入 AppData 下的独立会话目录，恢复时拒绝覆盖用户在清理后重新创建的
//! 路径或注册表值。

use crate::modules::cleaner::safety;
use crate::modules::common::atomic_file;
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
use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
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
const SESSION_LOCK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

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
        // 源文件未变不代表备份可恢复：载荷可能在准备后被截断、丢失或
        // 篡改。必须读回真实载荷并与准备时指纹比较，不能只信 Ready。
        if item.state == BackupItemState::Ready {
            validate_payload_integrity(&session_directory(&self.session_id)?, item)?;
        }
        let current = inspect_trace(trace)?;

        let matches_snapshot = match item.state {
            BackupItemState::Missing => !current.exists,
            BackupItemState::Ready => {
                // 备份后目标已消失（例如同批次清理先删除了父键，值随键一起消失）。
                // 缺失不属于"出现了新内容"的保护场景；按幂等放行，删除阶段会把
                // NotFound 视为成功，整次清理不应因此被误报为失败。
                if !current.exists {
                    return Ok(());
                }
                current.kind == item.kind
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
        } else {
            // 与 cleaner 的清理闸门保持同一套分流：共享应用值容器（Run/
            // App Paths/Environment/FirewallRules）里的值只做归属重校验，
            // 不按 is_critical 临界项整段拒绝——否则扫描器标记 critical 的
            // 应用私有值永远无法进入备份，也就永远无法被安全删除。
            let safety_result = if safety::is_shared_application_value_container(trace) {
                safety::revalidate_shared_value_ownership(trace)
            } else {
                safety::pre_delete_check(trace)
            };
            if let Err(error) = safety_result {
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
    let _session_lock = SessionLock::acquire(&session_dir)?;
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
    let _session_lock = SessionLock::acquire(&session_directory(session_id)?)?;
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
    // 验证阶段只关心目标是否仍然存在。共享应用值容器复用归属重校验，
    // 与清理/备份两道闸门保持同一套分流：is_critical 整段检查会误拒
    // 已经完成的应用私有值删除，把它谎报成失败。
    if crate::modules::cleaner::safety::is_shared_application_value_container(trace) {
        crate::modules::cleaner::safety::revalidate_shared_value_ownership(trace)?;
    } else {
        crate::modules::cleaner::safety::pre_delete_check(trace)?;
    }
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
    // 锁顺序固定为操作锁 → 清单事务锁；清理也遵循此顺序，禁止交错改数据。
    let _operation = acquire_operation(session_id)?;
    let _session_lock = SessionLock::acquire(&session_directory(session_id)?)?;
    let mut session = load_session(session_id)?;
    let session_dir = session_directory(&session.id)?;
    let mut restored_count = 0usize;
    let mut failed_count = 0usize;

    for index in 0..session.items.len() {
        {
            let item = &mut session.items[index];
            if item.state == BackupItemState::Missing {
                // 备份时已经不存在的项目没有载荷，也不应伪装成可恢复项。
            } else if item.state == BackupItemState::BackupFailed {
                failed_count += 1;
            } else {
                // Restored 只是历史状态，不是本次完整性证据；重验失败也要
                // 写入该项目的失败态并继续其他项，不能直接 ? 中止整个会话。
                let preparation = if item.state == BackupItemState::Restored {
                    validate_restore_inputs(&session_dir, item)
                } else {
                    restore_item(&session_dir, item)
                };
                match preparation.and_then(|()| {
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
    // 只有载荷成功落盘后才能持久化 Ready，进而允许清理源文件。
    OpenOptions::new()
        .write(true)
        .open(destination)?
        .sync_all()?;
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

fn validate_restore_inputs(session_dir: &Path, item: &BackupItem) -> Result<(), UninstallerError> {
    // 恢复前同样校验原始指纹；复制损坏载荷后再与损坏载荷自身比对会产生
    // 假成功，且可能把错误内容写回系统，因此校验必须在任何目标写入之前。
    validate_payload_integrity(session_dir, item)?;
    if matches!(item.kind, BackupItemKind::File | BackupItemKind::Directory) {
        ensure_safe_parent(Path::new(&item.original_path))?;
    }
    Ok(())
}

fn restore_item(session_dir: &Path, item: &BackupItem) -> Result<(), UninstallerError> {
    validate_restore_inputs(session_dir, item)?;
    // 数据落位和清单持久化不是同一个系统事务。上次在两者之间失败时，
    // 只接受与准备时完整指纹一致的既有目标，不重复写入，更不能覆盖新数据。
    let current = inspect_path(item.trace_type, &item.original_path)?;
    if current.exists && current.kind == item.kind && current.fingerprint == item.fingerprint {
        return Ok(());
    }
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

/// 删除与恢复共用的载荷完整性闸门。目录必须对整棵树算指纹，注册表键
/// 必须比较完整节点树，不能用“备份仍有的内容都存在”这种子集校验。
fn validate_payload_integrity(
    session_dir: &Path,
    item: &BackupItem,
) -> Result<(), UninstallerError> {
    let expected = item.fingerprint.as_deref().ok_or_else(|| {
        UninstallerError::Other("备份缺少完整性指纹，已拒绝删除或恢复".to_string())
    })?;
    let actual = match item.kind {
        BackupItemKind::File | BackupItemKind::Directory => {
            let relative = item
                .payload
                .as_deref()
                .ok_or_else(|| UninstallerError::Other("备份项目缺少载荷文件".to_string()))?;
            let path = safe_payload_path(session_dir, relative)?;
            let metadata = fs::symlink_metadata(&path)?;
            let kind_matches = match item.kind {
                BackupItemKind::File => metadata.is_file(),
                BackupItemKind::Directory => metadata.is_dir(),
                _ => false,
            };
            if metadata.file_type().is_symlink() || !kind_matches {
                return Err(UninstallerError::Other(
                    "备份载荷类型改变，已拒绝删除或恢复".to_string(),
                ));
            }
            filesystem_fingerprint(&path)?
        }
        BackupItemKind::RegistryKey => {
            let payload = read_json_payload::<RegistryKeyPayload>(session_dir, item)?;
            let (hive, key_path) = parse_registry_key_path(&item.original_path)?;
            if payload.hive != hive || payload.key_path != key_path {
                return Err(UninstallerError::Other(
                    "备份注册表键目标改变，已拒绝删除或恢复".to_string(),
                ));
            }
            hash_serializable(&payload.root)?
        }
        BackupItemKind::RegistryValue => {
            let payload = read_json_payload::<RegistryValuePayload>(session_dir, item)?;
            let (hive, key_path, value_name) = parse_registry_value_path(&item.original_path)?;
            if payload.hive != hive
                || payload.key_path != key_path
                || payload.value_name != value_name
            {
                return Err(UninstallerError::Other(
                    "备份注册表值目标改变，已拒绝删除或恢复".to_string(),
                ));
            }
            hash_bytes_with_type(payload.value.value_type, &payload.value.bytes)
        }
    };
    if actual != expected {
        return Err(UninstallerError::Other(
            "备份载荷完整性校验失败，已拒绝删除或恢复".to_string(),
        ));
    }
    Ok(())
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
    copy_reader_create_new(&mut input, destination)
}

fn copy_reader_create_new(
    input: &mut impl Read,
    destination: &Path,
) -> Result<(), UninstallerError> {
    let temporary = unique_restore_path(destination)?;
    let result = (|| {
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        io::copy(input, &mut output)?;
        output.flush()?;
        output.sync_all()?;
        drop(output);

        // 恢复数据先完整写入同目录临时文件，再原子落位；失败时绝不留下
        // 会阻塞后续重试的半成品目标，也不会覆盖用户新创建的内容。
        reject_existing_target(destination)?;
        atomic_file::rename_no_replace(&temporary, destination)?;
        Ok(())
    })();

    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn unique_restore_path(destination: &Path) -> Result<PathBuf, UninstallerError> {
    let parent = destination.parent().ok_or_else(|| {
        UninstallerError::Other(format!("恢复目标缺少父目录: {}", destination.display()))
    })?;
    for _ in 0..16 {
        let candidate = parent.join(format!(".rust-yu-restore-{}.tmp", Uuid::new_v4()));
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err(UninstallerError::Other(
        "无法创建唯一的恢复临时路径".to_string(),
    ))
}

fn copy_directory_create_new(source: &Path, destination: &Path) -> Result<(), UninstallerError> {
    let temporary = unique_restore_path(destination)?;
    fs::create_dir(&temporary)?;
    let result = copy_directory_contents(source, &temporary).and_then(|()| {
        // Windows 的同卷目录重命名不会覆盖已有目标；前置检查提供更清晰的错误。
        reject_existing_target(destination)?;
        atomic_file::rename_no_replace(&temporary, destination)?;
        Ok(())
    });

    if result.is_err() {
        let _ = fs::remove_dir_all(&temporary);
    }
    result
}

fn copy_directory_contents(source: &Path, destination: &Path) -> Result<(), UninstallerError> {
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
    // 扫描器路径里默认值以展示名 `(Default)` 结尾，真实值名是空串；
    // 不做映射会按字面量读取失败，导致默认值永远无法进入备份/恢复。
    let value_name = if value_name.eq_ignore_ascii_case("(Default)") {
        ""
    } else {
        value_name
    };
    // 值级备份只读取单个值做快照，不做关键路径整段拦截：防火墙规则值位于
    // HKLM\SYSTEM 下、Run/App Paths 值位于共享分支下，它们需要可备份才能
    // 安全删除；"是否允许删除"由 cleaner::safety 的共享容器白名单 + 归属
    // 重校验把守，备份侧不再重复设卡（整键备份仍走 parse_registry_key_path
    // 的关键路径检查）。
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
    atomic_file::write_atomic(path, &content)?;
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

/// 整个读改写事务按会话加锁，跨线程/进程共享；不同会话互不阻塞。
/// 文件句柄关闭（包括进程异常退出）由系统释放锁，锁文件不能删除重建，
/// 否则等待旧句柄的写者会与新锁文件的写者同时进入事务。
struct SessionLock {
    _file: File,
}

impl SessionLock {
    fn acquire(directory: &Path) -> Result<Self, UninstallerError> {
        Self::acquire_with_timeout(directory, SESSION_LOCK_TIMEOUT)
    }

    fn acquire_with_timeout(
        directory: &Path,
        timeout: std::time::Duration,
    ) -> Result<Self, UninstallerError> {
        let file = open_session_lock(directory, ".session.lock")?;
        let started = std::time::Instant::now();
        loop {
            match file.try_lock() {
                Ok(()) => return Ok(Self { _file: file }),
                Err(std::fs::TryLockError::Error(error)) => {
                    return Err(UninstallerError::FileSystem(error));
                }
                Err(std::fs::TryLockError::WouldBlock) => {
                    let remaining = timeout.saturating_sub(started.elapsed());
                    if remaining.is_zero() {
                        return Err(UninstallerError::Timeout(
                            "等待备份会话清单锁超时，请稍后重试".to_string(),
                        ));
                    }
                    // 仍然等待真实的独占锁，不能为超时跳过锁或改成无锁写入。
                    // 只限制锁竞争等待；IO错误原样返回，不伪装成忙碌。
                    std::thread::sleep(remaining.min(std::time::Duration::from_millis(20)));
                }
            }
        }
    }
}

/// 删除与恢复必须互斥到数据操作和结果落盘全部结束，不能只锁清单的读改写。
/// 与清单事务锁分开，避免清理持锁后写回单项结果时重入同一把锁而死锁。
pub(crate) struct BackupOperationGuard {
    _file: File,
}

pub(crate) fn acquire_operation(
    session_id: &str,
) -> Result<BackupOperationGuard, UninstallerError> {
    let file = open_session_lock(&session_directory(session_id)?, ".operation.lock")?;
    // 冲突立即报错，不能让清理/恢复命令无限等待；句柄关闭或进程退出即释放。
    file.try_lock().map_err(|error| {
        UninstallerError::Other(format!(
            "无法获取备份会话操作锁，清理或恢复可能正在运行: {error}"
        ))
    })?;
    Ok(BackupOperationGuard { _file: file })
}

fn open_session_lock(directory: &Path, name: &str) -> Result<File, UninstallerError> {
    atomic_file::validate_directory_chain(directory)?;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        // 不共享删除：持锁期间不能把锁文件替换成另一文件。
        .share_mode(
            (windows::Win32::Storage::FileSystem::FILE_SHARE_READ
                | windows::Win32::Storage::FileSystem::FILE_SHARE_WRITE)
                .0,
        )
        .custom_flags(windows::Win32::Storage::FileSystem::FILE_FLAG_OPEN_REPARSE_POINT.0)
        .open(directory.join(name))?;
    let metadata = file.metadata()?;
    if !metadata.is_file()
        || metadata.file_attributes()
            & windows::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT.0
            != 0
    {
        return Err(UninstallerError::PermissionDenied(
            "备份会话锁不是普通文件".to_string(),
        ));
    }
    Ok(file)
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
        copy_reader_create_new, list_sessions, plan_for_traces, prepare_for_traces,
        record_cleanup_result, restore_session, BackupItemKind, BackupItemState,
        BackupSessionStatus,
    };
    use crate::modules::lister::storage::TEST_STORAGE_ENV_LOCK;
    use crate::modules::scanner::models::{Trace, TraceType};
    use std::fs;
    use std::io::{self, Read};
    use std::path::PathBuf;

    struct FailAfterFirstChunk {
        first_chunk: Option<&'static [u8]>,
    }

    struct TestChildren(Vec<std::process::Child>);

    impl Drop for TestChildren {
        fn drop(&mut self) {
            for child in &mut self.0 {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }

    fn wait_for_test_condition(mut ready: impl FnMut() -> bool, message: &str) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        while !ready() {
            assert!(std::time::Instant::now() < deadline, "{message}");
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }

    impl TestChildren {
        fn wait_success(&mut self) {
            for child in &mut self.0 {
                wait_for_test_condition(
                    || match child.try_wait().expect("读取子进程状态") {
                        Some(status) => {
                            assert!(status.success(), "子进程执行失败");
                            true
                        }
                        None => false,
                    },
                    "子进程执行超时",
                );
            }
        }
    }

    impl Read for FailAfterFirstChunk {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            if let Some(chunk) = self.first_chunk.take() {
                let length = chunk.len().min(buffer.len());
                buffer[..length].copy_from_slice(&chunk[..length]);
                Ok(length)
            } else {
                Err(io::Error::other("simulated interrupted restore"))
            }
        }
    }

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
        // 负例会故意 panic；必须先恢复进程环境再释放共享锁，避免后续
        // 并行测试继承已被删除的临时存储路径。
        struct StorageScope {
            previous: Option<std::ffi::OsString>,
            root: PathBuf,
        }
        impl Drop for StorageScope {
            fn drop(&mut self) {
                match self.previous.take() {
                    Some(value) => std::env::set_var("RUST_YU_STORAGE_DIR", value),
                    None => std::env::remove_var("RUST_YU_STORAGE_DIR"),
                }
                // root 仅由 temporary_root 生成于系统临时目录的 UUID 子目录。
                let _ = fs::remove_dir_all(&self.root);
            }
        }
        let scope = StorageScope {
            previous: std::env::var_os("RUST_YU_STORAGE_DIR"),
            root,
        };
        std::env::set_var("RUST_YU_STORAGE_DIR", &scope.root);
        test(&scope.root)
    }

    #[test]
    fn session_lock_timeout_preserves_manifest_and_allows_retry() {
        with_storage_root("transaction-timeout", |root| {
            fs::create_dir_all(root).expect("创建独占夹具根");
            let target = root.join("original.txt");
            fs::write(&target, b"unchanged-source").expect("写入原始夹具");
            let trace = Trace::new(
                "RustYu lock timeout fixture".to_string(),
                TraceType::File,
                target.to_string_lossy().into_owned(),
            );
            let prepared = prepare_for_traces(std::slice::from_ref(&trace), "lock timeout")
                .expect("备份成功")
                .expect("存在会话");
            let directory = super::session_directory(&prepared.session_id).expect("会话目录");
            let manifest = directory.join(super::MANIFEST_FILE_NAME);
            let original = fs::read(&manifest).expect("原清单字节");
            let held = super::open_session_lock(&directory, ".session.lock").expect("真实锁文件");
            held.lock().expect("占有真实清单锁");
            let (release, waiting) = std::sync::mpsc::channel::<()>();
            let holder = std::thread::spawn(move || {
                let _held = held;
                // 旧实现会无限等待；仅测试看门狗在3秒后释放，保证失败测试也退出。
                let _ = waiting.recv_timeout(std::time::Duration::from_secs(3));
            });
            let result = super::SessionLock::acquire_with_timeout(
                &directory,
                std::time::Duration::from_millis(50),
            );
            let _ = release.send(());
            holder.join().expect("持锁线程已回收");
            assert!(
                matches!(
                    result,
                    Err(crate::modules::common::error::UninstallerError::Timeout(_))
                ),
                "锁仍被占用且等待预算耗尽时必须返回超时"
            );
            assert_eq!(fs::read(&manifest).expect("超时后清单"), original);
            assert_eq!(
                fs::read(&target).expect("超时后源文件"),
                b"unchanged-source"
            );
            // 零预算也应先尝试一次，释放后可以立即成功，不能伪造忙碌。
            let retry =
                super::SessionLock::acquire_with_timeout(&directory, std::time::Duration::ZERO)
                    .expect("锁释放后立即重试");
            drop(retry);
            record_cleanup_result(
                &prepared.session_id,
                &trace.id,
                false,
                Some("not deleted".to_string()),
            )
            .expect("生产入口仍可写回结果");
        });
    }

    #[test]
    fn backup_locks_released_after_process_termination_allow_recovery() {
        with_storage_root("terminated-lock-holder", |root| {
            fs::create_dir_all(root).expect("创建独占夹具根");
            let target = root.join("interrupted.txt");
            fs::write(&target, b"original-after-interruption").expect("写入夹具原字节");
            let trace = Trace::new(
                "RustYu terminated process fixture".to_string(),
                TraceType::File,
                target.to_string_lossy().into_owned(),
            );
            let prepared = prepare_for_traces(std::slice::from_ref(&trace), "process termination")
                .expect("备份成功")
                .expect("存在会话");
            fs::remove_file(&target).expect("仅删除独占测试文件");
            record_cleanup_result(&prepared.session_id, &trace.id, true, None).expect("记录删除");
            let directory = super::session_directory(&prepared.session_id).expect("会话目录");
            let manifest = directory.join(super::MANIFEST_FILE_NAME);
            let original_manifest = fs::read(&manifest).expect("中断前完整清单");
            let ready = root.join("ready");
            let fixture =
                serde_json::to_string(&(&prepared.session_id, &ready, root.join("never-release")))
                    .expect("受控子进程参数");
            let mut children = TestChildren(vec![std::process::Command::new(
                std::env::current_exe().expect("本次测试程序"),
            )
            .args([
                "--exact",
                "modules::backup::tests::backup_result_child_process",
            ])
            .env("RUST_YU_INTERNAL_BACKUP_OPERATION_TEST", fixture)
            .env("RUST_YU_INTERNAL_BACKUP_HOLD_TRANSACTION", "1")
            .spawn()
            .expect("启动本轮独占持锁子进程")]);
            wait_for_test_condition(|| ready.exists(), "持锁子进程准备超时");
            assert!(super::acquire_operation(&prepared.session_id).is_err());
            assert!(matches!(
                super::SessionLock::acquire_with_timeout(&directory, std::time::Duration::ZERO),
                Err(crate::modules::common::error::UninstallerError::Timeout(_))
            ));
            assert!(restore_session(&prepared.session_id).is_err());

            // 只强制终止本测试刚创建且持有句柄的子进程，模拟无机会执行Drop的崩溃。
            children.0[0].kill().expect("终止本轮持锁夹具进程");
            assert!(!children.0[0].wait().expect("回收子进程").success());
            assert_eq!(
                fs::read(&manifest).expect("异常退出后清单"),
                original_manifest
            );
            assert!(!target.exists(), "持锁拒绝与异常退出均不能偷偷恢复文件");
            let operation = super::acquire_operation(&prepared.session_id).expect("OS已释放操作锁");
            let transaction =
                super::SessionLock::acquire_with_timeout(&directory, std::time::Duration::ZERO)
                    .expect("OS已释放清单锁");
            drop(transaction);
            drop(operation);
            let result = restore_session(&prepared.session_id).expect("异常中断后允许恢复重试");
            assert!(result.success);
            assert_eq!(result.restored_count, 1);
            assert_eq!(
                fs::read(&target).expect("独立读回原字节"),
                b"original-after-interruption"
            );
        });
    }

    #[test]
    fn operation_exclusion_preserves_data_and_manifest_until_cleanup_finishes() {
        with_storage_root("operation-exclusion", |root| {
            fs::create_dir_all(root).expect("创建独占夹具根");
            let target = root.join("pending.json");
            fs::write(&target, b"original-pending-data").expect("写入原始夹具");
            let trace = Trace::new(
                "RustYu operation fixture".to_string(),
                TraceType::File,
                target.to_string_lossy().into_owned(),
            );
            let prepared = prepare_for_traces(std::slice::from_ref(&trace), "operation exclusion")
                .expect("准备备份")
                .expect("存在会话");
            let operation = super::acquire_operation(&prepared.session_id).expect("清理占有操作锁");
            prepared.validate_trace(&trace).expect("删除前校验");
            let manifest = super::session_directory(&prepared.session_id)
                .expect("会话目录")
                .join(super::MANIFEST_FILE_NAME);
            let original_manifest = fs::read(&manifest).expect("清单原字节");
            // 精确停在校验与实际删除之间。恢复若放行，会先报告 Restored，
            // 后续删除却把刚报告恢复成功的数据再次删掉。
            let blocked = restore_session(&prepared.session_id);
            assert!(
                blocked.is_err(),
                "清理未结束时恢复必须拒绝，不能提前报告恢复成功"
            );
            assert_eq!(fs::read(&manifest).expect("拒绝后清单"), original_manifest);
            assert_eq!(
                fs::read(&target).expect("拒绝后源文件"),
                b"original-pending-data"
            );
            assert!(super::acquire_operation(&prepared.session_id).is_err());

            fs::remove_file(&target).expect("仅删除独占夹具文件");
            record_cleanup_result(&prepared.session_id, &trace.id, true, None)
                .expect("操作锁内仍可持有独立清单事务锁");
            assert!(restore_session(&prepared.session_id).is_err());
            assert!(!target.exists());
            drop(operation);

            let restored = restore_session(&prepared.session_id).expect("清理结束后可恢复");
            assert!(restored.success);
            assert_eq!(
                fs::read(&target).expect("恢复原字节"),
                b"original-pending-data"
            );
            assert!(
                restore_session(&prepared.session_id)
                    .expect("重复恢复")
                    .success
            );
            assert!(
                super::acquire_operation(&prepared.session_id).is_ok(),
                "恢复结束释放操作锁"
            );
        });
    }

    #[test]
    fn operation_exclusion_is_cross_process_and_other_sessions_remain_available() {
        with_storage_root("process-operation", |root| {
            fs::create_dir_all(root).expect("创建独占测试根");
            let target = root.join("deleted.txt");
            fs::write(&target, b"recover-after-operation").expect("写入独占夹具");
            let trace = Trace::new(
                "RustYu process operation fixture".to_string(),
                TraceType::File,
                target.to_string_lossy().into_owned(),
            );
            let prepared =
                prepare_for_traces(std::slice::from_ref(&trace), "cross-process operation")
                    .expect("准备备份")
                    .expect("存在会话");
            fs::remove_file(&target).expect("删除独占夹具");
            record_cleanup_result(&prepared.session_id, &trace.id, true, None).expect("记录删除");
            let directory = super::session_directory(&prepared.session_id).expect("会话目录");
            let manifest = directory.join(super::MANIFEST_FILE_NAME);
            let bytes = fs::read(&manifest).expect("原清单字节");
            let ready = root.join("operation-ready");
            let go = root.join("operation-release");
            let fixture = serde_json::to_string(&(&prepared.session_id, &ready, &go))
                .expect("子进程测试参数");
            let mut children = TestChildren(vec![std::process::Command::new(
                std::env::current_exe().expect("当前测试程序"),
            )
            .args([
                "--exact",
                "modules::backup::tests::backup_result_child_process",
            ])
            .env("RUST_YU_INTERNAL_BACKUP_OPERATION_TEST", fixture)
            .spawn()
            .expect("启动真实持锁子进程")]);
            wait_for_test_condition(|| ready.exists(), "持锁子进程准备超时");
            assert!(restore_session(&prepared.session_id).is_err());
            assert!(!target.exists(), "冲突拒绝时不能写回任何数据");
            assert_eq!(fs::read(&manifest).expect("拒绝后清单"), bytes);

            let independent = prepare_for_traces(
                &[Trace::new(
                    "RustYu independent fixture".to_string(),
                    TraceType::File,
                    root.join("already-missing.txt")
                        .to_string_lossy()
                        .into_owned(),
                )],
                "independent session",
            )
            .expect("独立会话")
            .expect("存在会话");
            assert!(
                restore_session(&independent.session_id)
                    .expect("不同会话不受阻塞")
                    .success
            );
            fs::write(go, b"release").expect("释放子进程操作锁");
            children.wait_success();
            assert!(
                restore_session(&prepared.session_id)
                    .expect("子进程退出后可重试")
                    .success
            );
            assert_eq!(
                fs::read(&target).expect("恢复原字节"),
                b"recover-after-operation"
            );
        });
    }

    #[test]
    fn restored_session_rejects_changed_file_and_directory_payloads() {
        with_storage_root("restored-payload-recheck", |root| {
            fs::create_dir_all(root).expect("创建临时根");
            let file = root.join("restored.txt");
            let directory = root.join("restored-directory");
            let healthy = root.join("healthy.txt");
            fs::write(&file, b"original").expect("文件原字节");
            fs::create_dir(&directory).expect("目录夹具");
            fs::write(directory.join("keep.txt"), b"keep").expect("保留子文件");
            fs::write(directory.join("lost.txt"), b"original-child").expect("另一子文件");
            fs::write(&healthy, b"healthy-original").expect("不受影响的夹具");
            let traces = [&file, &directory, &healthy].map(|path| {
                Trace::new(
                    "RustYu restore recheck fixture".to_string(),
                    TraceType::File,
                    path.to_string_lossy().into_owned(),
                )
            });
            let prepared = prepare_for_traces(&traces, "restored payload integrity")
                .expect("备份成功")
                .expect("存在会话");
            fs::remove_file(&file).expect("仅清理自建文件");
            fs::remove_dir_all(&directory).expect("仅清理自建目录");
            fs::remove_file(&healthy).expect("仅清理健康夹具");
            for trace in &traces {
                record_cleanup_result(&prepared.session_id, &trace.id, true, None)
                    .expect("记录清理结果");
            }
            let first = restore_session(&prepared.session_id).expect("首次恢复");
            assert!(first.success);
            assert!(first
                .session
                .items
                .iter()
                .all(|item| item.state == BackupItemState::Restored));
            let session_dir = root.join("backups").join(&prepared.session_id);
            let file_payload =
                session_dir.join(first.session.items[0].payload.as_ref().expect("文件载荷"));
            let directory_payload =
                session_dir.join(first.session.items[1].payload.as_ref().expect("目录载荷"));
            // 目标与载荷一起发生同样变化，自比对会误判；原始指纹必须始终生效。
            fs::write(&file, b"tampered").expect("修改自建恢复目标");
            fs::write(&file_payload, b"tampered").expect("模拟同长度载荷损坏");
            fs::remove_file(directory.join("lost.txt")).expect("模拟恢复子文件丢失");
            fs::remove_file(directory_payload.join("lost.txt")).expect("模拟载荷子文件丢失");
            let rejected =
                restore_session(&prepared.session_id).expect("返回逐项失败，继续核验健康项");
            assert!(!rejected.success, "历史 Restored 不能绕过原始指纹核验");
            assert_eq!(rejected.failed_count, 2);
            assert_eq!(rejected.restored_count, 1);
            assert!(rejected.session.items[..2]
                .iter()
                .all(|item| item.state == BackupItemState::RestoreFailed));
            assert!(rejected.session.items[..2]
                .iter()
                .all(|item| item.error.is_some()));
            assert_eq!(fs::read(&file).expect("不覆盖现有字节"), b"tampered");
            assert_eq!(
                fs::read(&healthy).expect("健康项未受影响"),
                b"healthy-original"
            );
            fs::write(&file_payload, b"original").expect("修复自建载荷");
            fs::write(directory_payload.join("lost.txt"), b"original-child").expect("补回原始载荷");
            assert!(
                !restore_session(&prepared.session_id)
                    .expect("目标冲突仍需明确报告")
                    .success
            );
            assert_eq!(fs::read(&file).expect("用户变动字节仍保留"), b"tampered");
            fs::remove_file(&file).expect("仅移除自建冲突文件");
            fs::remove_dir_all(&directory).expect("仅移除自建冲突目录");
            let retry = restore_session(&prepared.session_id).expect("冲突解除后可重试");
            assert!(retry.success);
            assert_eq!(retry.restored_count, 3);
            assert_eq!(fs::read(&file).expect("恢复原始文件字节"), b"original");
            assert_eq!(
                fs::read(directory.join("lost.txt")).expect("恢复原始目录子文件"),
                b"original-child"
            );
        });
    }

    #[test]
    fn restore_retry_after_manifest_write_failure_recovers_original_bytes() {
        use std::os::windows::fs::OpenOptionsExt;
        with_storage_root("restore-persist-retry", |root| {
            fs::create_dir_all(root).expect("测试目录可创建");
            let pending = root.join("中文夹具").join("pending.json");
            fs::create_dir_all(pending.parent().expect("父目录")).expect("夹具父目录");
            fs::write(&pending, b"{\"pending\":\"original\"}").expect("写入原始字节");
            let directory = root.join("目录夹具");
            fs::create_dir(&directory).expect("创建目录夹具");
            fs::write(directory.join("data.txt"), b"directory-original").expect("目录子文件");
            let traces = [
                Trace::new(
                    "RustYu retry fixture".to_string(),
                    TraceType::File,
                    pending.to_string_lossy().into_owned(),
                ),
                Trace::new(
                    "RustYu retry fixture".to_string(),
                    TraceType::File,
                    directory.to_string_lossy().into_owned(),
                ),
            ];
            let prepared = prepare_for_traces(&traces, "restore manifest failure")
                .expect("备份成功")
                .expect("存在会话");
            for trace in &traces {
                record_cleanup_result(&prepared.session_id, &trace.id, true, None)
                    .expect("记录夹具清理状态");
            }
            fs::remove_file(&pending).expect("只删除自建夹具文件");
            fs::remove_dir_all(&directory).expect("只删除自建夹具目录");
            let manifest = root
                .join("backups")
                .join(&prepared.session_id)
                .join("manifest.json");
            let before = fs::read(&manifest).expect("记录旧清单");
            let busy = fs::OpenOptions::new()
                .read(true)
                .share_mode(3)
                .open(&manifest)
                .expect("模拟真实清单占用，禁止替换");
            assert!(
                restore_session(&prepared.session_id).is_err(),
                "清单落盘失败必须如实报错"
            );
            assert_eq!(fs::read(&manifest).expect("旧清单仍可读"), before);
            assert_eq!(
                fs::read(&pending).expect("第一项数据已恢复"),
                b"{\"pending\":\"original\"}"
            );
            assert!(!directory.exists(), "首次状态落盘失败后应中止后续恢复");
            drop(busy);
            let retry = restore_session(&prepared.session_id).expect("解除占用后可重试");
            assert!(retry.success, "已恢复但状态未落盘的项目必须安全重试");
            assert_eq!(retry.restored_count, 2);
            assert_eq!(
                fs::read(&pending).expect("原始文件字节"),
                b"{\"pending\":\"original\"}"
            );
            assert_eq!(
                fs::read(directory.join("data.txt")).expect("目录原始字节"),
                b"directory-original"
            );
            assert!(
                restore_session(&prepared.session_id)
                    .expect("重复操作幂等")
                    .success
            );
            fs::write(&pending, b"later-user-data").expect("模拟恢复后的用户修改");
            for _ in 0..2 {
                assert!(
                    !restore_session(&prepared.session_id)
                        .expect("返回冲突结果")
                        .success
                );
                assert_eq!(
                    fs::read(&pending).expect("用户新字节保留"),
                    b"later-user-data"
                );
            }
            fs::remove_file(&pending).expect("仅移除自建的冲突夹具");
            assert!(
                restore_session(&prepared.session_id)
                    .expect("冲突解除后重试")
                    .success
            );
            assert_eq!(
                fs::read(&pending).expect("重新恢复原字节"),
                b"{\"pending\":\"original\"}"
            );
        });
    }

    #[test]
    fn backup_result_child_process() {
        if let Ok(fixture) = std::env::var("RUST_YU_INTERNAL_BACKUP_OPERATION_TEST") {
            let (session_id, ready_path, go_path): (String, PathBuf, PathBuf) =
                serde_json::from_str(&fixture).expect("独占操作夹具参数");
            let _operation = super::acquire_operation(&session_id).expect("子进程占有操作锁");
            let _transaction =
                if std::env::var_os("RUST_YU_INTERNAL_BACKUP_HOLD_TRANSACTION").is_some() {
                    Some(
                        super::SessionLock::acquire(
                            &super::session_directory(&session_id).expect("会话目录"),
                        )
                        .expect("子进程占有清单事务锁"),
                    )
                } else {
                    None
                };
            fs::write(ready_path, b"locked").expect("操作锁就绪");
            wait_for_test_condition(|| go_path.exists(), "等待操作释放屏障超时");
            return;
        }
        let Ok(fixture) = std::env::var("RUST_YU_INTERNAL_BACKUP_CONCURRENCY_TEST") else {
            return;
        };
        let (session_id, trace_ids, ready_path, go_path): (String, Vec<String>, PathBuf, PathBuf) =
            serde_json::from_str(&fixture).expect("父测试传入有效夹具");
        fs::write(ready_path, b"ready").expect("子进程准备完成");
        wait_for_test_condition(|| go_path.exists(), "等待测试屏障超时");
        for trace_id in trace_ids {
            record_cleanup_result(&session_id, &trace_id, false, Some(trace_id.clone()))
                .expect("子进程更新结果应成功");
        }
    }

    #[test]
    fn concurrent_processes_preserve_every_cleanup_result() {
        with_storage_root("process-results", |root| {
            fs::create_dir_all(root).expect("测试目录可创建");
            let traces = (0..16)
                .map(|index| {
                    let path = root.join(format!("process-{index}.txt"));
                    fs::write(&path, b"fixture").expect("夹具可写入");
                    Trace::new(
                        "RustYu process fixture".to_string(),
                        TraceType::File,
                        path.to_string_lossy().into_owned(),
                    )
                })
                .collect::<Vec<_>>();
            let prepared = prepare_for_traces(&traces, "process results")
                .expect("备份成功")
                .expect("存在会话");
            let mut children = TestChildren(Vec::new());
            let go_path = root.join("go");
            for (index, batch) in traces.chunks(8).enumerate() {
                let fixture = serde_json::to_string(&(
                    &prepared.session_id,
                    batch.iter().map(|trace| &trace.id).collect::<Vec<_>>(),
                    root.join(format!("ready-{index}")),
                    &go_path,
                ))
                .expect("可序列化测试参数");
                children.0.push(
                    std::process::Command::new(std::env::current_exe().expect("测试程序路径"))
                        .args([
                            "--exact",
                            "modules::backup::tests::backup_result_child_process",
                        ])
                        .env("RUST_YU_INTERNAL_BACKUP_CONCURRENCY_TEST", fixture)
                        .spawn()
                        .expect("启动本次编译的测试程序"),
                );
            }
            wait_for_test_condition(
                || root.join("ready-0").exists() && root.join("ready-1").exists(),
                "子进程准备超时",
            );
            fs::write(&go_path, b"go").expect("同时释放两个子进程");
            children.wait_success();
            let session = super::get_session(&prepared.session_id).expect("清单可解析");
            assert_eq!(session.items.len(), traces.len());
            assert!(
                session
                    .items
                    .iter()
                    .all(|item| item.state == BackupItemState::DeleteFailed
                        && item.error.as_ref() == Some(&item.trace_id)),
                "跨进程不能丢更新"
            );
        });
    }

    #[test]
    fn concurrent_cleanup_results_preserve_every_item() {
        with_storage_root("concurrent-results", |root| {
            fs::create_dir_all(root).expect("测试目录可创建");
            let traces = (0..16)
                .map(|index| {
                    let path = root.join(format!("item-{index}.txt"));
                    fs::write(&path, b"fixture").expect("夹具可写入");
                    Trace::new(
                        "RustYu concurrency fixture".to_string(),
                        TraceType::File,
                        path.to_string_lossy().into_owned(),
                    )
                })
                .collect::<Vec<_>>();
            let prepared = prepare_for_traces(&traces, "parallel results")
                .expect("备份成功")
                .expect("存在会话");
            let barrier = std::sync::Barrier::new(traces.len());
            std::thread::scope(|scope| {
                let handles = traces
                    .iter()
                    .map(|trace| {
                        let session_id = &prepared.session_id;
                        let barrier = &barrier;
                        scope.spawn(move || {
                            barrier.wait();
                            record_cleanup_result(
                                session_id,
                                &trace.id,
                                false,
                                Some(trace.id.clone()),
                            )
                        })
                    })
                    .collect::<Vec<_>>();
                for handle in handles {
                    handle
                        .join()
                        .expect("线程不应 panic")
                        .expect("结果应完整持久化");
                }
            });
            let session = super::get_session(&prepared.session_id).expect("清单应可解析");
            assert_eq!(session.items.len(), traces.len());
            assert!(
                session.items.iter().all(|item| {
                    item.state == BackupItemState::DeleteFailed
                        && item.error.as_ref() == Some(&item.trace_id)
                }),
                "并发更新不能丢失其他项目的结果"
            );
        });
    }

    #[test]
    fn deletion_validation_rejects_corrupt_or_missing_file_payload() {
        with_storage_root("payload-integrity", |root| {
            fs::create_dir_all(root).expect("测试根目录应可创建");
            let source = root.join("source.txt");
            fs::write(&source, b"original").expect("测试源文件应可写入");
            let trace = Trace::new(
                "Demo".to_string(),
                TraceType::File,
                source.to_string_lossy().into_owned(),
            );
            let preparation = prepare_for_traces(std::slice::from_ref(&trace), "integrity")
                .expect("备份准备应成功")
                .expect("应有备份会话");
            let session = super::get_session(preparation.session_id()).expect("应能读取会话");
            let relative = session.items[0].payload.as_ref().expect("应有备份载荷");
            let payload = root
                .join("backups")
                .join(preparation.session_id())
                .join(relative);
            assert!(preparation.validate_trace(&trace).is_ok());

            // 同长度篡改、截断和丢失均不得凭“源文件未变化”被放行删除。
            for replacement in [Some(&b"tampered"[..]), Some(&b""[..]), None] {
                match replacement {
                    Some(bytes) => fs::write(&payload, bytes).expect("应能模拟载荷损坏"),
                    None => fs::remove_file(&payload).expect("应能模拟载荷丢失"),
                }
                assert!(
                    preparation.validate_trace(&trace).is_err(),
                    "备份载荷损坏或丢失时必须拒绝删除，即使源数据完全未变"
                );
                assert_eq!(fs::read(&source).expect("源数据应仍可读"), b"original");
            }
            fs::write(&payload, b"original").expect("应能恢复测试载荷");
            assert!(preparation.validate_trace(&trace).is_ok());

            // 仅移除本测试创建的源文件，验证损坏备份不会被复制回目标；
            // 修复载荷后，失败会话仍可重试并恢复原始字节。
            fs::remove_file(&source).expect("应能移除临时夹具源文件");
            fs::write(&payload, b"tampered").expect("应能模拟载荷损坏");
            let rejected = restore_session(preparation.session_id()).expect("应返回逐项失败");
            assert!(!rejected.success);
            assert!(!source.exists(), "完整性校验失败不得先写回损坏内容");
            fs::write(&payload, b"original").expect("应能修复临时载荷");
            let restored = restore_session(preparation.session_id()).expect("失败会话应可重试");
            assert!(restored.success);
            assert_eq!(fs::read(&source).expect("应恢复原始字节"), b"original");
        });
    }

    #[test]
    fn deletion_validation_rejects_directory_payload_missing_a_child() {
        with_storage_root("directory-payload-integrity", |root| {
            let source = root.join("source-directory");
            fs::create_dir_all(&source).expect("应能创建夹具目录");
            fs::write(source.join("keep.txt"), b"keep").expect("应能创建夹具文件");
            fs::write(source.join("lost.txt"), b"original").expect("应能创建夹具文件");
            let trace = Trace::new(
                "Demo".to_string(),
                TraceType::File,
                source.to_string_lossy().into_owned(),
            );
            let preparation = prepare_for_traces(std::slice::from_ref(&trace), "integrity")
                .expect("目录备份应成功")
                .expect("应有会话");
            let session = super::get_session(preparation.session_id()).expect("应有会话");
            let payload = root
                .join("backups")
                .join(preparation.session_id())
                .join(session.items[0].payload.as_ref().expect("应有载荷目录"));
            fs::remove_file(payload.join("lost.txt")).expect("仅移除备份中的测试子文件");
            assert!(preparation.validate_trace(&trace).is_err());
            assert_eq!(
                fs::read(source.join("lost.txt")).expect("源文件须保留"),
                b"original"
            );
        });
    }

    #[test]
    fn deletion_validation_rejects_incomplete_registry_payload() {
        with_storage_root("registry-payload-integrity", |root| {
            struct RegistryFixture(String);
            impl Drop for RegistryFixture {
                fn drop(&mut self) {
                    let _ = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER)
                        .delete_subkey_all(&self.0);
                }
            }
            let fixture = RegistryFixture(format!(
                "Software\\RustYu\\BackupIntegrityTest\\{}",
                uuid::Uuid::new_v4()
            ));
            let (key, _) = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER)
                .create_subkey(&fixture.0)
                .expect("应能创建 UUID 测试键");
            key.set_value("Keep", &"original").expect("应能写入测试值");
            let trace = Trace::new(
                "Demo".to_string(),
                TraceType::RegistryKey,
                format!("HKCU\\{}", fixture.0),
            );
            let value_trace = Trace::new(
                "Demo".to_string(),
                TraceType::RegistryValue,
                format!("HKCU\\{}\\Keep", fixture.0),
            );
            let preparation =
                prepare_for_traces(&[trace.clone(), value_trace.clone()], "integrity")
                    .expect("注册表备份应成功")
                    .expect("应有会话");
            let session = super::get_session(preparation.session_id()).expect("应有会话");
            let payload = root
                .join("backups")
                .join(preparation.session_id())
                .join(session.items[0].payload.as_ref().expect("应有注册表载荷"));
            let mut snapshot: serde_json::Value =
                serde_json::from_slice(&fs::read(&payload).expect("应能读取载荷"))
                    .expect("载荷应为 JSON");
            // 空树仍是有效 JSON，子集式校验会误报成功，必须比较完整指纹。
            snapshot["root"]["values"] = serde_json::json!([]);
            fs::write(&payload, serde_json::to_vec(&snapshot).expect("应能序列化"))
                .expect("应能模拟载荷丢项");
            assert!(preparation.validate_trace(&trace).is_err());
            assert_eq!(
                key.get_value::<String, _>("Keep").expect("源值须保留"),
                "original"
            );

            let value_payload = root
                .join("backups")
                .join(preparation.session_id())
                .join(session.items[1].payload.as_ref().expect("应有值载荷"));
            let original_value: serde_json::Value =
                serde_json::from_slice(&fs::read(&value_payload).expect("应能读取值载荷"))
                    .expect("应为 JSON");
            for damage in ["bytes", "type", "target"] {
                let mut damaged = original_value.clone();
                match damage {
                    "bytes" => damaged["value"]["bytes"] = serde_json::json!([0]),
                    "type" => {
                        damaged["value"]["value_type"] =
                            serde_json::json!(winreg::enums::REG_DWORD as u32)
                    }
                    _ => {
                        damaged["key_path"] = serde_json::json!(format!("{}\\Redirect", fixture.0))
                    }
                }
                fs::write(
                    &value_payload,
                    serde_json::to_vec(&damaged).expect("应能序列化"),
                )
                .expect("应能模拟值载荷损坏");
                assert!(
                    preparation.validate_trace(&value_trace).is_err(),
                    "{damage}"
                );
            }
            assert!(
                !restore_session(preparation.session_id())
                    .expect("应返回失败状态")
                    .success
            );
            assert_eq!(
                key.get_value::<String, _>("Keep").expect("源值须保留"),
                "original"
            );
            assert!(
                key.open_subkey("Redirect").is_err(),
                "损坏载荷不得先写入其他目标"
            );
        });
    }

    #[test]
    fn interrupted_file_restore_leaves_no_final_target_and_can_retry() {
        let root = temporary_root("interrupted-copy");
        fs::create_dir_all(&root).expect("测试目录应可创建");
        let destination = root.join("restored.bin");
        let mut failing = FailAfterFirstChunk {
            first_chunk: Some(b"partial"),
        };

        let error =
            copy_reader_create_new(&mut failing, &destination).expect_err("中断的恢复必须返回错误");
        assert!(error.to_string().contains("simulated interrupted restore"));
        assert!(!destination.exists());

        let mut retry = io::Cursor::new(b"complete".to_vec());
        copy_reader_create_new(&mut retry, &destination).expect("清理半成品后应允许重试");
        assert_eq!(
            fs::read(&destination).ok().as_deref(),
            Some(&b"complete"[..])
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn shared_container_value_plan_uses_ownership_review_not_critical_gate() {
        // 扫描器会把共享分支里的值标记 is_critical；备份计划必须走归属
        // 重校验而非临界项整段拒绝，否则应用私有值永远进不了备份
        let suffix = uuid::Uuid::new_v4().to_string();
        let base = format!(r"Software\rust-yu-test\{suffix}\Microsoft\Windows\CurrentVersion\Run");
        let hive = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER);
        let (key, _) = hive.create_subkey(&base).expect("应能创建测试 Run 容器键");
        key.set_value("DemoApp", &r#""C:\Program Files\Demo App\demo.exe""#)
            .expect("应能写入自有值");
        key.set_value("Stranger", &"totally different program")
            .expect("应能写入他人值");

        let mut owned = Trace::new(
            "Demo App".to_string(),
            TraceType::RegistryValue,
            format!(r"HKCU\{base}\DemoApp"),
        );
        owned.is_critical = true;
        let plan = plan_for_traces(std::slice::from_ref(&owned));
        assert_eq!(
            plan.eligible_count, 1,
            "归属明确的应用值即使被标记 critical 也应可备份"
        );

        let mut stranger = Trace::new(
            "Demo App".to_string(),
            TraceType::RegistryValue,
            format!(r"HKCU\{base}\Stranger"),
        );
        stranger.is_critical = true;
        let stranger_plan = plan_for_traces(std::slice::from_ref(&stranger));
        assert_eq!(
            stranger_plan.eligible_count, 0,
            "共享容器中他人的值不得进入备份计划"
        );

        // is_shared 硬否决同样作用于备份计划：仍被其他程序共用的候选
        // 即使归属明确也不可备份，且同键邻居值保持原样
        let mut shared_owned = Trace::new(
            "Demo App".to_string(),
            TraceType::RegistryValue,
            format!(r"HKCU\{base}\DemoApp"),
        );
        shared_owned.is_critical = true;
        shared_owned.is_shared = true;
        let shared_plan = plan_for_traces(std::slice::from_ref(&shared_owned));
        assert_eq!(
            shared_plan.eligible_count, 0,
            "is_shared=true 的候选不得进入备份计划"
        );
        assert!(
            key.get_raw_value("Stranger").is_ok(),
            "备份计划不得触碰同键邻居值"
        );
        assert!(
            key.get_raw_value("DemoApp").is_ok(),
            "被拒绝的候选本身也必须原样保留"
        );

        let _ = hive.delete_subkey_all(&base);
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
