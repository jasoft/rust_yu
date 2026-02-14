use serde::{Deserialize, Serialize};

/// 痕迹类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TraceType {
    /// 注册表项
    RegistryKey,
    /// 注册表值
    RegistryValue,
    /// 文件或目录
    File,
    /// 快捷方式 (.lnk)
    Shortcut,
    /// AppData/LocalAppData
    AppData,
    /// 计划任务
    ScheduledTask,
    /// 服务
    Service,
    /// 驱动程序
    Driver,
}

impl Default for TraceType {
    fn default() -> Self {
        TraceType::File
    }
}

impl std::fmt::Display for TraceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TraceType::RegistryKey => write!(f, "RegistryKey"),
            TraceType::RegistryValue => write!(f, "RegistryValue"),
            TraceType::File => write!(f, "File"),
            TraceType::Shortcut => write!(f, "Shortcut"),
            TraceType::AppData => write!(f, "AppData"),
            TraceType::ScheduledTask => write!(f, "ScheduledTask"),
            TraceType::Service => write!(f, "Service"),
            TraceType::Driver => write!(f, "Driver"),
        }
    }
}

/// 匹配置信度
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, PartialOrd, Ord)]
pub enum Confidence {
    /// 高置信度 - 直接关联
    High,
    /// 中置信度 - 名称相似
    Medium,
    /// 低置信度 - 可能相关
    Low,
}

impl Default for Confidence {
    fn default() -> Self {
        Confidence::Low
    }
}

/// 痕迹项目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trace {
    pub id: String,
    pub program_name: String,
    pub trace_type: TraceType,
    pub path: String,
    pub description: String,
    pub size: Option<u64>,
    pub is_critical: bool,
    pub confidence: Confidence,
    pub exists: bool,
}

impl Trace {
    pub fn new(program_name: String, trace_type: TraceType, path: String) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            program_name,
            trace_type,
            path,
            description: String::new(),
            size: None,
            is_critical: false,
            confidence: Confidence::Low,
            exists: true,
        }
    }

    pub fn with_description(mut self, description: String) -> Self {
        self.description = description;
        self
    }

    pub fn with_size(mut self, size: u64) -> Self {
        self.size = Some(size);
        self
    }

    pub fn with_confidence(mut self, confidence: Confidence) -> Self {
        self.confidence = confidence;
        self
    }
}
