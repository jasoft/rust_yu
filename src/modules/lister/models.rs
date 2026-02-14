use serde::{Deserialize, Serialize};

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
    pub install_source: InstallSource,
    pub size: Option<u64>,
    pub icon_path: Option<String>,
    pub estimated_size: Option<u64>,
    pub display_version: Option<String>,
    pub url_info_about: Option<String>,
    pub help_link: Option<String>,
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
            estimated_size: None,
            display_version: None,
            url_info_about: None,
            help_link: None,
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
