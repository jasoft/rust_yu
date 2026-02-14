use serde::{Deserialize, Serialize};

/// 删除操作结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanResult {
    pub trace_id: String,
    pub path: String,
    pub success: bool,
    pub error: Option<String>,
    pub bytes_freed: u64,
}
