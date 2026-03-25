use rust_yu_lib::lister;
use rust_yu_lib::lister::models::{
    InstallSourceSelector, ListProgramsQuery, MetadataWarmupQuery, MetadataWarmupSummary,
    ProgramListResponse,
};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

use super::CommandError;

const DEFAULT_METADATA_PROGRESS_EVENT: &str = "installed-program-metadata-progress";

#[derive(Debug, Serialize, Deserialize)]
pub struct ListOptions {
    pub source: Option<String>,
    pub search: Option<String>,
    pub refresh: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetadataWarmupOptions {
    pub source: Option<String>,
    pub search: Option<String>,
    pub refresh: Option<bool>,
    pub icons: Option<bool>,
    pub sizes: Option<bool>,
    pub progress_event: Option<String>,
}

#[tauri::command]
pub async fn list_programs(
    options: Option<ListOptions>,
) -> Result<ProgramListResponse, CommandError> {
    let source = options
        .as_ref()
        .and_then(|value| value.source.as_deref())
        .map(parse_source)
        .transpose()?
        .unwrap_or(InstallSourceSelector::All);

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

#[tauri::command]
pub async fn warmup_program_metadata(
    app: AppHandle,
    options: Option<MetadataWarmupOptions>,
) -> Result<MetadataWarmupSummary, CommandError> {
    let icons = options
        .as_ref()
        .and_then(|value| value.icons)
        .unwrap_or(false);
    let sizes = options
        .as_ref()
        .and_then(|value| value.sizes)
        .unwrap_or(false);
    if !icons && !sizes {
        return Err(CommandError::with_code(
            "invalid_options",
            "至少需要选择 icons 或 sizes",
        ));
    }

    let source = options
        .as_ref()
        .and_then(|value| value.source.as_deref())
        .map(parse_source)
        .transpose()?
        .unwrap_or(InstallSourceSelector::All);
    let search = options.as_ref().and_then(|value| value.search.clone());
    let refresh = options
        .as_ref()
        .and_then(|value| value.refresh)
        .unwrap_or(false);
    let progress_event = options
        .as_ref()
        .and_then(|value| value.progress_event.clone())
        .unwrap_or_else(|| DEFAULT_METADATA_PROGRESS_EVENT.to_string());

    let query = MetadataWarmupQuery {
        source,
        search,
        refresh,
        cache_ttl_seconds: rust_yu_lib::lister::storage::DEFAULT_CACHE_TTL_SECONDS,
        icons,
        sizes,
    };

    let join_result = tauri::async_runtime::spawn_blocking(move || {
        lister::warmup_program_metadata(query, |progress| {
            let _ = app.emit(&progress_event, &progress);
        })
    })
    .await
    .map_err(|error| CommandError::new(format!("元数据预热任务执行失败: {}", error)))?;

    join_result.map_err(CommandError::from)
}

fn parse_source(source: &str) -> Result<InstallSourceSelector, CommandError> {
    InstallSourceSelector::parse(source)
        .ok_or_else(|| CommandError::with_code("invalid_selector", format!("未知来源: {source}")))
}

#[cfg(test)]
mod tests {
    use super::parse_source;
    use rust_yu_lib::lister::models::InstallSourceSelector;

    #[test]
    fn parse_source_accepts_supported_aliases() {
        assert_eq!(
            parse_source("registry").ok(),
            Some(InstallSourceSelector::Registry)
        );
        assert_eq!(parse_source("MSI").ok(), Some(InstallSourceSelector::Msi));
        assert_eq!(parse_source("all").ok(), Some(InstallSourceSelector::All));
    }

    #[test]
    fn parse_source_rejects_unknown_values() {
        let error = parse_source("winget").expect_err("expected invalid selector");

        assert_eq!(error.code.as_deref(), Some("invalid_selector"));
    }
}
