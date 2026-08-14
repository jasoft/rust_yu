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
    pub fn matches_component(&self, component: &str) -> bool {
        let without_extension = Path::new(component)
            .file_stem()
            .and_then(|value| value.to_str())
            .unwrap_or(component);
        let normalized = compact_identifier(without_extension);
        !normalized.is_empty() && self.aliases.iter().any(|alias| alias == &normalized)
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

pub fn is_protected_install_location(path: &Path) -> bool {
    let components = path_components(path);
    if components.is_empty() {
        return true;
    }

    is_protected_shared_path(path)
        || contains_sequence(&components, &["appdata", "roaming", "microsoft"])
        || contains_sequence(&components, &["appdata", "local", "microsoft"])
        || contains_sequence(&components, &["programdata", "microsoft"])
}

#[cfg(test)]
mod tests {
    use super::{
        is_protected_appdata_path, is_protected_install_location, is_protected_shared_path,
        ScanIdentity,
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
}
