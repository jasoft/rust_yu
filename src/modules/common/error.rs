use thiserror::Error;

#[derive(Error, Debug)]
#[allow(dead_code)]
pub enum UninstallerError {
    #[error("注册表错误: {0}")]
    Registry(String),

    #[error("文件系统错误: {0}")]
    FileSystem(#[from] std::io::Error),

    #[error("MSI 错误: {0}")]
    Msi(String),

    #[error("商店应用错误: {0}")]
    StoreApp(String),

    #[error("权限不足: {0}")]
    PermissionDenied(String),

    #[error("关键系统项: {0}")]
    CriticalSystemItem(String),

    #[error("未找到: {0}")]
    NotFound(String),

    #[error("COM 错误: {0}")]
    Com(String),

    #[error("其他错误: {0}")]
    Other(String),

    #[error("超时: {0}")]
    Timeout(String),

    #[error("序列化错误: {0}")]
    Serde(String),
}


impl serde::Serialize for UninstallerError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}
