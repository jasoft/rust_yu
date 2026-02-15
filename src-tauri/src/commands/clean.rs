use rust_yu_lib::cleaner;
use rust_yu_lib::cleaner::models::CleanResult;
use rust_yu_lib::scanner::models::Trace;
use serde::{Deserialize, Serialize};

use super::CommandError;

#[derive(Debug, Serialize, Deserialize)]
pub struct CleanOptions {
    pub traces: Vec<Trace>,
    pub confirm: bool,
    pub preview: bool,
}

#[tauri::command]
pub async fn clean_traces(options: CleanOptions) -> Result<Vec<CleanResult>, CommandError> {
    // 预览模式只返回空结果，不执行清理
    if options.preview {
        return Ok(vec![]);
    }

    let results = cleaner::clean_traces(options.traces, options.confirm)
        .await
        .map_err(CommandError::from)?;

    Ok(results)
}
