use rust_yu_lib::lister;
use rust_yu_lib::lister::models::InstalledProgram;
use serde::{Deserialize, Serialize};

use super::CommandError;

#[derive(Debug, Serialize, Deserialize)]
pub struct SearchOptions {
    pub query: String,
    pub source: Option<String>,
}

#[tauri::command]
pub async fn search_programs(query: String) -> Result<Vec<InstalledProgram>, CommandError> {
    // 搜索功能使用 list_all_programs 的 search 参数
    let programs = lister::list_all_programs(None, Some(&query)).map_err(CommandError::from)?;

    Ok(programs)
}
