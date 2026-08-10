//! 竞品路线图 F-14、F-18、F-19、F-20 的安全实现。
//!
//! 本模块只把可解释、可回滚的能力暴露给 UI：存量取证永远标注为事后推断；
//! 休眠只禁用与安装目录精确关联的启动项；基线导入只做差异比较；策略配置档
//! 只能收紧清理权限，不能绕过确认、关键路径检查或备份门禁。

use crate::modules::common::error::UninstallerError;
use crate::modules::health;
use crate::modules::lister::models::{InstallSourceSelector, InstalledProgram, ListProgramsQuery};
use crate::modules::scanner::models::{Confidence, Trace, TraceType};
use crate::modules::{lister, scanner, startup};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use uuid::Uuid;

const STORAGE_ENV: &str = "RUST_YU_STORAGE_DIR";
const FORENSICS_DIR: &str = "forensics";
const BASELINES_DIR: &str = "inventory-baselines";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceConfidence {
    High,
    Medium,
    Low,
    Unknown,
}

impl From<Confidence> for EvidenceConfidence {
    fn from(value: Confidence) -> Self {
        match value {
            Confidence::High => Self::High,
            Confidence::Medium => Self::Medium,
            Confidence::Low => Self::Low,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceRecord {
    pub id: String,
    pub category: String,
    pub target: String,
    pub source: String,
    pub observed_at: DateTime<Utc>,
    pub confidence: EvidenceConfidence,
    pub exists: Option<bool>,
    pub result: String,
    pub destructive_eligible: bool,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReconstructedEvidencePacket {
    pub id: String,
    pub schema_version: u32,
    pub generated_at: DateTime<Utc>,
    pub program: InstalledProgram,
    pub inference_notice: String,
    pub vendor: Option<String>,
    pub signature_status: String,
    pub signature_subject: Option<String>,
    pub evidence: Vec<EvidenceRecord>,
    pub warnings: Vec<String>,
    pub immutable_sha256: String,
}

/// F-14：对未被监控的软件建立事后证据包。扫描结果绝不因数量或名称相似而
/// 自动提升置信度，低置信度和未知项默认不可用于破坏性操作。
pub async fn reconstruct_installation(
    program: InstalledProgram,
) -> Result<ReconstructedEvidencePacket, UninstallerError> {
    let mut warnings = Vec::new();
    let mut traces = scanner::scan_all_traces_for_program(&program, None).await?;
    traces.sort_by(|left, right| left.path.cmp(&right.path));
    let observed_at = Utc::now();
    let mut evidence = Vec::new();

    if let Some(key) = program.uninstall_registry_key_path.as_deref() {
        evidence.push(record(
            "uninstall_registry",
            key,
            "installed_program.uninstall_registry_key_path",
            EvidenceConfidence::High,
            Some(true),
            true,
            "卸载清单中的精确注册表键；存在性由清单扫描来源证明。",
        ));
    } else {
        warnings.push("程序清单没有卸载注册表键路径。".to_string());
    }
    if let Some(path) = program.install_location.as_deref() {
        let exists = Path::new(path).exists();
        evidence.push(record(
            "install_directory",
            path,
            "installed_program.install_location",
            EvidenceConfidence::High,
            Some(exists),
            exists,
            "安装清单中的精确目录；删除仍需单独执行备份门禁。",
        ));
    }

    for trace in traces {
        let confidence = EvidenceConfidence::from(trace.confidence);
        evidence.push(record(
            trace_category(trace.trace_type),
            &trace.path,
            "post_hoc_scanner",
            confidence,
            Some(trace.exists),
            confidence == EvidenceConfidence::High && !trace.is_critical,
            &format!("事后推断：{}", trace.description),
        ));
    }

    let signature_location = program.install_location.clone();
    let signature_icon = program.icon_path.clone();
    let (signature_status, signature_subject, signature_warning) =
        tokio::task::spawn_blocking(move || {
            inspect_signature(signature_location.as_deref(), signature_icon.as_deref())
        })
        .await
        .map_err(|error| UninstallerError::Other(format!("签名检查任务失败：{error}")))?;
    if let Some(warning) = signature_warning {
        warnings.push(warning);
    }

    let mut packet = ReconstructedEvidencePacket {
        id: Uuid::new_v4().to_string(),
        schema_version: 1,
        generated_at: observed_at,
        vendor: program.publisher.clone(),
        program,
        inference_notice:
            "此证据包来自安装后的只读重建，不等同于安装时监控；低置信度或未知项目保持保留状态。"
                .to_string(),
        signature_status,
        signature_subject,
        evidence,
        warnings,
        immutable_sha256: String::new(),
    };
    packet.immutable_sha256 = packet_hash(&packet)?;
    persist_json(FORENSICS_DIR, &packet.id, &packet)?;
    Ok(packet)
}

fn record(
    category: &str,
    target: &str,
    source: &str,
    confidence: EvidenceConfidence,
    exists: Option<bool>,
    destructive_eligible: bool,
    note: &str,
) -> EvidenceRecord {
    EvidenceRecord {
        id: Uuid::new_v4().to_string(),
        category: category.to_string(),
        target: target.to_string(),
        source: source.to_string(),
        observed_at: Utc::now(),
        confidence,
        exists,
        result: if exists == Some(false) {
            "missing"
        } else {
            "observed"
        }
        .to_string(),
        destructive_eligible,
        note: note.to_string(),
    }
}

fn trace_category(trace_type: TraceType) -> &'static str {
    match trace_type {
        TraceType::RegistryKey | TraceType::RegistryValue => "registry",
        TraceType::File | TraceType::Shortcut => "filesystem",
        TraceType::AppData => "appdata",
        TraceType::ScheduledTask => "scheduled_task",
        TraceType::Service => "service",
        TraceType::Driver => "driver",
    }
}

fn inspect_signature(
    install_location: Option<&str>,
    icon_path: Option<&str>,
) -> (String, Option<String>, Option<String>) {
    let candidate = icon_path
        .map(PathBuf::from)
        .filter(|path| path.is_file())
        .or_else(|| {
            install_location.and_then(|location| {
                fs::read_dir(location)
                    .ok()?
                    .filter_map(Result::ok)
                    .map(|entry| entry.path())
                    .find(|path| {
                        path.extension()
                            .is_some_and(|ext| ext.eq_ignore_ascii_case("exe"))
                    })
            })
        });
    let Some(path) = candidate else {
        return (
            "unknown".to_string(),
            None,
            Some("没有找到可读取签名的程序文件。".to_string()),
        );
    };
    let escaped = path.to_string_lossy().replace('\'', "''");
    let script = format!(
        "$s=Get-AuthenticodeSignature -LiteralPath '{}'; [pscustomobject]@{{Status=[string]$s.Status;Subject=if($s.SignerCertificate){{$s.SignerCertificate.Subject}}else{{$null}}}} | ConvertTo-Json -Compress",
        escaped
    );
    match Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
    {
        Ok(output) if output.status.success() => {
            let parsed = serde_json::from_slice::<serde_json::Value>(&output.stdout).ok();
            let status = parsed
                .as_ref()
                .and_then(|value| value["Status"].as_str())
                .unwrap_or("Unknown");
            let subject = parsed
                .as_ref()
                .and_then(|value| value["Subject"].as_str())
                .map(str::to_string);
            (status.to_ascii_lowercase(), subject, None)
        }
        Ok(output) => (
            "unknown".to_string(),
            None,
            Some(format!("签名读取失败，退出码 {:?}。", output.status.code())),
        ),
        Err(error) => (
            "unknown".to_string(),
            None,
            Some(format!("无法启动签名检查：{error}")),
        ),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CleanupPolicyKind {
    Audit,
    Safe,
    Recovery,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupPolicyProfile {
    pub kind: CleanupPolicyKind,
    pub title: String,
    pub description: String,
    pub analyze_only: bool,
    pub require_confirmation: bool,
    pub require_backup: bool,
    pub allowed_confidence: Vec<EvidenceConfidence>,
    pub allowed_actions: Vec<String>,
    pub irreversible_actions: Vec<String>,
}

pub fn cleanup_profiles() -> Vec<CleanupPolicyProfile> {
    vec![
        CleanupPolicyProfile {
            kind: CleanupPolicyKind::Audit,
            title: "审计".to_string(),
            description: "只分析和导出证据，不修改系统。".to_string(),
            analyze_only: true,
            require_confirmation: true,
            require_backup: false,
            allowed_confidence: Vec::new(),
            allowed_actions: vec!["scan".to_string(), "export".to_string()],
            irreversible_actions: Vec::new(),
        },
        CleanupPolicyProfile {
            kind: CleanupPolicyKind::Safe,
            title: "安全清理".to_string(),
            description: "只允许高置信度、非关键且已成功备份的目标。".to_string(),
            analyze_only: false,
            require_confirmation: true,
            require_backup: true,
            allowed_confidence: vec![EvidenceConfidence::High],
            allowed_actions: vec![
                "backup".to_string(),
                "delete_after_confirmation".to_string(),
            ],
            irreversible_actions: Vec::new(),
        },
        CleanupPolicyProfile {
            kind: CleanupPolicyKind::Recovery,
            title: "恢复优先".to_string(),
            description: "仅恢复历史会话；不执行新的删除。".to_string(),
            analyze_only: false,
            require_confirmation: true,
            require_backup: false,
            allowed_confidence: Vec::new(),
            allowed_actions: vec![
                "list_backups".to_string(),
                "restore".to_string(),
                "retry_restore".to_string(),
            ],
            irreversible_actions: Vec::new(),
        },
    ]
}

pub fn validate_cleanup_selection(
    kind: CleanupPolicyKind,
    traces: &[Trace],
    confirmed: bool,
) -> Result<(), UninstallerError> {
    if !confirmed {
        return Err(UninstallerError::Other(
            "清理策略不能绕过用户确认。".to_string(),
        ));
    }
    match kind {
        CleanupPolicyKind::Audit => {
            Err(UninstallerError::Other("审计配置档禁止删除。".to_string()))
        }
        CleanupPolicyKind::Recovery => Err(UninstallerError::Other(
            "恢复优先配置档禁止创建新的删除操作。".to_string(),
        )),
        CleanupPolicyKind::Safe => {
            if traces.is_empty() {
                return Err(UninstallerError::Other("没有选择清理目标。".to_string()));
            }
            if traces
                .iter()
                .any(|trace| trace.confidence != Confidence::High || trace.is_critical)
            {
                return Err(UninstallerError::Other(
                    "安全清理仅允许高置信度、非关键目标。".to_string(),
                ));
            }
            Ok(())
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HibernationCandidate {
    pub item_id: String,
    pub name: String,
    pub source: String,
    pub command: Option<String>,
    pub association: EvidenceConfidence,
    pub reversible: bool,
    pub prohibited: bool,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HibernationPlan {
    pub id: String,
    pub program: InstalledProgram,
    pub created_at: DateTime<Utc>,
    pub last_used: Option<String>,
    pub times_used: Option<u32>,
    pub candidates: Vec<HibernationCandidate>,
    pub selected_item_ids: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HibernationResult {
    pub plan_id: String,
    pub applied: bool,
    pub change_ids: Vec<String>,
    pub errors: Vec<String>,
}

pub fn plan_hibernation(program: InstalledProgram) -> Result<HibernationPlan, UninstallerError> {
    let response = startup::manager::list_startup_items(Default::default())
        .map_err(|error| UninstallerError::Other(error.to_string()))?;
    let install_root = program.install_location.as_deref().map(normalize_path);
    let mut usage_snapshots = health::load_usage_snapshots(std::slice::from_ref(&program));
    let usage = usage_snapshots.remove(&program.id).unwrap_or_default();
    let mut warnings = Vec::new();
    if install_root.is_none() {
        warnings.push("程序没有可验证的安装目录，因此不会自动选择任何休眠项。".to_string());
    }
    let mut candidates = Vec::new();
    let mut selected_item_ids = Vec::new();
    for item in response.items {
        let associated_path = item.executable_path.as_deref().or(item.command.as_deref());
        let exact = install_root.as_ref().is_some_and(|root| {
            associated_path.is_some_and(|path| normalize_path(path).starts_with(root))
        });
        let system_path =
            associated_path.is_some_and(crate::modules::common::utils::is_system_critical_path);
        let prohibited = system_path || !exact;
        let reversible = item.capabilities.can_disable && item.capabilities.can_rollback;
        if exact && reversible && !prohibited {
            selected_item_ids.push(item.id.clone());
        }
        candidates.push(HibernationCandidate {
            item_id: item.id,
            name: item.name,
            source: item.source.as_str().to_string(),
            command: item.command,
            association: if exact {
                EvidenceConfidence::High
            } else {
                EvidenceConfidence::Unknown
            },
            reversible,
            prohibited,
            reason: if system_path {
                "系统关键路径，禁止休眠。".to_string()
            } else if !exact {
                "无法证明该项位于程序安装目录，默认保留。".to_string()
            } else if !reversible {
                "该启动源不支持可验证回滚。".to_string()
            } else {
                "与安装目录精确关联，可生成可回滚禁用操作。".to_string()
            },
        });
    }
    Ok(HibernationPlan {
        id: Uuid::new_v4().to_string(),
        program,
        created_at: Utc::now(),
        last_used: usage.last_used,
        times_used: usage.times_used,
        candidates,
        selected_item_ids,
        warnings,
    })
}

pub fn apply_hibernation(
    plan: &HibernationPlan,
    item_ids: &[String],
    confirmed: bool,
) -> Result<HibernationResult, UninstallerError> {
    if !confirmed {
        return Err(UninstallerError::Other("休眠前需要明确确认。".to_string()));
    }
    let allowed = plan.selected_item_ids.iter().collect::<HashSet<_>>();
    if item_ids.iter().any(|id| !allowed.contains(id)) {
        return Err(UninstallerError::Other(
            "休眠选择包含未通过影响分析的项目。".to_string(),
        ));
    }
    let mut change_ids = Vec::new();
    let mut errors = Vec::new();
    for id in item_ids {
        match startup::manager::apply_action(
            id,
            startup::models::StartupAction::Disable,
            Some(format!("Rust Yu 安全休眠：{}", plan.program.name)),
        ) {
            Ok(result) => {
                if let Some(change_id) = result.change_id {
                    change_ids.push(change_id);
                }
            }
            Err(error) => errors.push(format!("{id}: {error}")),
        }
    }
    Ok(HibernationResult {
        plan_id: plan.id.clone(),
        applied: errors.is_empty(),
        change_ids,
        errors,
    })
}

pub fn wake_hibernation(
    change_ids: &[String],
    confirmed: bool,
) -> Result<HibernationResult, UninstallerError> {
    if !confirmed {
        return Err(UninstallerError::Other("唤醒前需要明确确认。".to_string()));
    }
    let mut errors = Vec::new();
    for change_id in change_ids {
        if let Err(error) =
            startup::manager::rollback_action(change_id, Some("Rust Yu 唤醒软件".to_string()))
        {
            errors.push(format!("{change_id}: {error}"));
        }
    }
    Ok(HibernationResult {
        plan_id: "wake".to_string(),
        applied: errors.is_empty(),
        change_ids: change_ids.to_vec(),
        errors,
    })
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventoryEntry {
    pub program_id: String,
    pub name: String,
    pub publisher: Option<String>,
    pub version: Option<String>,
    pub source: String,
    pub install_location: Option<String>,
    pub uninstall_capability: String,
    pub signature_status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventoryBaseline {
    pub id: String,
    pub schema_version: u32,
    pub captured_at: DateTime<Utc>,
    pub machine_label: String,
    pub entries: Vec<InventoryEntry>,
    pub sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventoryDifference {
    pub key: String,
    pub name: String,
    pub status: String,
    pub baseline_version: Option<String>,
    pub current_version: Option<String>,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventoryComparison {
    pub compared_at: DateTime<Utc>,
    pub baseline_id: String,
    pub baseline_captured_at: DateTime<Utc>,
    pub differences: Vec<InventoryDifference>,
    pub read_only_notice: String,
}

pub fn create_inventory_baseline(
    machine_label: String,
) -> Result<InventoryBaseline, UninstallerError> {
    let response = lister::list_programs_with_cache(ListProgramsQuery {
        source: InstallSourceSelector::All,
        search: None,
        refresh: true,
        cache_ttl_seconds: 0,
    })?;
    baseline_from_programs(machine_label, response.programs)
}

pub fn baseline_from_programs(
    machine_label: String,
    programs: Vec<InstalledProgram>,
) -> Result<InventoryBaseline, UninstallerError> {
    let mut entries = programs
        .into_iter()
        .map(|program| {
            let uninstall_capability = if program.preferred_uninstall_string().is_some() {
                "available"
            } else {
                "missing"
            }
            .to_string();
            let (signature_status, _, _) = inspect_signature(
                program.install_location.as_deref(),
                program.icon_path.as_deref(),
            );
            InventoryEntry {
                program_id: program.id,
                name: program.name,
                publisher: program.publisher,
                version: program.display_version.or(program.version),
                source: program.install_source.to_string(),
                install_location: program.install_location,
                uninstall_capability,
                signature_status,
            }
        })
        .collect::<Vec<_>>();
    entries.sort_by(|a, b| {
        normalized_key(&a.name, a.publisher.as_deref())
            .cmp(&normalized_key(&b.name, b.publisher.as_deref()))
    });
    let mut baseline = InventoryBaseline {
        id: Uuid::new_v4().to_string(),
        schema_version: 1,
        captured_at: Utc::now(),
        machine_label: machine_label.trim().to_string(),
        entries,
        sha256: String::new(),
    };
    baseline.sha256 = baseline_hash(&baseline)?;
    Ok(baseline)
}

pub fn save_inventory_baseline(baseline: &InventoryBaseline) -> Result<String, UninstallerError> {
    if baseline_hash(baseline)? != baseline.sha256 {
        return Err(UninstallerError::Other(
            "基线 SHA-256 校验失败。".to_string(),
        ));
    }
    Ok(persist_json(BASELINES_DIR, &baseline.id, baseline)?
        .to_string_lossy()
        .to_string())
}

pub fn compare_inventory(
    baseline: &InventoryBaseline,
    current_programs: &[InstalledProgram],
) -> Result<InventoryComparison, UninstallerError> {
    if baseline_hash(baseline)? != baseline.sha256 {
        return Err(UninstallerError::Other(
            "基线内容与 SHA-256 不一致，拒绝比较。".to_string(),
        ));
    }
    let baseline_map = baseline
        .entries
        .iter()
        .map(|entry| {
            (
                normalized_key(&entry.name, entry.publisher.as_deref()),
                entry,
            )
        })
        .collect::<HashMap<_, _>>();
    let current_map = current_programs
        .iter()
        .map(|program| {
            (
                normalized_key(&program.name, program.publisher.as_deref()),
                program,
            )
        })
        .collect::<HashMap<_, _>>();
    let mut keys = baseline_map
        .keys()
        .chain(current_map.keys())
        .cloned()
        .collect::<HashSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    keys.sort();
    let mut differences = Vec::new();
    for key in keys {
        match (baseline_map.get(&key), current_map.get(&key)) {
            (Some(old), None) => differences.push(InventoryDifference {
                key,
                name: old.name.clone(),
                status: "missing".to_string(),
                baseline_version: old.version.clone(),
                current_version: None,
                note: "当前系统未发现；不会自动下载或安装。".to_string(),
            }),
            (None, Some(new)) => differences.push(InventoryDifference {
                key,
                name: new.name.clone(),
                status: "added".to_string(),
                baseline_version: None,
                current_version: new.display_version.clone().or(new.version.clone()),
                note: "当前系统新增；不会自动卸载。".to_string(),
            }),
            (Some(old), Some(new)) => {
                let current_version = new.display_version.clone().or(new.version.clone());
                if old.version != current_version {
                    differences.push(InventoryDifference {
                        key,
                        name: new.name.clone(),
                        status: "version_changed".to_string(),
                        baseline_version: old.version.clone(),
                        current_version,
                        note: "版本不同，仅供人工迁移核对。".to_string(),
                    });
                }
            }
            (None, None) => {}
        }
    }
    Ok(InventoryComparison {
        compared_at: Utc::now(),
        baseline_id: baseline.id.clone(),
        baseline_captured_at: baseline.captured_at,
        differences,
        read_only_notice: "迁移对比仅生成差异，不执行下载、安装、卸载或系统修改。".to_string(),
    })
}

pub fn load_inventory_baseline(path: &Path) -> Result<InventoryBaseline, UninstallerError> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err(UninstallerError::Other(
            "基线必须是普通 JSON 文件。".to_string(),
        ));
    }
    let baseline: InventoryBaseline = serde_json::from_slice(&fs::read(path)?)
        .map_err(|error| UninstallerError::Serde(error.to_string()))?;
    if baseline_hash(&baseline)? != baseline.sha256 {
        return Err(UninstallerError::Other(
            "基线 SHA-256 校验失败。".to_string(),
        ));
    }
    Ok(baseline)
}

fn packet_hash(packet: &ReconstructedEvidencePacket) -> Result<String, UninstallerError> {
    let mut clone = packet.clone();
    clone.immutable_sha256.clear();
    hash_json(&clone)
}

fn baseline_hash(baseline: &InventoryBaseline) -> Result<String, UninstallerError> {
    let mut clone = baseline.clone();
    clone.sha256.clear();
    hash_json(&clone)
}

fn hash_json<T: Serialize>(value: &T) -> Result<String, UninstallerError> {
    let bytes =
        serde_json::to_vec(value).map_err(|error| UninstallerError::Serde(error.to_string()))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn normalized_key(name: &str, publisher: Option<&str>) -> String {
    format!(
        "{}|{}",
        name.trim().to_lowercase(),
        publisher.unwrap_or_default().trim().to_lowercase()
    )
}

fn normalize_path(value: &str) -> String {
    value
        .trim_matches([' ', '"'])
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_lowercase()
}

fn storage_root() -> Result<PathBuf, UninstallerError> {
    if let Ok(path) = std::env::var(STORAGE_ENV) {
        if !path.trim().is_empty() {
            return Ok(PathBuf::from(path));
        }
    }
    dirs::data_local_dir()
        .map(|path| path.join("rust-yu"))
        .ok_or_else(|| UninstallerError::Other("无法确定本机应用数据目录。".to_string()))
}

fn persist_json<T: Serialize>(
    directory: &str,
    id: &str,
    value: &T,
) -> Result<PathBuf, UninstallerError> {
    if !id
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || character == '-')
    {
        return Err(UninstallerError::Other("持久化标识无效。".to_string()));
    }
    let root = storage_root()?.join(directory);
    fs::create_dir_all(&root)?;
    let metadata = fs::symlink_metadata(&root)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(UninstallerError::Other("存储目录不安全。".to_string()));
    }
    let path = root.join(format!("{id}.json"));
    fs::write(
        &path,
        serde_json::to_vec_pretty(value)
            .map_err(|error| UninstallerError::Serde(error.to_string()))?,
    )?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::lister::models::InstallSource;

    fn trace(confidence: Confidence, critical: bool) -> Trace {
        let mut trace = Trace::new("Demo".to_string(), TraceType::File, r"C:\Demo".to_string());
        trace.confidence = confidence;
        trace.is_critical = critical;
        trace
    }

    #[test]
    fn cleanup_profiles_never_expose_fix_all_or_irreversible_actions() {
        for profile in cleanup_profiles() {
            assert!(profile.require_confirmation);
            assert!(profile.irreversible_actions.is_empty());
            assert!(!profile
                .allowed_actions
                .iter()
                .any(|action| action == "fix_all"));
        }
    }

    #[test]
    fn safe_policy_rejects_medium_and_critical_targets() {
        assert!(validate_cleanup_selection(
            CleanupPolicyKind::Safe,
            &[trace(Confidence::High, false)],
            true
        )
        .is_ok());
        assert!(validate_cleanup_selection(
            CleanupPolicyKind::Safe,
            &[trace(Confidence::Medium, false)],
            true
        )
        .is_err());
        assert!(validate_cleanup_selection(
            CleanupPolicyKind::Safe,
            &[trace(Confidence::High, true)],
            true
        )
        .is_err());
    }

    #[test]
    fn inventory_comparison_is_read_only_and_detects_missing_added_and_version_changes() {
        let mut old = InstalledProgram::new("Old".to_string(), InstallSource::Registry);
        old.publisher = Some("Vendor".to_string());
        old.version = Some("1".to_string());
        let mut changed = InstalledProgram::new("Changed".to_string(), InstallSource::Registry);
        changed.version = Some("1".to_string());
        let baseline = baseline_from_programs("test".to_string(), vec![old, changed.clone()])
            .expect("baseline");
        changed.version = Some("2".to_string());
        let added = InstalledProgram::new("Added".to_string(), InstallSource::Registry);
        let comparison = compare_inventory(&baseline, &[changed, added]).expect("comparison");
        assert!(comparison
            .differences
            .iter()
            .any(|item| item.status == "missing"));
        assert!(comparison
            .differences
            .iter()
            .any(|item| item.status == "added"));
        assert!(comparison
            .differences
            .iter()
            .any(|item| item.status == "version_changed"));
        assert!(comparison.read_only_notice.contains("不执行"));
    }

    #[test]
    fn tampered_baseline_is_rejected() {
        let program = InstalledProgram::new("Demo".to_string(), InstallSource::Registry);
        let mut baseline =
            baseline_from_programs("test".to_string(), vec![program]).expect("baseline");
        baseline.machine_label = "tampered".to_string();
        assert!(compare_inventory(&baseline, &[]).is_err());
    }
}
