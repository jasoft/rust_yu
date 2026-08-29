//! Residue scanner identity and path-safety boundaries.
//!
//! A residue scanner must never treat an arbitrary substring in a Windows path
//! as ownership evidence.  This module keeps the matching and protected-area
//! rules shared by filesystem, AppData and shortcut scanners.

use crate::modules::lister::models::InstalledProgram;
use std::path::{Path, PathBuf};

const GENERIC_SUFFIXES: &[&str] = &["app", "application", "client", "software", "suite", "tool"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanIdentity {
    display_name: String,
    aliases: Vec<String>,
}

impl ScanIdentity {
    pub fn from_name(name: &str) -> Self {
        let display_name = name.trim().to_string();
        let compact = compact_identifier(&display_name);
        let mut aliases = vec![compact.clone()];

        // Legacy installers often omit a generic trailing "App"/"Software"
        // suffix.  Remove only one known generic suffix, never arbitrary
        // prefixes or substrings; this keeps RustYuLegacyTest valid without
        // matching unrelated names such as Internet Explorer.
        let significant = significant_tokens(&display_name);
        if significant.len() < token_count(&display_name) {
            let without_generic = compact_identifier(&significant.join(" "));
            if without_generic.len() >= 6 && !aliases.contains(&without_generic) {
                aliases.push(without_generic);
            }
        }

        Self {
            display_name,
            aliases,
        }
    }

    pub fn from_program(program: &InstalledProgram) -> Self {
        Self::from_name(&program.name)
    }

    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    pub fn aliases(&self) -> &[String] {
        &self.aliases
    }

    /// Match one path component or a filename stem.
    ///
    /// Matching is deliberately equality-based after Windows-safe case and
    /// punctuation normalization.  `xplorer` therefore does not match the
    /// `explorer` component in `Internet Explorer`.
    ///
    /// 先比较完整组件，再回退到去掉扩展名的文件主干。带版本点的 DisplayName
    /// （例如 `Choice 9.0.0.1`）会被 file_stem 截成 `Choice 9.0.0`，只比较
    /// 主干会让 DisplayName 与自身完全相等却匹配失败，导致 Uninstall 键
    /// 永远扫描不到。
    pub fn matches_component(&self, component: &str) -> bool {
        let normalized_full = compact_identifier(component);
        if !normalized_full.is_empty() && self.aliases.iter().any(|alias| alias == &normalized_full)
        {
            return true;
        }
        let without_extension = Path::new(component)
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or(component);
        let normalized_stem = compact_identifier(without_extension);
        normalized_stem != normalized_full
            && !normalized_stem.is_empty()
            && self.aliases.iter().any(|alias| alias == &normalized_stem)
    }

    pub fn matches_display_name(&self, value: &str) -> bool {
        if self.matches_component(value) {
            return true;
        }

        // Uninstall DisplayName 常带版本号，例如 `Xplorer 0.3.1`。
        // 只接受完整身份后紧跟纯数字版本后缀，不接受任意
        // `xplorer-helper`/`internet-explorer` 之类的名称相似项。
        let normalized = compact_identifier(value);
        self.aliases.iter().any(|alias| {
            normalized.strip_prefix(alias).is_some_and(|suffix| {
                !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_digit())
            })
        })
    }
}

fn token_count(value: &str) -> usize {
    value
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .count()
}

fn significant_tokens(value: &str) -> Vec<String> {
    let mut tokens = value
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(|token| token.to_ascii_lowercase())
        .collect::<Vec<_>>();
    if tokens
        .last()
        .is_some_and(|token| GENERIC_SUFFIXES.contains(&token.as_str()))
    {
        let _ = tokens.pop();
    }
    tokens
}

