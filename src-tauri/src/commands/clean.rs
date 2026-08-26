use rust_yu_lib::cleaner;
use rust_yu_lib::cleaner::models::CleanResult;
use rust_yu_lib::scanner::models::Trace;
use serde::{Deserialize, Serialize};

use super::{require_administrator, CommandError};

#[derive(Debug, Serialize, Deserialize)]
pub struct CleanOptions {
    pub traces: Vec<Trace>,
    pub confirm: bool,
    pub preview: bool,
    #[serde(default)]
    pub reviewed: bool,
}

#[tauri::command]
pub async fn clean_traces(options: CleanOptions) -> Result<Vec<CleanResult>, CommandError> {
    // 预览模式只返回空结果，不执行清理
    if options.preview {
        return Ok(vec![]);
    }
    require_administrator()?;

    // 残留复核和深度分析都是用户逐项确认后的清理；这里保留关键项拦截、
    // 保护路径重检和备份，只放宽“低置信度一律拒绝”的默认策略。
    let results = if options.reviewed {
        cleaner::clean_reviewed_traces(options.traces, options.confirm)
            .await
            .map_err(CommandError::from)?
    } else {
        cleaner::clean_traces(options.traces, options.confirm)
            .await
            .map_err(CommandError::from)?
    };

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::CleanOptions;

    #[test]
    fn missing_reviewed_flag_keeps_safe_default_policy() {
        let options: CleanOptions =
            serde_json::from_str(r#"{"traces":[],"confirm":true,"preview":false}"#)
                .unwrap_or_else(|error| panic!("deserialize legacy cleanup options: {error}"));

        assert!(!options.reviewed);
    }
}
