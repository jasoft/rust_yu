use std::path::{Path, PathBuf};

use super::models::InstalledProgram;
use crate::modules::common::utils::split_command_for_spawn;

/// 安装位置分析结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AppAnalysisResult {
    /// 推断出的安装目录。
    pub install_location: Option<PathBuf>,
    /// 卸载命令中能够确认存在的可执行文件，可作为图标候选。
    pub executable_path: Option<PathBuf>,
    /// 命中的分析器名称，便于日志和诊断。
    pub analyzer: &'static str,
}

/// 按 Delphi 版 `TInstallLocationAnalyser` 的顺序分析卸载命令。
///
/// 顺序必须保持为 Wise、InstallShield、Default：前两个安装器的日志/配置文件
/// 比命令中的卸载器路径更能代表真实安装目录，默认分析器最后才使用可执行文件父目录。
pub fn analyze_program(program: &InstalledProgram) -> Option<AppAnalysisResult> {
    if let Some(location) = valid_install_location(program.install_location.as_deref()) {
        return Some(AppAnalysisResult {
            install_location: Some(location),
            executable_path: None,
            analyzer: "registry",
        });
    }

    let command = program.preferred_uninstall_string()?;
    analyze_uninstall_command(command)
}

/// 分析传统卸载命令，复刻原项目的 Wise、InstallShield、Default 过滤器。
pub fn analyze_uninstall_command(command: &str) -> Option<AppAnalysisResult> {
    if let Some(location) = analyze_wise_command(command) {
        return Some(location);
    }

    if let Some(location) = analyze_installshield_command(command) {
        return Some(location);
    }

    analyze_default_command(command)
}

fn analyze_wise_command(command: &str) -> Option<AppAnalysisResult> {
    let lower = command.to_lowercase();
    let marker = if lower.contains("unvise32.exe") {
        "unvise32.exe"
    } else if lower.contains("unwise.exe") {
        "unwise.exe"
    } else {
        return None;
    };

    let log_path = extract_existing_path_after_marker(command, marker, ".log")?;
    Some(AppAnalysisResult {
        install_location: log_path.parent().map(Path::to_path_buf),
        executable_path: executable_path_from_command(command),
        analyzer: "wise",
    })
}

fn analyze_installshield_command(command: &str) -> Option<AppAnalysisResult> {
    if !command.to_lowercase().contains("-f") {
        return None;
    }

    let isu_path = extract_existing_path_after_marker(command, "-f", ".isu")?;
    Some(AppAnalysisResult {
        install_location: isu_path.parent().map(Path::to_path_buf),
        executable_path: executable_path_from_command(command),
        analyzer: "installshield",
    })
}

fn analyze_default_command(command: &str) -> Option<AppAnalysisResult> {
    let executable_path = executable_path_from_command(command)?;
    let location = executable_path.parent().map(Path::to_path_buf)?;
    let location = valid_install_location(Some(location.to_string_lossy().as_ref()))?;

    Some(AppAnalysisResult {
        install_location: Some(location),
        executable_path: Some(executable_path),
        analyzer: "default",
    })
}

fn executable_path_from_command(command: &str) -> Option<PathBuf> {
    let (executable, _) = split_command_for_spawn(command).ok()?;
    let executable = expand_windows_environment_variables(&executable);
    let path = PathBuf::from(executable);
    path.is_file().then_some(path)
}

fn extract_existing_path_after_marker(
    command: &str,
    marker: &str,
    extension: &str,
) -> Option<PathBuf> {
    let lower_command = command.to_lowercase();
    let marker_position = lower_command.find(&marker.to_lowercase())?;
    let remainder = &command[marker_position + marker.len()..];
    let lower_remainder = remainder.to_lowercase();
    let extension_end = lower_remainder.find(&extension.to_lowercase())? + extension.len();
    let prefix = &remainder[..extension_end];
    let start = prefix
        .rfind('"')
        .map(|position| position + 1)
        .or_else(|| {
            prefix
                .rfind(char::is_whitespace)
                .map(|position| position + 1)
        })
        .unwrap_or(0);
    let candidate = expand_windows_environment_variables(prefix[start..].trim_matches('"').trim());
    let path = PathBuf::from(candidate);

    path.is_file().then_some(path)
}