pub fn compact_identifier(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

pub fn normalized_path(path: &Path) -> String {
    strip_extended_prefix(path)
        .to_string_lossy()
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_ascii_lowercase()
}

pub fn strip_extended_prefix(path: &Path) -> PathBuf {
    let value = path.to_string_lossy();
    if value.len() >= 8 && value[..8].eq_ignore_ascii_case(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{}", &value[8..]));
    }
    if value.len() >= 4 && value[..4].eq_ignore_ascii_case(r"\\?\") {
        return PathBuf::from(&value[4..]);
    }
    path.to_path_buf()
}

fn path_components(path: &Path) -> Vec<String> {
    normalized_path(path)
        .split(['\\', '/'])
        .filter(|component| !component.is_empty() && !component.ends_with(':'))
        .map(ToOwned::to_owned)
        .collect()
}

fn contains_sequence(components: &[String], sequence: &[&str]) -> bool {
    components.windows(sequence.len()).any(|window| {
        window
            .iter()
            .zip(sequence.iter())
            .all(|(actual, expected)| actual == expected)
    })
}

/// Shared Windows and shell locations where name-only residue discovery is
/// forbidden.  Explicit install-location scans apply their own narrower
/// policy, but also call this guard for child paths before creating traces.
pub fn is_protected_shared_path(path: &Path) -> bool {
    let components = path_components(path);
    if components.is_empty() {
        return true;
    }

    const ALWAYS_PROTECTED: &[&str] = &[
        "windows",
        "system32",
        "syswow64",
        "winsxs",
        "servicing",
        "boot",
        "efi",
        "driverstore",
        "windowsapps",
        "common files",
        "public",
        "default user",
        "all users",
        "packages",
        "connecteddevicesplatform",
        "internet explorer",
        "quick launch",
        "user pinned",
        "taskbar",
    ];
    if components
        .iter()
        .any(|component| ALWAYS_PROTECTED.contains(&component.as_str()))
    {
        return true;
    }

    contains_sequence(&components, &["microsoft", "windows"])
        || contains_sequence(&components, &["microsoft", "internet explorer"])
        || contains_sequence(&components, &["microsoft", "start menu"])
        || contains_sequence(&components, &["appdata", "roaming", "microsoft"])
        || contains_sequence(&components, &["appdata", "local", "microsoft"])
        || contains_sequence(&components, &["appdata", "locallow", "microsoft"])
        || contains_sequence(&components, &["programdata", "microsoft"])
}

pub fn is_protected_appdata_path(path: &Path) -> bool {
    let components = path_components(path);
    if components
        .iter()
        .any(|component| matches!(component.as_str(), "microsoft" | "packages" | "windows"))
    {
        return true;
    }
    is_protected_shared_path(path)
}

/// `%LOCALAPPDATA%\Programs` 是当前用户安装目录，不是程序运行数据目录。
pub fn is_user_program_install_path(path: &Path) -> bool {
    contains_sequence(&path_components(path), &["appdata", "local", "programs"])
}

pub fn is_protected_install_location(path: &Path) -> bool {
    let components = path_components(path);
    if components.is_empty() {
        return true;
    }

    // Microsoft Store 的 InstallLocation 是 WindowsApps 下某一个明确包目录。
    // 允许扫描这个精确目录以便向用户展示内容，但 WindowsApps 根目录、其余共享
    // 区域和实际清理仍保持保护；文件痕迹会在 filesystem 扫描器中标为关键项。
    if is_specific_windows_apps_package(path) {
        return false;
    }

    is_protected_shared_path(path)
        || contains_sequence(&components, &["appdata", "roaming", "microsoft"])
        || contains_sequence(&components, &["appdata", "local", "microsoft"])
        || contains_sequence(&components, &["programdata", "microsoft"])
}

pub fn is_specific_windows_apps_package(path: &Path) -> bool {
    let roots = windows_apps_roots();
    windows_apps_package_root(path, &roots)
        .is_some_and(|root| normalized_path(&root) == normalized_path(path))
}

/// 判断路径是否位于某个精确 AppX/MSIX 包目录内部。
///
/// `WindowsApps` 本身仍是共享系统区域；但它的一个合法包目录是单个应用的
/// 私有安装内容。扫描展示和卸载后残留清理可以按用户确认处理这些文件，
/// 不应再因为父目录名包含 `WindowsApps` 就把它们泛化成受保护系统项。
pub fn is_within_specific_windows_apps_package(path: &Path) -> bool {
    let roots = windows_apps_roots();
    is_within_specific_windows_apps_package_with_roots(path, &roots)
}

pub(crate) fn is_within_specific_windows_apps_package_with_roots(
    path: &Path,
    windows_apps_roots: &[PathBuf],
) -> bool {
    windows_apps_package_root(path, windows_apps_roots)
        .is_some_and(|root| normalized_path(&root) != normalized_path(path))
}

pub(super) fn windows_apps_roots() -> Vec<PathBuf> {
    ["ProgramFiles", "ProgramW6432"]
        .into_iter()
        .filter_map(|name| std::env::var(name).ok())
        .map(|root| PathBuf::from(root).join("WindowsApps"))
        .collect()
}

fn windows_apps_package_root(path: &Path, windows_apps_roots: &[PathBuf]) -> Option<PathBuf> {
    let components = path_components(path);
    let index = components
        .iter()
        .position(|component| component == "windowsapps")?;

    // AppX 包目录固定为 WindowsApps 的直接子目录；要求包身份常见的多段下划线
    // 结构，避免把根目录或任意更深路径误当成可信安装位置。
    if index + 2 > components.len() {
        return None;
    }
    let excess_depth = components.len().saturating_sub(index + 2);
    let Some(package_root) = path.ancestors().nth(excess_depth) else {
        return None;
    };
    let is_real_windows_apps_root = windows_apps_roots.iter().any(|root| {
        package_root
            .parent()
            .is_some_and(|parent| normalized_path(parent) == normalized_path(root))
    });
    if !is_real_windows_apps_root {
        return None;
    }
    let package = &components[index + 1];
    if package.len() < 8 || package.matches('_').count() < 4 {
        return None;
    }
    Some(package_root.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::{
        is_protected_appdata_path, is_protected_install_location, is_protected_shared_path,
        is_specific_windows_apps_package, is_user_program_install_path,
        is_within_specific_windows_apps_package, ScanIdentity,
    };

    #[test]
    fn matching_requires_a_component_boundary() {
        let identity = ScanIdentity::from_name("Xplorer");
        assert!(identity.matches_component("Xplorer"));
        assert!(identity.matches_component("xplorer.exe"));
        assert!(!identity.matches_component("Internet Explorer"));
        assert!(!identity.matches_component("xplorer-backup"));
    }

    #[test]
    fn dotted_display_name_matches_itself_but_not_other_versions() {
        // DisplayName 自带版本点时会被 file_stem 截断（`Choice 9.0.0.1` →
        // `Choice 9.0.0`）；完整组件必须优先比较，否则 Uninstall 键永远
        // 无法按 DisplayName 匹配，失效安装的卸载残留无法清理。
        let identity = ScanIdentity::from_name("Choice 9.0.0.1");
        assert!(identity.matches_component("Choice 9.0.0.1"));
        assert!(identity.matches_display_name("Choice 9.0.0.1"));
        assert!(!identity.matches_display_name("Choice 9.0.0.2"));
        // 卸载键名与身份组件不同时依旧不匹配，保持保守边界。
        assert!(!identity.matches_component("ChoicePro"));
    }

    #[test]
    fn legacy_generic_suffix_alias_is_exact() {
        let identity = ScanIdentity::from_name("RustYu Legacy Test App");
        assert!(identity.matches_component("RustYuLegacyTest"));
        assert!(!identity.matches_component("RustYuLegacyTesting"));
    }

    #[test]
    fn protected_shell_and_system_paths_are_rejected() {
        assert!(is_protected_shared_path(std::path::Path::new(
            r"C:\Users\weiwang\AppData\Roaming\Microsoft\Internet Explorer\Quick Launch\User Pinned\TaskBar",
        )));
        assert!(is_protected_shared_path(std::path::Path::new(
            r"C:\Windows\System32\xplorer.dll",
        )));
        assert!(is_protected_appdata_path(std::path::Path::new(
            r"C:\Users\weiwang\AppData\Local\Packages\Demo\",
        )));
        assert!(is_protected_install_location(std::path::Path::new(
            r"C:\Users\weiwang\AppData\Roaming\Microsoft\Internet Explorer",
        )));
        assert!(!is_protected_install_location(std::path::Path::new(
            r"C:\Program Files\Xplorer",
        )));
        assert!(!is_protected_shared_path(std::path::Path::new(
            r"C:\Program Files\Xplorer\resources\app.dll",
        )));
    }

    #[test]
    fn exact_windows_apps_package_can_be_analyzed_without_unprotecting_the_root() {
        let package = std::path::Path::new(
            r"C:\Program Files\WindowsApps\OpenAI.ChatGPT_1.2.3.0_x64__2p2nqsd0c76g0",
        );
        assert!(is_specific_windows_apps_package(package));
        assert!(!is_protected_install_location(package));
        assert!(!is_within_specific_windows_apps_package(package));
        assert!(is_within_specific_windows_apps_package(
            &package.join("app")
        ));
        assert!(is_within_specific_windows_apps_package(
            &package.join("resources\\app.dll")
        ));
        assert!(is_protected_install_location(std::path::Path::new(
            r"C:\Program Files\WindowsApps",
        )));
        assert!(!is_specific_windows_apps_package(std::path::Path::new(
            r"C:\Program Files\WindowsApps\OpenAI.ChatGPT_1.2.3.0_x64__2p2nqsd0c76g0\assets",
        )));
        assert!(is_within_specific_windows_apps_package(
            std::path::Path::new(
                r"C:\Program Files\WindowsApps\OpenAI.ChatGPT_1.2.3.0_x64__2p2nqsd0c76g0\assets",
            )
        ));
        assert!(!is_specific_windows_apps_package(std::path::Path::new(
            r"C:\Temp\WindowsApps\OpenAI.ChatGPT_1.2.3.0_x64__2p2nqsd0c76g0",
        )));
    }

    #[test]
    fn local_appdata_programs_is_classified_as_an_install_location() {
        assert!(is_user_program_install_path(std::path::Path::new(
            r"C:\Users\demo\AppData\Local\Programs\Demo App"
        )));
        assert!(!is_user_program_install_path(std::path::Path::new(
            r"C:\Users\demo\AppData\Local\Demo App"
        )));
    }
}
