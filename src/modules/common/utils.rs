use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use winreg::enums::*;
use winreg::HKEY;

/// 规范化路径（处理大小写、斜杠等）
#[allow(dead_code)]
pub fn normalize_path(path: &str) -> String {
    let path = path.replace('/', "\\");

    // 处理连续的反斜杠
    while path.contains("\\\\") {
        let path = path.replace("\\\\", "\\");
        return normalize_path(&path);
    }

    path
}

/// 计算目录大小
pub fn calculate_dir_size(path: &std::path::Path) -> std::io::Result<u64> {
    let mut size = 0u64;

    if path.is_file() {
        return path.metadata().map(|m| m.len());
    }

    for entry in walkdir::WalkDir::new(path)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if entry.file_type().is_file() {
            if let Ok(metadata) = entry.metadata() {
                size += metadata.len();
            }
        }
    }

    Ok(size)
}

/// 模糊匹配字符串
pub fn fuzzy_match(text: &str, pattern: &str) -> bool {
    let matcher = SkimMatcherV2::default();
    matcher.fuzzy_match(text, pattern).is_some()
}

/// 获取模糊匹配分数
#[allow(dead_code)]
pub fn fuzzy_score(text: &str, pattern: &str) -> i64 {
    let matcher = SkimMatcherV2::default();
    matcher.fuzzy_match(text, pattern).unwrap_or(0)
}

/// 格式化文件大小
pub fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

/// 检查路径是否为系统关键路径
pub fn is_system_critical_path(path: &str) -> bool {
    let path_upper = path.to_uppercase();

    let critical_paths = [
        r"C:\WINDOWS",
        r"C:\WINDOWS\SYSTEM32",
        r"C:\WINDOWS\SYSWOW64",
        r"C:\WINDOWS\INF",
        r"C:\WINDOWS\WINSXS",
        r"C:\PROGRAM FILES\WINDOWS",
    ];

    critical_paths
        .iter()
        .any(|p| path_upper.starts_with(&p.to_uppercase()))
}

/// 检查注册表路径是否为关键路径
pub fn is_critical_registry_path(path: &str) -> bool {
    let path_upper = path.to_uppercase();

    let critical_paths = [
        r"HKLM\SYSTEM",
        r"HKLM\SOFTWARE\Microsoft\Windows NT",
        r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\RUN",
        r"HKCR\*",
        r"HKLM\BOOT",
    ];

    critical_paths.iter().any(|p| path_upper.starts_with(p))
}

/// 生成唯一 ID
#[allow(dead_code)]
pub fn generate_id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// 解析注册表路径
pub fn parse_registry_path(path: &str) -> Option<(HKEY, &str)> {
    let path = path.trim();

    if path.starts_with("HKLM\\") || path.starts_with("HKEY_LOCAL_MACHINE\\") {
        Some((HKEY_LOCAL_MACHINE, &path[5..]))
    } else if path.starts_with("HKCU\\") || path.starts_with("HKEY_CURRENT_USER\\") {
        Some((HKEY_CURRENT_USER, &path[5..]))
    } else if path.starts_with("HKCR\\") || path.starts_with("HKEY_CLASSES_ROOT\\") {
        Some((HKEY_CLASSES_ROOT, &path[5..]))
    } else if path.starts_with("HKU\\") || path.starts_with("HKEY_USERS\\") {
        Some((HKEY_USERS, &path[4..]))
    } else {
        None
    }
}

/// 获取 Windows 系统目录
#[allow(dead_code)]
pub fn get_system_dirs() -> Vec<std::path::PathBuf> {
    let mut dirs = Vec::new();

    if let Ok(program_files) = std::env::var("ProgramFiles") {
        dirs.push(std::path::PathBuf::from(program_files));
    }

    if let Ok(program_files_x86) = std::env::var("ProgramFiles(x86)") {
        dirs.push(std::path::PathBuf::from(program_files_x86));
    }

    if let Ok(program_w6432) = std::env::var("ProgramW6432") {
        dirs.push(std::path::PathBuf::from(program_w6432));
    }

    if let Ok(system_root) = std::env::var("SystemRoot") {
        let system_root = std::path::PathBuf::from(system_root);
        dirs.push(system_root.join("System32"));
        dirs.push(system_root.join("SysWOW64"));
    }

    // 公共目录
    if let Ok(public) = std::env::var("Public") {
        let public = std::path::PathBuf::from(public);
        dirs.push(public.join("Documents"));
        dirs.push(public.join("Desktop"));
    }

    // 用户目录
    if let Some(home) = dirs::home_dir() {
        dirs.push(home.join("AppData").join("Roaming"));
        dirs.push(home.join("AppData").join("Local"));
        dirs.push(home.join("Desktop"));
    }

    // 开始菜单
    if let Ok(program_data) = std::env::var("ProgramData") {
        dirs.push(
            std::path::PathBuf::from(program_data).join("Microsoft\\Windows\\Start Menu\\Programs"),
        );
    }

    dirs
}

/// 等待进程及其所有子进程结束
///
/// 在 Windows 上，uninstallString 可能启动 msiexec 或其他安装程序
/// 这些程序可能再 spawn 子进程，需要等待整个进程组结束
#[cfg(windows)]
pub async fn wait_for_process_group(
    pid: u32,
    timeout_secs: u64,
) -> Result<(), crate::modules::common::error::UninstallerError> {
    use std::time::{Duration, Instant};

    let start = Instant::now();

    // 首先等待主进程
    // 使用 Windows API 来等待进程结束
    loop {
        if start.elapsed() > Duration::from_secs(timeout_secs) {
            return Err(crate::modules::common::error::UninstallerError::Timeout(
                format!("进程 {} 在 {} 秒内未结束", pid, timeout_secs),
            ));
        }

        // 检查主进程是否还在运行
        if !is_process_running(pid) {
            // 主进程已结束，额外等待一下确保子进程也结束
            tokio::time::sleep(Duration::from_millis(500)).await;

            // 检查是否还有相关子进程
            if !has_child_processes(pid).await {
                return Ok(());
            }
        }

        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

/// 检查进程是否在运行
#[cfg(windows)]
fn is_process_running(pid: u32) -> bool {
    use std::process::Command;

    let output = Command::new("tasklist")
        .args(["/FI", &format!("PID eq {}", pid), "/NH"])
        .output();

    match output {
        Ok(output) => {
            let output_str = String::from_utf8_lossy(&output.stdout);
            // 如果找到进程，tasklist 会返回包含 PID 的行
            output_str.contains(&pid.to_string())
        }
        Err(_) => false,
    }
}

/// 检查是否有子进程在运行
#[cfg(windows)]
async fn has_child_processes(parent_pid: u32) -> bool {
    use std::process::Command;

    // 使用 wmic 获取子进程
    let output = Command::new("wmic")
        .args([
            "process",
            "where",
            &format!("ParentProcessId={}", parent_pid),
            "get",
            "ProcessId",
        ])
        .output();

    match output {
        Ok(output) => {
            let output_str = String::from_utf8_lossy(&output.stdout);
            // 如果有子进程，输出会包含多个 ProcessId
            let count = output_str
                .lines()
                .filter(|l| !l.trim().is_empty() && l.trim() != "ProcessId")
                .count();
            count > 0
        }
        Err(_) => false,
    }
}
