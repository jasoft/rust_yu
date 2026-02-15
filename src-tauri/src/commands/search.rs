use rust_yu_lib::lister;
use rust_yu_lib::lister::models::InstalledProgram;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct SearchOptions {
    pub query: String,
    pub source: Option<String>,
}

#[tauri::command]
pub async fn search_programs(query: String) -> Result<Vec<InstalledProgram>, String> {
    // 搜索功能使用 list_all_programs 的 search 参数
    let programs = lister::list_all_programs(None, Some(&query))
        .map_err(|e| e.to_string())?;

    Ok(programs)
}
