use serde::{Deserialize, Serialize};

/// 删除操作结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanResult {
    pub trace_id: String,
    pub path: String,
    pub success: bool,
    pub error: Option<String>,
    pub bytes_freed: u64,
    /// 清理前备份会话 ID；系统服务/任务等不可安全回滚的项目为空。
    #[serde(default)]
    pub backup_id: Option<String>,
}
