pub mod enrichment;
pub mod models;
pub mod msi;
pub mod registry;
pub mod storage;
pub mod store;

use chrono::Utc;

use crate::modules::common::error::UninstallerError;
use crate::modules::common::utils;
use models::{
    InstallSource, InstalledProgram, ListProgramsQuery, ProgramListCacheState, ProgramListResponse,
};

/// 列出所有已安装程序（兼容旧接口）
pub fn list_all_programs(
    source: Option<InstallSource>,
    search: Option<&str>,
) -> Result<Vec<InstalledProgram>, UninstallerError> {
    let mut all_programs = collect_programs(source);
    enrichment::enrich_programs(&mut all_programs);
    dedupe_and_sort(&mut all_programs);
    apply_search_filter(&mut all_programs, search);
    Ok(all_programs)
}

/// 列出程序（增强版，含缓存状态）
pub fn list_programs_with_cache(
    mut query: ListProgramsQuery,
) -> Result<ProgramListResponse, UninstallerError> {
    if query.cache_ttl_seconds <= 0 {
        query.cache_ttl_seconds = storage::DEFAULT_CACHE_TTL_SECONDS;
    }

    let cache_eligible = is_cache_eligible(query.source);
    let mut cache_state = ProgramListCacheState {
        schema_version: storage::CACHE_SCHEMA_VERSION,
        ..ProgramListCacheState::default()
    };

    if cache_eligible && !query.refresh {
        let cached = storage::read_scan_cache(query.cache_ttl_seconds)?;
        cache_state.schema_version = cached.schema_version;
        cache_state.generated_at = cached.generated_at.clone();
        cache_state.reason = cached.reason.clone();

        if cached.cache_hit && cached.cache_valid {
            let mut cached_programs = cached.entries.unwrap_or_default();
            apply_search_filter(&mut cached_programs, query.search.as_deref());
            cache_state.cache_hit = true;
            cache_state.cache_valid = true;
            cache_state.refreshed = false;
            return Ok(ProgramListResponse {
                programs: cached_programs,
                cache: cache_state,
            });
        }
    }

    let mut all_programs = collect_programs(query.source);
    enrichment::enrich_programs(&mut all_programs);
    dedupe_and_sort(&mut all_programs);

    if cache_eligible {
        storage::save_scan_cache(&all_programs)?;
        cache_state.cache_hit = false;
        cache_state.cache_valid = true;
        cache_state.refreshed = true;
        cache_state.schema_version = storage::CACHE_SCHEMA_VERSION;
        cache_state.generated_at = Some(Utc::now().to_rfc3339());
        if query.refresh {
            cache_state.reason = Some("force_refresh".to_string());
        } else if cache_state.reason.is_none() {
            cache_state.reason = Some("cache_rebuilt".to_string());
        }
    } else {
        cache_state.cache_hit = false;
        cache_state.cache_valid = false;
        cache_state.refreshed = false;
        cache_state.reason = Some("source_not_cacheable".to_string());
    }

    apply_search_filter(&mut all_programs, query.search.as_deref());

    Ok(ProgramListResponse {
        programs: all_programs,
        cache: cache_state,
    })
}

fn is_cache_eligible(source: Option<InstallSource>) -> bool {
    matches!(source, None | Some(InstallSource::Registry))
}

fn collect_programs(source: Option<InstallSource>) -> Vec<InstalledProgram> {
    let mut all_programs = Vec::new();

    // None 默认仅使用 Registry，避免 MSI 调用带来的额外开销
    let sources = match source {
        Some(selected) => vec![selected],
        None => vec![InstallSource::Registry],
    };

    for src in &sources {
        match src {
            InstallSource::Registry => match registry::list_registry_programs() {
                Ok(programs) => all_programs.extend(programs),
                Err(error) => tracing::warn!("读取注册表程序失败: {}", error),
            },
            InstallSource::Msi => match msi::list_msi_products() {
                Ok(programs) => all_programs.extend(programs),
                Err(error) => tracing::warn!("读取 MSI 程序失败: {}", error),
            },
            InstallSource::Store => match store::list_store_apps() {
                Ok(programs) => all_programs.extend(programs),
                Err(error) => tracing::warn!("读取商店应用失败: {}", error),
            },
            InstallSource::Unknown => {}
        }
    }

    all_programs
}

fn apply_search_filter(programs: &mut Vec<InstalledProgram>, search: Option<&str>) {
    if let Some(query) = search {
        let normalized_query = query.to_lowercase();
        programs.retain(|program| {
            utils::fuzzy_match(&program.name.to_lowercase(), &normalized_query)
                || program
                    .publisher
                    .as_ref()
                    .map(|publisher| {
                        utils::fuzzy_match(&publisher.to_lowercase(), &normalized_query)
                    })
                    .unwrap_or(false)
        });
    }
}

fn dedupe_and_sort(programs: &mut Vec<InstalledProgram>) {
    let mut seen = std::collections::HashSet::new();
    programs.retain(|program| seen.insert(program.name.to_lowercase()));
    programs.sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase()));
}
