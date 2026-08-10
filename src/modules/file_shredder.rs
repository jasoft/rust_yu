//! Windows 文件粉碎器。
//!
//! 这里的“粉碎”是尽力而为的文件级覆盖：机械硬盘通常能从中受益；SSD、快照、
//! 文件系统日志、云同步副本和存储控制器缓存仍可能保留数据。调用方必须把这个限制
//! 明确展示给用户，不能把文件级覆盖描述成对所有介质的绝对安全擦除。

use std::collections::HashSet;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::windows::fs::MetadataExt;
use std::os::windows::io::AsRawHandle;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use walkdir::WalkDir;
use windows::Win32::Foundation::HANDLE;
use windows::Win32::Storage::FileSystem::{
    GetFileInformationByHandle, GetFinalPathNameByHandleW, BY_HANDLE_FILE_INFORMATION,
    FILE_NAME_NORMALIZED,
};

const BUFFER_SIZE: usize = 1024 * 1024;
const MAX_FILES: usize = 20_000;
const CONFIRMATION_TEXT: &str = "永久粉碎";
const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShredMethod {
    Quick,
    Standard,
    Thorough,
}

impl ShredMethod {
    fn patterns(self) -> Vec<PassPattern> {
        match self {
            Self::Quick => vec![PassPattern::Zero],
            Self::Standard => vec![PassPattern::Zero, PassPattern::Ones, PassPattern::Random],
            Self::Thorough => vec![
                PassPattern::Zero,
                PassPattern::Ones,
                PassPattern::Random,
                PassPattern::Byte(0xaa),
                PassPattern::Byte(0x55),
                PassPattern::Random,
                PassPattern::Zero,
            ],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShredTargetPlan {
    pub path: String,
    pub kind: String,
    pub file_count: usize,
    pub size: u64,
    pub blocked_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShredPlan {
    pub method: ShredMethod,
    pub targets: Vec<ShredTargetPlan>,
    pub total_files: usize,
    pub total_bytes: u64,
    pub overwrite_bytes: u64,
    pub confirmation_token: String,
    pub confirmation_text: String,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShredRequest {
    pub paths: Vec<String>,
    pub method: ShredMethod,
    pub confirmation_token: String,
    pub confirmation_text: String,
    pub confirm: bool,
    pub dry_run: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShredFailure {
    pub path: String,
    pub error: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShredResult {
    pub dry_run: bool,
    pub shredded_files: usize,
    pub deleted_directories: usize,
    pub bytes_overwritten: u64,
    pub failures: Vec<ShredFailure>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ShredProgress {
    pub stage: String,
    pub current_path: String,
    pub processed_bytes: u64,
    pub total_bytes: u64,
    pub pass: usize,
    pub total_passes: usize,
    pub message: String,
}

#[derive(Debug, thiserror::Error)]
pub enum ShredError {
    #[error("未选择任何文件或文件夹")]
    EmptySelection,
    #[error("粉碎计划包含受保护或无效的目标，请先移除这些目标")]
    BlockedTargets,
    #[error("文件在确认后发生了变化，请重新分析并确认")]
    PlanChanged,
    #[error("必须输入“永久粉碎”并明确确认")]
    ConfirmationRequired,
    #[error("目标数量超过安全上限 {MAX_FILES}，请缩小选择范围")]
    TooManyFiles,
    #[error("文件系统错误: {0}")]
    Io(#[from] std::io::Error),
    #[error("无法获取安全随机数: {0}")]
    Random(String),
}

#[derive(Debug, Clone)]
struct PlannedFile {
    path: PathBuf,
    size: u64,
    fingerprint: String,
}

#[derive(Debug, Clone)]
struct InternalPlan {
    public: ShredPlan,
    files: Vec<PlannedFile>,
    directories: Vec<PathBuf>,
}

#[derive(Debug, Clone, Copy)]
enum PassPattern {
    Zero,
    Ones,
    Random,
    Byte(u8),
}

pub fn plan(paths: &[String], method: ShredMethod) -> Result<ShredPlan, ShredError> {
    Ok(build_plan(paths, method)?.public)
}

pub fn execute_with_progress(
    request: ShredRequest,
    mut progress: impl FnMut(ShredProgress),
) -> Result<ShredResult, ShredError> {
    if !request.confirm || request.confirmation_text.trim() != CONFIRMATION_TEXT {
        return Err(ShredError::ConfirmationRequired);
    }

    let plan = build_plan(&request.paths, request.method)?;
    if plan
        .public
        .targets
        .iter()
        .any(|target| target.blocked_reason.is_some())
    {
        return Err(ShredError::BlockedTargets);
    }
    if plan.public.confirmation_token != request.confirmation_token {
        return Err(ShredError::PlanChanged);
    }

    if request.dry_run {
        return Ok(ShredResult {
            dry_run: true,
            shredded_files: plan.files.len(),
            deleted_directories: plan.directories.len(),
            bytes_overwritten: plan.public.overwrite_bytes,
            failures: Vec::new(),
        });
    }

    let patterns = request.method.patterns();
    let mut result = ShredResult {
        dry_run: false,
        shredded_files: 0,
        deleted_directories: 0,
        bytes_overwritten: 0,
        failures: Vec::new(),
    };
    let mut processed_bytes = 0_u64;

    for planned in &plan.files {
        let path_text = planned.path.to_string_lossy().into_owned();
        match shred_file(
            &planned.path,
            &planned.fingerprint,
            &patterns,
            |pass, bytes, message| {
                progress(ShredProgress {
                    stage: "overwriting".to_string(),
                    current_path: path_text.clone(),
                    processed_bytes: processed_bytes.saturating_add(bytes),
                    total_bytes: plan.public.overwrite_bytes,
                    pass,
                    total_passes: patterns.len(),
                    message,
                });
            },
        ) {
            Ok(overwritten) => {
                result.shredded_files += 1;
                result.bytes_overwritten = result.bytes_overwritten.saturating_add(overwritten);
                processed_bytes = processed_bytes.saturating_add(overwritten);
            }
            Err(error) => result.failures.push(ShredFailure {
                path: path_text,
                error: error.to_string(),
            }),
        }
    }

    // 只从最深层向上移除空目录；任何失败文件都会让其父目录自然保留。
    for directory in &plan.directories {
        match fs::remove_dir(directory) {
            Ok(()) => result.deleted_directories += 1,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) if error.kind() == std::io::ErrorKind::DirectoryNotEmpty => {}
            Err(error) => result.failures.push(ShredFailure {
                path: directory.to_string_lossy().into_owned(),
                error: format!("无法移除空目录: {error}"),
            }),
        }
    }

    progress(ShredProgress {
        stage: "completed".to_string(),
        current_path: String::new(),
        processed_bytes,
        total_bytes: plan.public.overwrite_bytes,
        pass: patterns.len(),
        total_passes: patterns.len(),
        message: format!("粉碎完成：{} 个文件", result.shredded_files),
    });
    Ok(result)
}

fn build_plan(paths: &[String], method: ShredMethod) -> Result<InternalPlan, ShredError> {
    if paths.is_empty() {
        return Err(ShredError::EmptySelection);
    }

    let mut roots = normalize_roots(paths);
    let mut files = Vec::new();
    let mut directories = Vec::new();
    let mut targets = Vec::new();

    for requested in roots.drain(..) {
        let display = requested.to_string_lossy().into_owned();
        let mut target = ShredTargetPlan {
            path: display,
            kind: "missing".to_string(),
            file_count: 0,
            size: 0,
            blocked_reason: None,
        };

        let canonical = match fs::canonicalize(&requested) {
            Ok(value) => value,
            Err(error) => {
                target.blocked_reason = Some(format!("目标不存在或无法访问: {error}"));
                targets.push(target);
                continue;
            }
        };
        target.path = display_path(&canonical);

        if let Some(reason) = protected_reason(&canonical) {
            target.blocked_reason = Some(reason);
            targets.push(target);
            continue;
        }

        let metadata = fs::symlink_metadata(&canonical)?;
        if is_reparse_point(&metadata) {
            target.blocked_reason = Some("不允许粉碎符号链接、联接点或其他重解析点".to_string());
            targets.push(target);
            continue;
        }

        if metadata.is_file() {
            target.kind = "file".to_string();
            match plan_file(&canonical) {
                Ok(file) => {
                    target.file_count = 1;
                    target.size = file.size;
                    files.push(file);
                }
                Err(reason) => target.blocked_reason = Some(reason),
            }
        } else if metadata.is_dir() {
            target.kind = "directory".to_string();
            let before = files.len();
            let mut nested_dirs = Vec::new();
            for entry in WalkDir::new(&canonical)
                .follow_links(false)
                .contents_first(true)
            {
                let entry = match entry {
                    Ok(value) => value,
                    Err(error) => {
                        target.blocked_reason = Some(format!("无法完整遍历目录: {error}"));
                        break;
                    }
                };
                let entry_metadata = match fs::symlink_metadata(entry.path()) {
                    Ok(value) => value,
                    Err(error) => {
                        target.blocked_reason = Some(format!("无法读取目录项: {error}"));
                        break;
                    }
                };
                if is_reparse_point(&entry_metadata) {
                    target.blocked_reason = Some(format!(
                        "目录包含重解析点，已阻止以避免越界粉碎: {}",
                        entry.path().display()
                    ));
                    break;
                }
                if entry_metadata.is_file() {
                    match plan_file(entry.path()) {
                        Ok(file) => files.push(file),
                        Err(reason) => {
                            target.blocked_reason = Some(reason);
                            break;
                        }
                    }
                } else if entry_metadata.is_dir() {
                    nested_dirs.push(entry.path().to_path_buf());
                }
                if files.len() > MAX_FILES {
                    return Err(ShredError::TooManyFiles);
                }
            }
            if target.blocked_reason.is_some() {
                files.truncate(before);
            } else {
                target.file_count = files.len() - before;
                target.size = files[before..].iter().map(|file| file.size).sum();
                directories.extend(nested_dirs);
            }
        } else {
            target.blocked_reason = Some("只支持普通文件和文件夹".to_string());
        }
        targets.push(target);
    }

    if files.len() > MAX_FILES {
        return Err(ShredError::TooManyFiles);
    }
    directories.sort_by_key(|path| std::cmp::Reverse(path.components().count()));
    directories.dedup();

    let total_bytes = files.iter().map(|file| file.size).sum::<u64>();
    let overwrite_bytes = total_bytes.saturating_mul(method.patterns().len() as u64);
    let confirmation_token = plan_token(method, &targets, &files);
    Ok(InternalPlan {
        public: ShredPlan {
            method,
            targets,
            total_files: files.len(),
            total_bytes,
            overwrite_bytes,
            confirmation_token,
            confirmation_text: CONFIRMATION_TEXT.to_string(),
            warnings: vec![
                "SSD 的磨损均衡、TRIM、快照、日志和云端副本可能保留数据；本功能不能替代整盘安全擦除。".to_string(),
                "覆盖开始后不可撤销；锁定文件、多硬链接文件和重解析点会被拒绝。".to_string(),
            ],
        },
        files,
        directories,
    })
}

fn normalize_roots(paths: &[String]) -> Vec<PathBuf> {
    let mut seen = HashSet::new();
    let mut roots = paths
        .iter()
        .map(|path| PathBuf::from(path.trim()))
        .filter(|path| !path.as_os_str().is_empty())
        .filter(|path| seen.insert(path.to_string_lossy().to_lowercase()))
        .collect::<Vec<_>>();
    roots.sort_by_key(|path| path.components().count());

    let mut compact: Vec<PathBuf> = Vec::new();
    for candidate in roots {
        if !compact.iter().any(|root| candidate.starts_with(root)) {
            compact.push(candidate);
        }
    }
    compact
}

fn plan_file(path: &Path) -> Result<PlannedFile, String> {
    let metadata = fs::metadata(path).map_err(|error| format!("无法读取文件: {error}"))?;
    let file = File::open(path).map_err(|error| format!("无法独占检查文件: {error}"))?;
    let identity = file_identity(&file).map_err(|error| format!("无法读取文件身份: {error}"))?;
    if identity.links > 1 {
        return Err(format!(
            "文件存在多个硬链接，覆盖会同时破坏其他名称指向的数据: {}",
            path.display()
        ));
    }
    Ok(PlannedFile {
        path: path.to_path_buf(),
        size: metadata.len(),
        fingerprint: fingerprint_from_metadata(path, &metadata, identity),
    })
}

#[derive(Debug, Clone, Copy)]
struct FileIdentity {
    volume: u32,
    index: u64,
    links: u32,
}

fn file_identity(file: &File) -> Result<FileIdentity, std::io::Error> {
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    let handle = HANDLE(file.as_raw_handle());
    // SAFETY: handle 来自仍处于打开状态的 File；info 是有效且可写的结构体。
    unsafe { GetFileInformationByHandle(handle, &mut info) }
        .map_err(|error| std::io::Error::other(error.to_string()))?;
    Ok(FileIdentity {
        volume: info.dwVolumeSerialNumber,
        index: (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow),
        links: info.nNumberOfLinks,
    })
}

fn final_path(file: &File) -> Result<PathBuf, std::io::Error> {
    let handle = HANDLE(file.as_raw_handle());
    let mut buffer = vec![0_u16; 32_768];
    // SAFETY: handle 在调用期间有效，buffer 是可写 UTF-16 缓冲区。
    let length = unsafe { GetFinalPathNameByHandleW(handle, &mut buffer, FILE_NAME_NORMALIZED) };
    if length == 0 || length as usize >= buffer.len() {
        return Err(std::io::Error::last_os_error());
    }
    let value = String::from_utf16_lossy(&buffer[..length as usize]);
    Ok(PathBuf::from(value.strip_prefix(r"\\?\").unwrap_or(&value)))
}

fn fingerprint_from_metadata(
    path: &Path,
    metadata: &fs::Metadata,
    identity: FileIdentity,
) -> String {
    format!(
        "{}|{}|{}|{}|{}",
        display_path(path).to_lowercase(),
        metadata.len(),
        metadata.last_write_time(),
        identity.volume,
        identity.index
    )
}

fn plan_token(method: ShredMethod, targets: &[ShredTargetPlan], files: &[PlannedFile]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(format!("{method:?}"));
    for target in targets {
        hasher.update(target.path.as_bytes());
        hasher.update(
            target
                .blocked_reason
                .as_deref()
                .unwrap_or_default()
                .as_bytes(),
        );
    }
    for file in files {
        hasher.update(file.fingerprint.as_bytes());
    }
    format!("{:x}", hasher.finalize())
}

fn protected_reason(path: &Path) -> Option<String> {
    let path = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    if path.parent().is_none() {
        return Some("不允许粉碎磁盘根目录".to_string());
    }

    let mut protected = ["WINDIR", "ProgramFiles", "ProgramFiles(x86)", "ProgramData"]
        .iter()
        .filter_map(|name| std::env::var_os(name).map(PathBuf::from))
        .collect::<Vec<_>>();
    if let Ok(executable) = std::env::current_exe() {
        protected.push(executable);
    }

    for root in protected {
        if let Ok(root) = fs::canonicalize(root) {
            if path == root || path.starts_with(&root) || root.starts_with(&path) {
                return Some(format!("关键系统或应用路径受保护: {}", root.display()));
            }
        }
    }
    None
}

fn display_path(path: &Path) -> String {
    let value = path.to_string_lossy();
    value.strip_prefix(r"\\?\").unwrap_or(&value).to_string()
}

fn is_reparse_point(metadata: &fs::Metadata) -> bool {
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

fn shred_file(
    path: &Path,
    expected_fingerprint: &str,
    patterns: &[PassPattern],
    mut progress: impl FnMut(usize, u64, String),
) -> Result<u64, ShredError> {
    let metadata = fs::metadata(path)?;
    if is_reparse_point(&fs::symlink_metadata(path)?) {
        return Err(ShredError::Io(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "文件变成了重解析点，已拒绝覆盖",
        )));
    }

    if metadata.permissions().readonly() {
        let mut permissions = metadata.permissions();
        // 本项目仅支持 Windows；清除只读属性不会改变 Unix 权限位。
        #[allow(clippy::permissions_set_readonly_false)]
        permissions.set_readonly(false);
        fs::set_permissions(path, permissions)?;
    }

    let mut file = OpenOptions::new().read(true).write(true).open(path)?;
    let held_path = final_path(&file)?;
    if let Some(reason) = protected_reason(&held_path) {
        return Err(ShredError::Io(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            reason,
        )));
    }
    let held_metadata = file.metadata()?;
    let identity = file_identity(&file)?;
    if identity.links > 1 {
        return Err(ShredError::Io(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "文件出现多个硬链接，已拒绝覆盖",
        )));
    }
    let held_fingerprint = fingerprint_from_metadata(&held_path, &held_metadata, identity);
    if held_fingerprint != expected_fingerprint {
        return Err(ShredError::PlanChanged);
    }
    let size = file.metadata()?.len();
    let mut overwritten = 0_u64;
    for (index, pattern) in patterns.iter().enumerate() {
        let mut seed = [0_u8; 32];
        if matches!(pattern, PassPattern::Random) {
            getrandom::fill(&mut seed).map_err(|error| ShredError::Random(error.to_string()))?;
        }
        write_pass(&mut file, size, *pattern, &seed, |written| {
            progress(
                index + 1,
                overwritten.saturating_add(written),
                format!("第 {} 遍覆盖", index + 1),
            );
        })?;
        file.sync_data()?;
        verify_pass(&mut file, size, *pattern, &seed)?;
        overwritten = overwritten.saturating_add(size);
        progress(
            index + 1,
            overwritten,
            format!("第 {} 遍校验通过", index + 1),
        );
    }

    file.set_len(0)?;
    file.sync_all()?;
    drop(file);
    obscure_and_remove(path).map_err(|error| {
        ShredError::Io(std::io::Error::other(format!(
            "文件内容已覆盖、校验并截断，但目录项移除失败: {error}"
        )))
    })?;
    Ok(overwritten)
}

fn write_pass(
    file: &mut File,
    size: u64,
    pattern: PassPattern,
    seed: &[u8; 32],
    mut progress: impl FnMut(u64),
) -> Result<(), ShredError> {
    file.seek(SeekFrom::Start(0))?;
    let mut offset = 0_u64;
    let mut buffer = vec![0_u8; BUFFER_SIZE];
    while offset < size {
        let length =
            usize::try_from((size - offset).min(BUFFER_SIZE as u64)).unwrap_or(BUFFER_SIZE);
        fill_pattern(&mut buffer[..length], pattern, seed, offset);
        file.write_all(&buffer[..length])?;
        offset += length as u64;
        progress(offset);
    }
    Ok(())
}

fn verify_pass(
    file: &mut File,
    size: u64,
    pattern: PassPattern,
    seed: &[u8; 32],
) -> Result<(), ShredError> {
    file.seek(SeekFrom::Start(0))?;
    let mut offset = 0_u64;
    let mut actual = vec![0_u8; BUFFER_SIZE];
    let mut expected = vec![0_u8; BUFFER_SIZE];
    while offset < size {
        let length =
            usize::try_from((size - offset).min(BUFFER_SIZE as u64)).unwrap_or(BUFFER_SIZE);
        file.read_exact(&mut actual[..length])?;
        fill_pattern(&mut expected[..length], pattern, seed, offset);
        if actual[..length] != expected[..length] {
            return Err(ShredError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("覆盖后回读校验失败，偏移 {offset}"),
            )));
        }
        offset += length as u64;
    }
    Ok(())
}

fn fill_pattern(buffer: &mut [u8], pattern: PassPattern, seed: &[u8; 32], offset: u64) {
    match pattern {
        PassPattern::Zero => buffer.fill(0),
        PassPattern::Ones => buffer.fill(0xff),
        PassPattern::Byte(value) => buffer.fill(value),
        PassPattern::Random => {
            let mut position = 0;
            let mut counter = offset / 32;
            let prefix = (offset % 32) as usize;
            while position < buffer.len() {
                let mut hasher = Sha256::new();
                hasher.update(seed);
                hasher.update(counter.to_le_bytes());
                let block = hasher.finalize();
                let start = if position == 0 { prefix } else { 0 };
                let available = 32 - start;
                let copy = available.min(buffer.len() - position);
                buffer[position..position + copy].copy_from_slice(&block[start..start + copy]);
                position += copy;
                counter += 1;
            }
        }
    }
}

fn obscure_and_remove(original: &Path) -> Result<(), ShredError> {
    let parent = original.parent().ok_or_else(|| {
        ShredError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "目标没有父目录",
        ))
    })?;
    let mut current = original.to_path_buf();
    for _ in 0..3 {
        let mut random = [0_u8; 12];
        getrandom::fill(&mut random).map_err(|error| ShredError::Random(error.to_string()))?;
        let name = random
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let next = parent.join(name);
        fs::rename(&current, &next)?;
        current = next;
    }
    fs::remove_file(current)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("rust-yu-shredder-{name}-{}", uuid::Uuid::new_v4()))
    }

    #[test]
    fn plan_deduplicates_nested_selection_and_counts_files() {
        let root = temp_root("plan");
        let nested = root.join("nested");
        fs::create_dir_all(&nested).expect("create fixture");
        fs::write(root.join("a.txt"), b"abc").expect("write fixture");
        fs::write(nested.join("b.txt"), b"12345").expect("write fixture");

        let result = plan(
            &[
                root.to_string_lossy().into_owned(),
                nested.to_string_lossy().into_owned(),
            ],
            ShredMethod::Standard,
        )
        .expect("plan should succeed");

        assert_eq!(result.targets.len(), 1);
        assert_eq!(result.total_files, 2);
        assert_eq!(result.total_bytes, 8);
        assert_eq!(result.overwrite_bytes, 24);
        fs::remove_dir_all(root).expect("remove fixture");
    }

    #[test]
    fn execution_rejects_changed_plan() {
        let root = temp_root("changed");
        fs::create_dir_all(&root).expect("create fixture");
        let file = root.join("secret.txt");
        fs::write(&file, b"secret").expect("write fixture");
        let initial = plan(&[file.to_string_lossy().into_owned()], ShredMethod::Quick)
            .expect("plan should succeed");
        fs::write(&file, b"changed contents").expect("change fixture");

        let result = execute_with_progress(
            ShredRequest {
                paths: vec![file.to_string_lossy().into_owned()],
                method: ShredMethod::Quick,
                confirmation_token: initial.confirmation_token,
                confirmation_text: CONFIRMATION_TEXT.to_string(),
                confirm: true,
                dry_run: false,
            },
            |_| {},
        );

        assert!(matches!(result, Err(ShredError::PlanChanged)));
        assert!(file.exists());
        fs::remove_dir_all(root).expect("remove fixture");
    }

    #[test]
    fn quick_shred_overwrites_verifies_and_removes_file() {
        let root = temp_root("execute");
        fs::create_dir_all(&root).expect("create fixture");
        let file = root.join("secret.txt");
        fs::write(&file, vec![0x5a; BUFFER_SIZE + 17]).expect("write fixture");
        let planned = plan(&[file.to_string_lossy().into_owned()], ShredMethod::Quick)
            .expect("plan should succeed");

        let result = execute_with_progress(
            ShredRequest {
                paths: vec![file.to_string_lossy().into_owned()],
                method: ShredMethod::Quick,
                confirmation_token: planned.confirmation_token,
                confirmation_text: CONFIRMATION_TEXT.to_string(),
                confirm: true,
                dry_run: false,
            },
            |_| {},
        )
        .expect("shred should succeed");

        assert_eq!(result.shredded_files, 1, "failures: {:?}", result.failures);
        assert!(!file.exists());
        fs::remove_dir_all(root).expect("remove fixture");
    }

    #[test]
    fn dry_run_requires_confirmation_but_preserves_file() {
        let root = temp_root("dry-run");
        fs::create_dir_all(&root).expect("create fixture");
        let file = root.join("secret.txt");
        fs::write(&file, b"secret").expect("write fixture");
        let planned = plan(&[file.to_string_lossy().into_owned()], ShredMethod::Quick)
            .expect("plan should succeed");

        let result = execute_with_progress(
            ShredRequest {
                paths: vec![file.to_string_lossy().into_owned()],
                method: ShredMethod::Quick,
                confirmation_token: planned.confirmation_token,
                confirmation_text: CONFIRMATION_TEXT.to_string(),
                confirm: true,
                dry_run: true,
            },
            |_| {},
        )
        .expect("dry run should succeed");

        assert!(result.dry_run);
        assert!(file.exists());
        fs::remove_dir_all(root).expect("remove fixture");
    }

    #[test]
    fn plan_blocks_volume_root() {
        let result = plan(&[r"C:\".to_string()], ShredMethod::Quick).expect("plan root");
        assert_eq!(result.targets.len(), 1);
        assert!(result.targets[0]
            .blocked_reason
            .as_deref()
            .is_some_and(|reason| reason.contains("根目录")));
    }

    #[test]
    fn plan_blocks_files_with_multiple_hard_links() {
        let root = temp_root("hard-link");
        fs::create_dir_all(&root).expect("create fixture");
        let file = root.join("secret.txt");
        let link = root.join("second-name.txt");
        fs::write(&file, b"shared data").expect("write fixture");
        fs::hard_link(&file, &link).expect("create hard link");

        let result = plan(&[file.to_string_lossy().into_owned()], ShredMethod::Quick)
            .expect("plan should complete with blocked target");
        assert!(result.targets[0]
            .blocked_reason
            .as_deref()
            .is_some_and(|reason| reason.contains("硬链接")));
        fs::remove_dir_all(root).expect("remove fixture");
    }
}
