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
    pub source: Option<InstallSource>,
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
