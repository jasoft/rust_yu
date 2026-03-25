use serde::{Deserialize, Serialize};

/// 元数据置信度
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MetadataConfidence {
    High,
    Medium,
    Low,
    #[default]
    Unknown,
}

impl MetadataConfidence {
    fn score(self) -> u8 {
        match self {
            Self::High => 4,
            Self::Medium => 3,
            Self::Low => 2,
            Self::Unknown => 1,
        }
    }

    /// 取一组置信度中的最低值，确保对外展示保守
    pub fn lowest(values: &[Self]) -> Self {
        values
            .iter()
            .copied()
            .min_by_key(|value| value.score())
            .unwrap_or(Self::Unknown)
    }
}

/// 元数据来源
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MetadataSource {
    Registry,
    Filesystem,
    Derived,
    #[default]
    Unknown,
}

/// 已安装程序
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledProgram {
    pub id: String,
    pub name: String,
    pub publisher: Option<String>,
    pub version: Option<String>,
    pub install_date: Option<String>,
    pub install_location: Option<String>,
    pub uninstall_string: Option<String>,
    #[serde(default)]
    pub quiet_uninstall_string: Option<String>,
    #[serde(default)]
    pub install_source: InstallSource,
    pub size: Option<u64>,
    pub icon_path: Option<String>,
    #[serde(default)]
    pub icon_cache_path_32: Option<String>,
    #[serde(default)]
    pub icon_cache_path_48: Option<String>,
    #[serde(default)]
    pub size_last_updated_at: Option<String>,
    #[serde(default)]
    pub icon_data_url: Option<String>,
    #[serde(default)]
    pub icon_data_url_32: Option<String>,
    #[serde(default)]
    pub icon_data_url_48: Option<String>,
    pub estimated_size: Option<u64>,
    pub display_version: Option<String>,
    pub url_info_about: Option<String>,
    pub help_link: Option<String>,
    #[serde(default)]
    pub install_date_source: MetadataSource,
    #[serde(default)]
    pub install_date_confidence: MetadataConfidence,
    #[serde(default)]
    pub icon_source: MetadataSource,
    #[serde(default)]
    pub icon_confidence: MetadataConfidence,
    #[serde(default)]
    pub size_source: MetadataSource,
    #[serde(default)]
    pub size_confidence: MetadataConfidence,
    #[serde(default)]
    pub metadata_confidence: MetadataConfidence,
}

impl InstalledProgram {
    pub fn new(name: String, source: InstallSource) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            publisher: None,
            version: None,
            install_date: None,
            install_location: None,
            uninstall_string: None,
            quiet_uninstall_string: None,
            install_source: source,
            size: None,
            icon_path: None,
            icon_cache_path_32: None,
            icon_cache_path_48: None,
            size_last_updated_at: None,
            icon_data_url: None,
            icon_data_url_32: None,
            icon_data_url_48: None,
            estimated_size: None,
            display_version: None,
            url_info_about: None,
            help_link: None,
            install_date_source: MetadataSource::Unknown,
            install_date_confidence: MetadataConfidence::Unknown,
            icon_source: MetadataSource::Unknown,
            icon_confidence: MetadataConfidence::Unknown,
            size_source: MetadataSource::Unknown,
            size_confidence: MetadataConfidence::Unknown,
            metadata_confidence: MetadataConfidence::Unknown,
        }
    }

    /// 返回卸载时优先使用的命令。
    pub fn preferred_uninstall_string(&self) -> Option<&str> {
        self.quiet_uninstall_string
            .as_deref()
            .or(self.uninstall_string.as_deref())
    }
}

/// 安装来源
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InstallSource {
    /// 注册表中的 Win32 安装程序
    Registry,
    /// MSI 安装包
    Msi,
    /// 微软商店应用 (UWP)
    Store,
    /// 未知来源
    Unknown,
}

impl Default for InstallSource {
    fn default() -> Self {
        InstallSource::Unknown
    }
}

impl std::fmt::Display for InstallSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InstallSource::Registry => write!(f, "Registry"),
            InstallSource::Msi => write!(f, "MSI"),
            InstallSource::Store => write!(f, "Store"),
            InstallSource::Unknown => write!(f, "Unknown"),
        }
    }
}

const STANDARD_INSTALL_SOURCES: [InstallSource; 1] = [InstallSource::Registry];
const REGISTRY_INSTALL_SOURCES: [InstallSource; 1] = [InstallSource::Registry];
const MSI_INSTALL_SOURCES: [InstallSource; 1] = [InstallSource::Msi];
const STORE_INSTALL_SOURCES: [InstallSource; 1] = [InstallSource::Store];
const ALL_INSTALL_SOURCES: [InstallSource; 3] = [
    InstallSource::Registry,
    InstallSource::Msi,
    InstallSource::Store,
];

/// 列表查询时使用的来源选择器
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum InstallSourceSelector {
    /// 默认快速模式，仅扫描注册表
    #[default]
    Standard,
    Registry,
    Msi,
    Store,
    All,
}