fn valid_install_location(raw: Option<&str>) -> Option<PathBuf> {
    let path = PathBuf::from(expand_windows_environment_variables(raw?.trim()));
    if !path.is_dir() || is_system_or_common_files_path(&path) {
        return None;
    }

    Some(path)
}

fn is_system_or_common_files_path(path: &Path) -> bool {
    let normalized = path.to_string_lossy().replace('/', "\\").to_lowercase();
    let normalized = normalized.trim_end_matches('\\');
    let system_root = std::env::var("SystemRoot")
        .ok()
        .map(|value| value.replace('/', "\\").to_lowercase());
    let common_files = [
        std::env::var("CommonProgramFiles").ok(),
        std::env::var("CommonProgramFiles(x86)").ok(),
    ];

    system_root
        .as_deref()
        .map(|root| normalized == root || normalized.starts_with(&format!("{root}\\")))
        .unwrap_or(false)
        || common_files.iter().flatten().any(|root| {
            let root = root.replace('/', "\\").to_lowercase();
            normalized == root || normalized.starts_with(&format!("{root}\\"))
        })
}

fn expand_windows_environment_variables(value: &str) -> String {
    let mut expanded = value.to_string();
    let mut search_start = 0;

    while let Some(relative_start) = expanded[search_start..].find('%') {
        let start = search_start + relative_start;
        let Some(relative_end) = expanded[start + 1..].find('%') else {
            break;
        };
        let end = start + 1 + relative_end;
        let name = &expanded[start + 1..end];
        let Ok(replacement) = std::env::var(name) else {
            search_start = end + 1;
            continue;
        };
        expanded.replace_range(start..=end, &replacement);
        search_start = start + replacement.len();
    }

    expanded
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{analyze_uninstall_command, expand_windows_environment_variables};

    #[test]
    fn default_analyzer_rejects_missing_executable() {
        assert!(analyze_uninstall_command(r#"C:\missing\uninstall.exe /S"#).is_none());
    }

    #[test]
    fn environment_expansion_keeps_unknown_variables() {
        assert_eq!(
            expand_windows_environment_variables(r#"%RUST_YU_UNKNOWN%\App"#),
            r#"%RUST_YU_UNKNOWN%\App"#
        );
    }

    #[test]
    fn default_analyzer_returns_existing_executable_parent() {
        let root = std::env::temp_dir().join(format!("rust-yu-analyzer-{}", uuid::Uuid::new_v4()));
        assert!(fs::create_dir_all(&root).is_ok());
        let executable = root.join("uninstall.exe");
        assert!(fs::write(&executable, b"fixture").is_ok());

        let command = format!(r#""{}" /S"#, executable.display());
        let result = analyze_uninstall_command(&command);

        assert_eq!(
            result
                .as_ref()
                .and_then(|value| value.install_location.clone()),
            Some(root.clone())
        );
        assert_eq!(result.as_ref().map(|value| value.analyzer), Some("default"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn installshield_analyzer_prefers_existing_isu_file() {
        let root =
            std::env::temp_dir().join(format!("rust-yu-installshield-{}", uuid::Uuid::new_v4()));
        assert!(fs::create_dir_all(&root).is_ok());
        let isu_file = root.join("setup.isu");
        assert!(fs::write(&isu_file, b"fixture").is_ok());

        let command = format!(r#"setup.exe -f "{}""#, isu_file.display());
        let result = analyze_uninstall_command(&command);

        assert_eq!(
            result.as_ref().map(|value| value.analyzer),
            Some("installshield")
        );
        assert_eq!(
            result
                .as_ref()
                .and_then(|value| value.install_location.clone()),
            Some(root.clone())
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn wise_analyzer_uses_uninstall_log_directory() {
        let root = std::env::temp_dir().join(format!("rust-yu-wise-{}", uuid::Uuid::new_v4()));
        assert!(fs::create_dir_all(&root).is_ok());
        let log_file = root.join("uninstall.log");
        assert!(fs::write(&log_file, b"fixture").is_ok());

        let command = format!(r#"unwise.exe "{}""#, log_file.display());
        let result = analyze_uninstall_command(&command);

        assert_eq!(result.as_ref().map(|value| value.analyzer), Some("wise"));
        assert_eq!(
            result
                .as_ref()
                .and_then(|value| value.install_location.clone()),
            Some(root.clone())
        );
        let _ = fs::remove_dir_all(root);
    }
}
