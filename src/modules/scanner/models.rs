use serde::{Deserialize, Serialize};

/// 痕迹类型。对外统一使用 snake_case，同时接受旧快照中的 PascalCase。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TraceType {
    /// 注册表项
    #[serde(rename = "registry_key", alias = "RegistryKey")]
    RegistryKey,
    /// 注册表值
    #[serde(rename = "registry_value", alias = "RegistryValue")]
    RegistryValue,
    /// 文件或目录
    #[serde(rename = "file", alias = "File")]
    File,
    /// 快捷方式 (.lnk)
    #[serde(rename = "shortcut", alias = "Shortcut")]
    Shortcut,
    /// AppData/LocalAppData
    #[serde(rename = "appdata", alias = "AppData")]
    AppData,
    /// 计划任务
    #[serde(rename = "scheduled_task", alias = "ScheduledTask")]
    ScheduledTask,
    /// 服务
    #[serde(rename = "service", alias = "Service")]
    Service,
    /// 驱动程序
    #[serde(rename = "driver", alias = "Driver")]
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
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    /// 高置信度 - 直接关联
    #[serde(alias = "High")]
    High,
    /// 中置信度 - 名称相似
    #[serde(alias = "Medium")]
    Medium,
    /// 低置信度 - 可能相关
    #[serde(alias = "Low")]
    Low,
}

impl Default for Confidence {
    fn default() -> Self {
        Confidence::Low
    }
}

/// 置信度的可解释证据来源。前端展示它，而不是要求用户猜测“高/中/低”的由来。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfidenceReason {
    #[default]
    Unknown,
    ExplicitInstallLocation,
    StorePackageInstallLocation,
    StorePackageUserData,
    UninstallRegistry,
    RegistryValueReferencesInstallLocation,
    SystemCommandReferencesInstallLocation,
    ExactNameRegistry,
    ExactNameStandardLocation,
    ExactNameUserData,
    ExactShortcutName,
}

pub fn confidence_for_score(score: u8) -> Confidence {
    match score {
        80..=u8::MAX => Confidence::High,
        50..=79 => Confidence::Medium,
        _ => Confidence::Low,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScanArea {
    Registry,
    InstallFiles,
    UserData,
    Shortcuts,
    ScheduledTasks,
    Services,
    Drivers,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScanAreaStatus {
    Complete,
    Partial,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScanCoverage {
    pub area: ScanArea,
    pub status: ScanAreaStatus,
    pub item_count: usize,
}

impl ScanCoverage {
    pub fn new(area: ScanArea, status: ScanAreaStatus) -> Self {
        Self {
            area,
            status,
            item_count: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanReport {
    pub traces: Vec<Trace>,
    pub coverage: Vec<ScanCoverage>,
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
    /// 仍被其他已安装程序共享使用（如共用安装目录），删除会误伤其他程序
    #[serde(default)]
    pub is_shared: bool,
    pub confidence: Confidence,
    /// 0-100 的证据强度分数。0 表示旧快照尚未分配可解释证据。
    #[serde(default)]
    pub confidence_score: u8,
    #[serde(default)]
    pub confidence_reason: ConfidenceReason,
    pub exists: bool,
    /// 系统集成痕迹的关联路径，用于破坏性操作前重新核对当前目标。
    #[serde(default)]
    pub related_path: Option<String>,
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
            is_shared: false,
            confidence: Confidence::Low,
            confidence_score: 0,
            confidence_reason: ConfidenceReason::Unknown,
            exists: true,
            related_path: None,
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
        self.confidence_score = match confidence {
            Confidence::High => 90,
            Confidence::Medium => 60,
            Confidence::Low => 25,
        };
        self.confidence_reason = ConfidenceReason::Unknown;
        self
    }

    pub fn with_confidence_evidence(mut self, score: u8, reason: ConfidenceReason) -> Self {
        self.confidence_score = score.min(100);
        self.confidence = confidence_for_score(self.confidence_score);
        self.confidence_reason = reason;
        self
    }

    pub fn with_related_path(mut self, related_path: String) -> Self {
        self.related_path = Some(related_path);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::{confidence_for_score, Confidence, ConfidenceReason, Trace, TraceType};

    #[test]
    fn confidence_serialization_matches_webui_contract() {
        let cases = [
            (Confidence::High, "\"high\""),
            (Confidence::Medium, "\"medium\""),
            (Confidence::Low, "\"low\""),
        ];

        for (confidence, expected) in cases {
            let serialized = serde_json::to_string(&confidence)
                .unwrap_or_else(|error| panic!("serialize confidence: {error}"));
            assert_eq!(serialized, expected);

            let round_trip: Confidence = serde_json::from_str(expected)
                .unwrap_or_else(|error| panic!("deserialize confidence: {error}"));
            assert_eq!(round_trip, confidence);
        }

        assert_eq!(
            serde_json::from_str::<Confidence>("\"High\"").ok(),
            Some(Confidence::High),
        );
    }

    #[test]
    fn trace_type_display_is_stable() {
        assert_eq!(TraceType::RegistryKey.to_string(), "RegistryKey");
        assert_eq!(TraceType::Shortcut.to_string(), "Shortcut");
    }

    #[test]
    fn trace_type_wire_format_is_snake_case_and_reads_legacy_case() {
        assert!(matches!(
            serde_json::to_string(&TraceType::ScheduledTask).as_deref(),
            Ok("\"scheduled_task\"")
        ));
        assert_eq!(
            serde_json::from_str::<TraceType>("\"Service\"").ok(),
            Some(TraceType::Service),
        );
    }

    #[test]
    fn trace_builder_methods_update_expected_fields() {
        let trace = Trace::new(
            "Demo App".to_string(),
            TraceType::File,
            r"C:\Demo App".to_string(),
        )
        .with_description("leftover directory".to_string())
        .with_size(2048)
        .with_confidence_evidence(95, ConfidenceReason::ExplicitInstallLocation);

        assert_eq!(trace.description, "leftover directory");
        assert_eq!(trace.size, Some(2048));
        assert_eq!(trace.confidence, Confidence::High);
        assert_eq!(trace.confidence_score, 95);
        assert_eq!(
            trace.confidence_reason,
            ConfidenceReason::ExplicitInstallLocation
        );
        assert!(trace.exists);
        assert!(!trace.is_critical);
        assert_eq!(trace.related_path, None);
    }

    #[test]
    fn confidence_score_thresholds_are_stable() {
        assert_eq!(confidence_for_score(100), Confidence::High);
        assert_eq!(confidence_for_score(80), Confidence::High);
        assert_eq!(confidence_for_score(79), Confidence::Medium);
        assert_eq!(confidence_for_score(50), Confidence::Medium);
        assert_eq!(confidence_for_score(49), Confidence::Low);
    }
}