impl InstallSourceSelector {
    pub fn parse(value: &str) -> Option<Self> {
        match value.to_lowercase().as_str() {
            "standard" => Some(Self::Standard),
            "registry" => Some(Self::Registry),
            "msi" => Some(Self::Msi),
            "store" => Some(Self::Store),
            "all" => Some(Self::All),
            _ => None,
        }
    }

    pub fn is_cache_eligible(self) -> bool {
        matches!(self, Self::Standard | Self::Registry)
    }

    pub fn install_sources(self) -> &'static [InstallSource] {
        match self {
            Self::Standard => &STANDARD_INSTALL_SOURCES,
            Self::Registry => &REGISTRY_INSTALL_SOURCES,
            Self::Msi => &MSI_INSTALL_SOURCES,
            Self::Store => &STORE_INSTALL_SOURCES,
            Self::All => &ALL_INSTALL_SOURCES,
        }
    }

    pub fn from_install_source(source: Option<InstallSource>) -> Self {
        match source {
            Some(InstallSource::Registry) => Self::Registry,
            Some(InstallSource::Msi) => Self::Msi,
            Some(InstallSource::Store) => Self::Store,
            Some(InstallSource::Unknown) | None => Self::Standard,
        }
    }
}

/// 元数据预热任务类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetadataWarmupKind {
    Icons,
    Sizes,
}

impl MetadataWarmupKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Icons => "icons",
            Self::Sizes => "sizes",
        }
    }
}

impl std::fmt::Display for MetadataWarmupKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// 元数据预热进度阶段
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetadataWarmupStage {
    Started,
    ItemStarted,
    ItemFinished,
    Completed,
}

/// 单个预热条目的结果
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MetadataWarmupItemStatus {
    Updated,
    Skipped,
    Failed,
}

/// 预热任务统计
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MetadataWarmupStats {
    pub total: usize,
    pub eligible: usize,
    pub processed: usize,
    pub updated: usize,
    pub skipped: usize,
    pub failed: usize,
}

/// 预热任务进度事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetadataWarmupProgress {
    pub kind: MetadataWarmupKind,
    pub stage: MetadataWarmupStage,
    pub current: usize,
    pub total: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub program_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub program_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<MetadataWarmupItemStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub program: Option<InstalledProgram>,
}

/// 元数据预热参数
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MetadataWarmupQuery {
    pub source: InstallSourceSelector,
    pub search: Option<String>,
    pub refresh: bool,
    pub cache_ttl_seconds: i64,
    pub icons: bool,
    pub sizes: bool,
}

impl MetadataWarmupQuery {
    pub fn selected_kinds(&self) -> Vec<MetadataWarmupKind> {
        let mut kinds = Vec::new();
        if self.icons {
            kinds.push(MetadataWarmupKind::Icons);
        }
        if self.sizes {
            kinds.push(MetadataWarmupKind::Sizes);
        }
        kinds
    }
}

/// 元数据预热结果
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MetadataWarmupSummary {
    pub total_programs: usize,
    pub matched_programs: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icons: Option<MetadataWarmupStats>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sizes: Option<MetadataWarmupStats>,
    pub cache: ProgramListCacheState,
}

/// 列表缓存状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgramListCacheState {
    pub cache_hit: bool,
    pub cache_valid: bool,
    pub refreshed: bool,
    pub schema_version: u32,
    pub generated_at: Option<String>,
    pub reason: Option<String>,
}

impl Default for ProgramListCacheState {
    fn default() -> Self {
        Self {
            cache_hit: false,
            cache_valid: false,
            refreshed: false,
            schema_version: 0,
            generated_at: None,
            reason: None,
        }
    }
}

/// 列表查询参数
#[derive(Debug, Clone, Default)]
pub struct ListProgramsQuery {
    pub source: InstallSourceSelector,
    pub search: Option<String>,
    pub refresh: bool,
    pub cache_ttl_seconds: i64,
}

/// 列表查询返回
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgramListResponse {
    pub programs: Vec<InstalledProgram>,
    pub cache: ProgramListCacheState,
}

#[cfg(test)]
mod tests {
    use super::{InstallSource, InstalledProgram};

    #[test]
    fn preferred_uninstall_string_prefers_quiet_variant() {
        let mut program = InstalledProgram::new("QuietApp".to_string(), InstallSource::Registry);
        program.uninstall_string = Some(r#""C:\Program Files\QuietApp\uninstall.exe""#.to_string());
        program.quiet_uninstall_string =
            Some(r#""C:\Program Files\QuietApp\uninstall.exe" /S"#.to_string());

        assert_eq!(
            program.preferred_uninstall_string(),
            Some(r#""C:\Program Files\QuietApp\uninstall.exe" /S"#)
        );
    }

    #[test]
    fn preferred_uninstall_string_falls_back_to_default_variant() {
        let mut program = InstalledProgram::new("FallbackApp".to_string(), InstallSource::Registry);
        program.uninstall_string =
            Some(r#""C:\Program Files\FallbackApp\uninstall.exe""#.to_string());

        assert_eq!(
            program.preferred_uninstall_string(),
            Some(r#""C:\Program Files\FallbackApp\uninstall.exe""#)
        );
    }
}
