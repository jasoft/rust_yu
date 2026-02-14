use fuzzy_matcher::FuzzyMatcher;
use fuzzy_matcher::skim::SkimMatcherV2;
use std::path::Path;
use winreg::enums::*;
use winreg::HKEY;

/// 规范化路径（处理大小写、斜杠等）
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

    for entry in walkdir::WalkDir::new(path).into_iter().filter_map(|e| e.ok()) {
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

    critical_paths.iter().any(|p| path_upper.starts_with(&p.to_uppercase()))
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
        dirs.push(std::path::PathBuf::from(program_data)
            .join("Microsoft\\Windows\\Start Menu\\Programs"));
    }

    dirs
}
