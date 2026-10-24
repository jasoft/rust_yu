//! Residue scanner identity and path-safety boundaries.
//!
//! A residue scanner must never treat an arbitrary substring in a Windows path
//! as ownership evidence.  This module keeps the matching and protected-area
//! rules shared by filesystem, AppData and shortcut scanners.

use crate::modules::lister::models::InstalledProgram;
use std::path::{Path, PathBuf};

const GENERIC_SUFFIXES: &[&str] = &[
    "app",
    "application",
    "client",
    "software",
    "suite",
    "test",
    "tool",
];

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

        // Legacy installers often omit one or several generic trailing words:
        // `RustYu Dirty Traces Test App` commonly leaves vendor keys and data
        // directories named `RustYu Dirty Traces`.  Strip consecutive generic
        // suffixes only — never arbitrary prefixes or substrings; each alias
        // must stay long enough (>=6) so short unrelated names (e.g. `Foo`)
        // can never match, and `Internet Explorer`-style names stay distinct.
        let mut tokens = display_name
            .split(|character: char| !character.is_ascii_alphanumeric())
            .filter(|token| !token.is_empty())
            .map(|token| token.to_ascii_lowercase())
            .collect::<Vec<_>>();
        while tokens.len() > 1
            && tokens
                .last()
                .is_some_and(|token| GENERIC_SUFFIXES.contains(&token.as_str()))
        {
            tokens.pop();
            let alias = compact_identifier(&tokens.join(" "));
            if alias.len() < 6 || aliases.contains(&alias) {
                break;
            }
            aliases.push(alias);
        }

        Self {
            display_name,
            aliases,
        }
    }

    pub fn from_program(program: &InstalledProgram) -> Self {
        let mut identity = Self::from_name(&program.name);
        let Some(location) = program.install_location.as_deref() else {
            return identity;
        };
        let path = Path::new(location.trim().trim_matches('"'));
        if !path.is_absolute()
            || path
                .components()
                .any(|part| matches!(part, std::path::Component::ParentDir))
            || is_protected_install_location(path)
            || crate::modules::common::utils::is_system_critical_path(location)
        {
            return identity;
        }
        let Some(leaf) = path.file_name().and_then(|name| name.to_str()) else {
            return identity;
        };
        let alias = compact_identifier(leaf);
        // 显示名可能带发行标签，AppData/厂商键仍沿用私有标识。不能只凭
        // InstallLocation 的目录名扩大匹配：必须同时得到 ARP 键名和图标
        // 程序名的精确佐证；Helper/Runtime 等通用组件绝不能扩散成身份。
        if alias.len() < 6
            || GENERIC_SUFFIXES.contains(&alias.as_str())
            || [
                "helper",
                "runtime",
                "shared",
                "common",
                "browser",
                "service",
                "resources",
                "programs",
                "components",
            ]
            .contains(&alias.as_str())
            || !compact_identifier(&program.name).starts_with(&alias)
            || identity.aliases.contains(&alias)
        {
            return identity;
        }
        let arp_alias = program
            .uninstall_registry_key_path
            .as_deref()
            .and_then(|key| Path::new(key).file_name())
            .and_then(|name| name.to_str())
            .map(compact_identifier);
        let icon_alias = program
            .icon_path
            .as_deref()
            .and_then(|icon| Path::new(icon.trim().trim_matches('"')).file_stem())
            .and_then(|name| name.to_str())
            .map(compact_identifier);
        if arp_alias.as_deref() == Some(alias.as_str())
            && icon_alias.as_deref() == Some(alias.as_str())
        {
            // 这里只补名称候选，不提升已有置信度或绕过共享/备份/确认闸门。
            // 不依赖目录仍存在，卸载前缓存也必须能发现卸载后的用户数据。
            identity.aliases.push(alias);
        }
        identity
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
    fn consecutive_generic_suffixes_stay_matchable() {
        // 真实软件常用去掉多个尾部通用词的缩短名落厂商键/数据目录：
        // "RustYu Dirty Traces Test App" 的残留键叫 "RustYu Dirty Traces"。
        let identity = ScanIdentity::from_name("RustYu Dirty Traces Test App");
        assert!(identity.matches_component("RustYu Dirty Traces Test App"));
        assert!(identity.matches_component("RustYu Dirty Traces Test"));
        assert!(identity.matches_component("RustYu Dirty Traces"));
        // 只允许剥尾部通用词：改词、加词、换前缀都不得命中
        assert!(!identity.matches_component("RustYu Dirty Traces Viewer"));
        assert!(!identity.matches_component("Dirty Traces Test App"));
        assert!(!identity.matches_component("RustYu Dirty"));
    }

    #[test]
    fn short_aliases_after_stripping_are_rejected() {
        // 剥完通用词后长度不足 6 的别名必须被拒绝，防止短名误报
        let identity = ScanIdentity::from_name("Foo Test App Client");
        assert!(identity.matches_component("Foo Test App Client"));
        assert!(identity.matches_component("Foo Test App"));
        // "footest"（7 位）可用，但 "foo"（3 位）不可用
        assert!(identity.matches_component("Foo Test"));
        assert!(!identity.matches_component("Foo"));
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
