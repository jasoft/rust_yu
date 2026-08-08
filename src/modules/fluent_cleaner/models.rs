use serde::{Deserialize, Serialize};

#[derive(Debug, Clone)]
pub(crate) struct CleanerEntry {
    pub id: String,
    pub name: String,
    pub category: String,
    pub warning: Option<String>,
    pub default_enabled: bool,
    pub detect_keys: Vec<String>,
    pub detect_files: Vec<String>,
    pub file_keys: Vec<FileKey>,
    pub registry_keys: Vec<RegistryKey>,
    pub exclusions: Vec<Exclusion>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FileKeyFlag {
    None,
    Recurse,
    RemoveSelf,
}

#[derive(Debug, Clone)]
pub(crate) struct FileKey {
    pub path: String,
    pub patterns: Vec<String>,
    pub flag: FileKeyFlag,
}

#[derive(Debug, Clone)]
pub(crate) struct RegistryKey {
    pub path: String,
    pub value_name: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExclusionKind {
    File,
    Path,
    Registry,
}

#[derive(Debug, Clone)]
pub(crate) struct Exclusion {
    pub kind: ExclusionKind,
    pub path: String,
    pub pattern: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CleanerEntrySummary {
    pub id: String,
    pub name: String,
    pub category: String,
    pub warning: Option<String>,
    pub default_enabled: bool,
    pub file_rule_count: usize,
    pub registry_rule_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct CleanerCatalog {
    pub entries: Vec<CleanerEntrySummary>,
    pub database_version: String,
    pub total_rule_count: usize,
    pub detected_rule_count: usize,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum CleanerTargetKind {
    File,
    RegistryKey,
    RegistryValue,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanerTarget {
    pub id: String,
    pub entry_id: String,
    pub entry_name: String,
    pub kind: CleanerTargetKind,
    pub path: String,
    pub value_name: Option<String>,
    pub size: u64,
    pub requires_admin: bool,
    pub blocked_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CleanerScanResult {
    pub targets: Vec<CleanerTarget>,
    pub total_bytes: u64,
    pub truncated: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CleanSelection {
    pub entry_ids: Vec<String>,
    pub target_ids: Vec<String>,
    pub confirm: bool,
    pub dry_run: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct CleanerCleanItemResult {
    pub target_id: String,
    pub path: String,
    pub success: bool,
    pub error: Option<String>,
    pub bytes_freed: u64,
    pub dry_run: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct CleanerCleanResult {
    pub items: Vec<CleanerCleanItemResult>,
    pub bytes_freed: u64,
}
