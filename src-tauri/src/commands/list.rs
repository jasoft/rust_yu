use rust_yu_lib::lister;
use rust_yu_lib::lister::models::{InstallSource, ListProgramsQuery, ProgramListResponse};
use serde::{Deserialize, Serialize};

use super::CommandError;

#[derive(Debug, Serialize, Deserialize)]
pub struct ListOptions {
    pub source: Option<String>,
    pub search: Option<String>,
    pub refresh: Option<bool>,
}

#[tauri::command]
pub async fn list_programs(
    options: Option<ListOptions>,
) -> Result<ProgramListResponse, CommandError> {
    let source = options.as_ref().and_then(|o| o.source.as_deref()).map(|s| {
        match s.to_lowercase().as_str() {
            "registry" => InstallSource::Registry,
            "msi" => InstallSource::Msi,
            "store" => InstallSource::Store,
            _ => InstallSource::Registry,
        }
    });

    let search = options.as_ref().and_then(|o| o.search.clone());
    let refresh = options.as_ref().and_then(|o| o.refresh).unwrap_or(false);

    let query = ListProgramsQuery {
        source,
        search,
        refresh,
        cache_ttl_seconds: rust_yu_lib::lister::storage::DEFAULT_CACHE_TTL_SECONDS,
    };

    let join_result =
        tauri::async_runtime::spawn_blocking(move || lister::list_programs_with_cache(query))
            .await
            .map_err(|error| CommandError::new(format!("程序列表任务执行失败: {}", error)))?;

    join_result.map_err(CommandError::from)
}
