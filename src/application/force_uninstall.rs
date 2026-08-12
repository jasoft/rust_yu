use super::uninstall::error::{UninstallError, UninstallErrorCode};
use crate::modules::cleaner;
use crate::modules::cleaner::models::CleanResult;
use crate::modules::common::utils;
use crate::modules::scanner::models::{Confidence, Trace, TraceType};
use crate::modules::scanner::registry;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use walkdir::WalkDir;

const MAX_REGISTRY_CANDIDATES: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ForceTargetKind {
    Directory,
    Executable,
    Shortcut,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForceUninstallTarget {
    pub input_path: String,
    pub resolved_path: String,
    pub name: String,
    pub kind: ForceTargetKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForceUninstallPlan {
    pub plan_id: String,
    pub target: ForceUninstallTarget,
    pub fingerprint: String,
    pub traces: Vec<Trace>,
    pub default_selected_ids: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForceCleanupSelection {
    pub plan_id: String,
    pub trace_ids: Vec<String>,
    pub confirm: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForceUninstallResult {
    pub plan_id: String,
    pub success: bool,
    pub message: String,
    pub traces_found: usize,
    pub traces_cleaned: usize,
    pub failed_count: usize,
    pub bytes_freed: u64,
    pub outcomes: Vec<CleanResult>,
}

/// 生成强制卸载计划。
///
/// 这里故意只把用户明确提供的安装目录作为高置信度目标；注册表候选仅作
/// 低风险的辅助审查项。强制模式不运行未知命令，也不会因为名称相似就删除
/// 用户目录中的任意文件。
pub fn plan_force_uninstall(
    input_path: &str,
    display_name: Option<&str>,
) -> Result<ForceUninstallPlan, UninstallError> {
    let (target, root) = resolve_target(input_path, display_name)?;
    let target_name = target.name.clone();
    let mut traces = vec![build_root_trace(&target_name, &root)];
    let mut seen_registry_paths = HashSet::new();

    for search_name in search_names(&target_name, &root) {
        let candidates = match registry::scan_registry_traces(&search_name) {
            Ok(candidates) => candidates,
            Err(error) => {
                tracing::debug!("强制卸载计划跳过注册表候选扫描 {}: {}", search_name, error);
                continue;
            }
        };

        for mut candidate in candidates {
            if !is_relevant_registry_candidate(&candidate, &target_name, &root) {
                continue;
            }

            let normalized_path = candidate.path.to_lowercase();
            if !seen_registry_paths.insert(normalized_path) {
                continue;
            }

            candidate.id = stable_trace_id(candidate.trace_type, &candidate.path);
            candidate.program_name = target_name.clone();
            candidate.confidence = Confidence::Medium;
            candidate.description = format!(
                "可能属于 {} 的注册表候选；强制模式不会默认删除",
                target_name
            );
            candidate.is_critical =
                candidate.is_critical || utils::is_critical_registry_path(&candidate.path);
            traces.push(candidate);

            if traces.len() >= MAX_REGISTRY_CANDIDATES + 1 {
                break;
            }
        }

        if traces.len() >= MAX_REGISTRY_CANDIDATES + 1 {
            break;
        }
    }

    traces.sort_by(|left, right| {
        right
            .confidence
            .cmp(&left.confidence)
            .then_with(|| left.path.to_lowercase().cmp(&right.path.to_lowercase()))
    });

    let mut warnings = vec![
        "强制卸载不会运行原厂卸载器，只处理你在本页明确确认的目标。".to_string(),
        "注册表候选是启发式结果，默认保留；共享组件和系统项不要删除。".to_string(),
    ];
    if traces.len() == 1 {
        warnings.push("只发现了用户提供的目标目录，没有发现可供审核的注册表候选。".to_string());
    }

    Ok(ForceUninstallPlan {
        plan_id: uuid::Uuid::new_v4().to_string(),
        target,
        fingerprint: fingerprint_target(&root)?,
        traces,
        default_selected_ids: Vec::new(),
        warnings,
    })
}

/// 对用户提交的计划重新扫描并验证，然后清理选中的精确目标。
///
/// 计划对象来自前端，不能直接当作授权凭据；执行前必须重新解析路径、重新
/// 计算指纹，并检查每个 trace id 是否仍属于当前计划。
pub async fn clean_force_uninstall(
    plan: &ForceUninstallPlan,
    selection: ForceCleanupSelection,
) -> Result<ForceUninstallResult, UninstallError> {
    if selection.plan_id != plan.plan_id {
        return Err(UninstallError::new(
            UninstallErrorCode::TargetChanged,
            "强制卸载计划已变化，请重新扫描目标",
        ));
    }
    if !selection.confirm {
        return Err(UninstallError::new(
            UninstallErrorCode::ConfirmationRequired,
            "强制卸载必须明确确认清理范围",
        ));
    }
    if selection.trace_ids.is_empty() {
        return Err(UninstallError::new(
            UninstallErrorCode::NoTraceSelected,
            "至少选择一个明确的目标后才能执行强制卸载",
        ));
    }

    let input_path = plan.target.input_path.clone();
    let display_name = plan.target.name.clone();
    let expected_fingerprint = plan.fingerprint.clone();
    let current_plan =
        tokio::task::spawn_blocking(move || plan_force_uninstall(&input_path, Some(&display_name)))
            .await
            .map_err(|error| {
                UninstallError::new(
                    UninstallErrorCode::ResidueScanFailed,
                    format!("重新扫描强制卸载目标失败: {error}"),
                )
            })??;

    if current_plan.fingerprint != expected_fingerprint {
        return Err(UninstallError::new(
            UninstallErrorCode::TargetChanged,
            "目标目录在确认后发生变化，请重新扫描再执行",
        ));
    }

    let selected = select_revalidated_traces(plan, &current_plan, &selection.trace_ids)?;
    let traces_found = current_plan.traces.len();
    let outcomes = cleaner::clean_traces(selected, true)
        .await
        .map_err(|error| {
            UninstallError::new(UninstallErrorCode::CleanupFailed, error.to_string())
        })?;
    let traces_cleaned = outcomes.iter().filter(|outcome| outcome.success).count();
    let failed_count = outcomes.len().saturating_sub(traces_cleaned);
    let bytes_freed = outcomes
        .iter()
        .filter(|outcome| outcome.success)
        .map(|outcome| outcome.bytes_freed)
        .sum();

    Ok(ForceUninstallResult {
        plan_id: plan.plan_id.clone(),
        success: failed_count == 0,
        message: if failed_count == 0 {
            format!("强制卸载完成，已处理 {traces_cleaned} 项")
        } else {
            format!("强制卸载部分完成，成功 {traces_cleaned} 项，失败 {failed_count} 项")
        },
        traces_found,
        traces_cleaned,
        failed_count,
        bytes_freed,
        outcomes,
    })
}

fn select_revalidated_traces(
    original_plan: &ForceUninstallPlan,
    current_plan: &ForceUninstallPlan,
    trace_ids: &[String],
) -> Result<Vec<Trace>, UninstallError> {
    let mut selected = Vec::with_capacity(trace_ids.len());
    let mut seen = HashSet::new();

    for trace_id in trace_ids {
        if !seen.insert(trace_id) {
            return Err(UninstallError::new(
                UninstallErrorCode::TraceNotInPlan,
                "强制卸载目标包含重复项目",
            ));
        }
        if !original_plan
            .traces
            .iter()
            .any(|trace| trace.id == *trace_id)
        {
            return Err(UninstallError::new(
                UninstallErrorCode::TraceNotInPlan,
                "强制卸载目标不属于原始计划",
            ));
        }
        let trace = current_plan
            .traces
            .iter()
            .find(|trace| trace.id == *trace_id)
            .cloned()
            .ok_or_else(|| {
                UninstallError::new(
                    UninstallErrorCode::TargetChanged,
                    "强制卸载目标在执行前已不存在或发生变化",
                )
            })?;
        selected.push(trace);
    }

    for (index, left) in selected.iter().enumerate() {
        for right in selected.iter().skip(index + 1) {
            if left.trace_type == TraceType::File
                && right.trace_type == TraceType::File
                && paths_overlap(Path::new(&left.path), Path::new(&right.path))
            {
                return Err(UninstallError::new(
                    UninstallErrorCode::TraceNotInPlan,
                    "不能同时选择相互包含的文件目标",
                ));
            }
        }
    }

    Ok(selected)
}

fn resolve_target(
    input_path: &str,
    display_name: Option<&str>,
) -> Result<(ForceUninstallTarget, PathBuf), UninstallError> {
    let trimmed = input_path.trim();
    if trimmed.is_empty() {
        return Err(UninstallError::new(
            UninstallErrorCode::ForceTargetInvalid,
            "请输入程序目录、可执行文件或快捷方式路径",
        ));
    }

    let input = canonicalize_path(Path::new(trimmed)).map_err(|error| {
        UninstallError::new(
            UninstallErrorCode::ForceTargetNotFound,
            format!("找不到强制卸载目标: {error}"),
        )
    })?;
    let input_kind = if input.is_dir() {
        ForceTargetKind::Directory
    } else if input.is_file() {
        match input.extension().and_then(|extension| extension.to_str()) {
            Some(extension) if extension.eq_ignore_ascii_case("lnk") => ForceTargetKind::Shortcut,
            Some(extension)
                if ["exe", "com", "bat", "cmd", "msi", "scr"]
                    .iter()
                    .any(|allowed| extension.eq_ignore_ascii_case(allowed)) =>
            {
                ForceTargetKind::Executable
            }
            _ => {
                return Err(UninstallError::new(
                    UninstallErrorCode::ForceTargetInvalid,
                    "只能选择程序目录、EXE/安装程序或 Windows 快捷方式",
                ));
            }
        }
    } else {
        return Err(UninstallError::new(
            UninstallErrorCode::ForceTargetInvalid,
            "强制卸载目标不是可用的文件或目录",
        ));
    };

    let resolved_entry = if input_kind == ForceTargetKind::Shortcut {
        resolve_shortcut_target(&input)?
    } else {
        input.clone()
    };
    let root = if resolved_entry.is_dir() {
        resolved_entry.clone()
    } else {
        resolved_entry
            .parent()
            .map(Path::to_path_buf)
            .ok_or_else(|| {
                UninstallError::new(
                    UninstallErrorCode::ForceTargetInvalid,
                    "无法确定程序所在目录",
                )
            })?
    };
    validate_target_root(&root)?;

    let derived_name =
        if let Some(name) = display_name.map(str::trim).filter(|name| !name.is_empty()) {
            if name.contains(['\\', '/']) {
                return Err(UninstallError::new(
                    UninstallErrorCode::ForceTargetInvalid,
                    "程序名称不能包含路径分隔符",
                ));
            }
            name.to_string()
        } else if !resolved_entry.is_dir() {
            resolved_entry
                .file_stem()
                .and_then(|stem| stem.to_str())
                .filter(|name| !name.trim().is_empty())
                .map(str::to_string)
                .ok_or_else(|| {
                    UninstallError::new(
                        UninstallErrorCode::ForceTargetInvalid,
                        "无法从程序路径推断名称，请手动输入名称",
                    )
                })?
        } else {
            root.file_name()
                .and_then(|name| name.to_str())
                .filter(|name| !name.trim().is_empty())
                .map(str::to_string)
                .ok_or_else(|| {
                    UninstallError::new(
                        UninstallErrorCode::ForceTargetInvalid,
                        "无法从目录路径推断名称，请手动输入名称",
                    )
                })?
        };

    let target = ForceUninstallTarget {
        input_path: input.to_string_lossy().to_string(),
        resolved_path: root.to_string_lossy().to_string(),
        name: derived_name,
        kind: input_kind,
    };
    Ok((target, root))
}

fn resolve_shortcut_target(path: &Path) -> Result<PathBuf, UninstallError> {
    let escaped_path = path.to_string_lossy().replace('\'', "''");
    let script = format!(
        "$shell = New-Object -ComObject WScript.Shell; $shortcut = $shell.CreateShortcut('{escaped_path}'); $shortcut.TargetPath"
    );
    let output = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .output()
        .map_err(|error| {
            UninstallError::new(
                UninstallErrorCode::ForceTargetInvalid,
                format!("无法读取快捷方式目标: {error}"),
            )
        })?;
    if !output.status.success() {
        return Err(UninstallError::new(
            UninstallErrorCode::ForceTargetInvalid,
            "无法读取快捷方式目标，请改用程序目录或 EXE 路径",
        ));
    }
    let target = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if target.is_empty() {
        return Err(UninstallError::new(
            UninstallErrorCode::ForceTargetInvalid,
            "快捷方式没有可解析的程序目标",
        ));
    }
    canonicalize_path(Path::new(&target)).map_err(|error| {
        UninstallError::new(
            UninstallErrorCode::ForceTargetNotFound,
            format!("找不到快捷方式指向的程序: {error}"),
        )
    })
}

fn canonicalize_path(path: &Path) -> std::io::Result<PathBuf> {
    Ok(strip_extended_prefix(fs::canonicalize(path)?))
}

fn strip_extended_prefix(path: PathBuf) -> PathBuf {
    let value = path.to_string_lossy();
    if value.len() >= 8 && value[..8].eq_ignore_ascii_case("\\\\?\\UNC\\") {
        return PathBuf::from(format!("\\\\{}", &value[8..]));
    }
    if value.len() >= 4 && value[..4].eq_ignore_ascii_case("\\\\?\\") {
        return PathBuf::from(&value[4..]);
    }
    path
}

fn validate_target_root(root: &Path) -> Result<(), UninstallError> {
    if !root.is_dir() {
        return Err(UninstallError::new(
            UninstallErrorCode::ForceTargetNotFound,
            "程序所在目录不存在",
        ));
    }
    if root.parent().is_none() || utils::is_system_critical_path(&root.to_string_lossy()) {
        return Err(UninstallError::new(
            UninstallErrorCode::UnsafeInstallLocation,
            "不能把磁盘根目录或 Windows 系统目录作为强制卸载目标",
        ));
    }

    let forbidden_names = [
        "windows",
        "system32",
        "syswow64",
        "winsxs",
        "program files",
        "program files (x86)",
        "programdata",
        "users",
        "public",
        "appdata",
        "windowsapps",
        "common files",
    ];
    let leaf = root
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .trim()
        .to_lowercase();
    if forbidden_names.iter().any(|name| *name == leaf) {
        return Err(UninstallError::new(
            UninstallErrorCode::UnsafeInstallLocation,
            "目标目录范围过大或属于系统共享目录，请选择具体的程序目录",
        ));
    }

    if dirs::home_dir().is_some_and(|home| same_path(&home, root)) {
        return Err(UninstallError::new(
            UninstallErrorCode::UnsafeInstallLocation,
            "不能把用户主目录作为强制卸载目标",
        ));
    }
    Ok(())
}

fn build_root_trace(name: &str, root: &Path) -> Trace {
    let path = root.to_string_lossy().to_string();
    let mut trace = Trace::new(name.to_string(), TraceType::File, path.clone())
        .with_description("用户明确提供的程序目录（强制模式）".to_string())
        .with_confidence(Confidence::High);
    trace.id = stable_trace_id(TraceType::File, &path);
    trace.size = utils::calculate_dir_size(root).ok();
    trace
}

fn is_relevant_registry_candidate(trace: &Trace, name: &str, root: &Path) -> bool {
    if !matches!(
        trace.trace_type,
        TraceType::RegistryKey | TraceType::RegistryValue
    ) {
        return false;
    }
    let path = trace.path.to_lowercase();
    let compact_path = compact_identifier(&path);
    let compact_name = compact_identifier(name);
    let compact_root = root
        .file_name()
        .and_then(|value| value.to_str())
        .map(compact_identifier)
        .unwrap_or_default();
    path.contains(&name.to_lowercase())
        || (!compact_name.is_empty() && compact_path.contains(&compact_name))
        || (!compact_root.is_empty() && compact_path.contains(&compact_root))
}

fn search_names(name: &str, root: &Path) -> Vec<String> {
    let mut names = vec![name.to_string()];
    if let Some(root_name) = root.file_name().and_then(|value| value.to_str()) {
        if !root_name.eq_ignore_ascii_case(name) {
            names.push(root_name.to_string());
        }
    }
    let compact = compact_identifier(name);
    if compact.len() >= 4 && !names.iter().any(|item| item.eq_ignore_ascii_case(&compact)) {
        names.push(compact);
    }
    names
}

fn compact_identifier(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn stable_trace_id(trace_type: TraceType, path: &str) -> String {
    let canonical = format!("{trace_type:?}:{}", path.to_lowercase());
    format!("force-{:016x}", fnv1a64(canonical.as_bytes()))
}

fn fingerprint_target(root: &Path) -> Result<String, UninstallError> {
    let mut entries = WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| fingerprint_error(root, error))?;
    entries.sort_by(|left, right| {
        left.path()
            .to_string_lossy()
            .to_lowercase()
            .cmp(&right.path().to_string_lossy().to_lowercase())
    });

    let mut hasher = Sha256::new();
    hasher.update(root.to_string_lossy().to_lowercase().as_bytes());
    for entry in entries {
        let relative = entry.path().strip_prefix(root).map_err(|error| {
            UninstallError::new(
                UninstallErrorCode::ResidueScanFailed,
                format!("计算强制卸载目标相对路径失败: {error}"),
            )
        })?;
        hasher.update(relative.to_string_lossy().to_lowercase().as_bytes());
        if entry.file_type().is_dir() {
            hasher.update(b"directory\0");
        } else if entry.file_type().is_file() {
            hasher.update(b"file\0");
            hash_target_file(entry.path(), &mut hasher)?;
        } else if entry.file_type().is_symlink() {
            hasher.update(b"link\0");
            let target = fs::read_link(entry.path())
                .map_err(|error| fingerprint_error(entry.path(), error))?;
            hasher.update(target.to_string_lossy().to_lowercase().as_bytes());
        } else {
            hasher.update(b"other\0");
        }
    }
    Ok(format!("force-sha256:{:x}", hasher.finalize()))
}

fn hash_target_file(path: &Path, hasher: &mut Sha256) -> Result<(), UninstallError> {
    let mut file = File::open(path).map_err(|error| fingerprint_error(path, error))?;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| fingerprint_error(path, error))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(())
}

fn fingerprint_error(path: &Path, error: impl std::fmt::Display) -> UninstallError {
    UninstallError::new(
        UninstallErrorCode::ResidueScanFailed,
        format!("读取强制卸载目标失败（{}）: {error}", path.display()),
    )
}

fn paths_overlap(left: &Path, right: &Path) -> bool {
    same_path(left, right) || left.starts_with(right) || right.starts_with(left)
}

fn same_path(left: &Path, right: &Path) -> bool {
    left.to_string_lossy()
        .eq_ignore_ascii_case(&right.to_string_lossy())
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    })
}

#[cfg(test)]
mod tests {
    use super::{
        build_root_trace, fingerprint_target, plan_force_uninstall, select_revalidated_traces,
        ForceUninstallPlan,
    };
    use crate::application::uninstall::error::UninstallErrorCode;
    use crate::modules::scanner::models::{Confidence, Trace, TraceType};
    use std::fs;
    use std::path::PathBuf;

    fn temp_target(name: &str) -> PathBuf {
        let path =
            std::env::temp_dir().join(format!("rust-yu-force-{name}-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&path).expect("测试目标目录应可创建");
        path
    }

    #[test]
    fn force_plan_uses_explicit_directory_as_high_confidence_trace() {
        let root = temp_target("plan");
        let file = root.join("payload.exe");
        fs::write(&file, b"fixture").expect("测试文件应可写入");

        let plan = plan_force_uninstall(root.to_string_lossy().as_ref(), None)
            .expect("具体测试目录应生成强制卸载计划");

        assert_eq!(
            plan.target.name,
            root.file_name()
                .and_then(|v| v.to_str())
                .unwrap_or_default()
        );
        assert_eq!(plan.traces[0].confidence, Confidence::High);
        assert!(plan
            .traces
            .iter()
            .any(|trace| trace.path.eq_ignore_ascii_case(&root.to_string_lossy())));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn force_plan_accepts_an_executable_and_uses_its_parent_directory() {
        let root = temp_target("exe");
        let executable = root.join("Demo.exe");
        fs::write(&executable, b"fixture").expect("测试 EXE 应可写入");

        let plan = plan_force_uninstall(executable.to_string_lossy().as_ref(), Some("Demo App"))
            .expect("具体测试 EXE 应生成强制卸载计划");

        assert_eq!(plan.target.name, "Demo App");
        assert_eq!(
            plan.target.resolved_path.to_lowercase(),
            root.to_string_lossy().to_lowercase()
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn force_plan_rejects_a_shared_directory() {
        let root = temp_target("safe");
        let shared = root.join("Program Files");
        fs::create_dir_all(&shared).expect("共享目录测试夹具应可创建");

        let error = plan_force_uninstall(shared.to_string_lossy().as_ref(), None)
            .expect_err("共享目录不能作为强制卸载目标");

        assert_eq!(error.code, UninstallErrorCode::UnsafeInstallLocation);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn force_trace_ids_are_stable_for_the_same_target() {
        let root = temp_target("fingerprint");
        let left = build_root_trace("Demo", &root);
        let right = build_root_trace("Demo", &root);

        assert_eq!(left.id, right.id);
        assert_eq!(
            fingerprint_target(&root).expect("首次指纹应成功"),
            fingerprint_target(&root).expect("重复指纹应成功")
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn fingerprint_changes_when_nested_file_content_changes() {
        let root = temp_target("fingerprint-content");
        let nested = root.join("data");
        fs::create_dir_all(&nested).expect("测试子目录应可创建");
        let file = nested.join("payload.bin");
        fs::write(&file, b"before").expect("初始内容应可写入");
        let before = fingerprint_target(&root).expect("初始指纹应成功");

        fs::write(&file, b"after!").expect("替换内容应可写入");
        let after = fingerprint_target(&root).expect("变更后指纹应成功");

        assert_ne!(before, after);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn stale_or_unknown_selection_is_rejected_before_deletion() {
        let root = temp_target("selection");
        let trace = build_root_trace("Demo", &root);
        let plan = ForceUninstallPlan {
            plan_id: "plan".to_string(),
            target: super::ForceUninstallTarget {
                input_path: root.to_string_lossy().to_string(),
                resolved_path: root.to_string_lossy().to_string(),
                name: "Demo".to_string(),
                kind: super::ForceTargetKind::Directory,
            },
            fingerprint: fingerprint_target(&root).expect("测试指纹应成功"),
            traces: vec![trace.clone()],
            default_selected_ids: Vec::new(),
            warnings: Vec::new(),
        };
        let current = plan.clone();

        let error = select_revalidated_traces(&plan, &current, &["missing".to_string()])
            .expect_err("未知 trace id 不能执行清理");

        assert_eq!(error.code, UninstallErrorCode::TraceNotInPlan);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn force_trace_type_is_file_for_the_explicit_directory() {
        let root = temp_target("type");
        let trace: Trace = build_root_trace("Demo", &root);
        assert_eq!(trace.trace_type, TraceType::File);
        let _ = fs::remove_dir_all(root);
    }

    #[tokio::test]
    async fn cleaning_selected_explicit_target_removes_only_that_fixture() {
        let root = temp_target("clean");
        let payload = root.join("payload.exe");
        fs::write(&payload, b"fixture").expect("测试文件应可写入");
        let plan = plan_force_uninstall(root.to_string_lossy().as_ref(), Some("Demo"))
            .expect("测试目标应生成计划");
        let root_trace_id = plan
            .traces
            .iter()
            .find(|trace| trace.path.eq_ignore_ascii_case(&root.to_string_lossy()))
            .map(|trace| trace.id.clone())
            .expect("计划应包含用户明确提供的目标目录");

        let result = super::clean_force_uninstall(
            &plan,
            super::ForceCleanupSelection {
                plan_id: plan.plan_id.clone(),
                trace_ids: vec![root_trace_id],
                confirm: true,
            },
        )
        .await
        .expect("测试目标应能被清理");

        assert!(result.success);
        assert_eq!(result.traces_cleaned, 1);
        assert!(!root.exists());
    }
}
