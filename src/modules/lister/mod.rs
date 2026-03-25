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
    InstallSource, InstallSourceSelector, InstalledProgram, ListProgramsQuery,
    MetadataWarmupItemStatus, MetadataWarmupProgress, MetadataWarmupQuery, MetadataWarmupStage,
    MetadataWarmupStats, MetadataWarmupSummary, ProgramListCacheState, ProgramListResponse,
};

/// 列出所有已安装程序（兼容旧接口）
pub fn list_all_programs(
    source: Option<InstallSource>,
    search: Option<&str>,
) -> Result<Vec<InstalledProgram>, UninstallerError> {
    let mut all_programs = collect_programs(InstallSourceSelector::from_install_source(source));
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

pub fn warmup_program_metadata<F>(
    mut query: MetadataWarmupQuery,
    mut on_progress: F,
) -> Result<MetadataWarmupSummary, UninstallerError>
where
    F: FnMut(MetadataWarmupProgress),
{
    if query.cache_ttl_seconds <= 0 {
        query.cache_ttl_seconds = storage::DEFAULT_CACHE_TTL_SECONDS;
    }

    let selected_kinds = query.selected_kinds();
    if selected_kinds.is_empty() {
        return Err(UninstallerError::Other(
            "至少需要选择一个元数据预热任务".to_string(),
        ));
    }

    let cache_eligible = is_cache_eligible(query.source);
    let base_query = ListProgramsQuery {
        source: query.source,
        search: None,
        refresh: query.refresh,
        cache_ttl_seconds: query.cache_ttl_seconds,
    };
    let mut response = list_programs_with_cache(base_query)?;
    let mut summary = MetadataWarmupSummary {
        total_programs: response.programs.len(),
        matched_programs: 0,
        cache: response.cache.clone(),
        ..MetadataWarmupSummary::default()
    };

    let selected_indices: Vec<usize> = response
        .programs
        .iter()
        .enumerate()
        .filter(|(_, program)| matches_search(program, query.search.as_deref()))
        .map(|(index, _)| index)
        .collect();
    summary.matched_programs = selected_indices.len();

    for kind in selected_kinds {
        let eligible_indices: Vec<usize> = selected_indices
            .iter()
            .copied()
            .filter(|index| {
                enrichment::is_program_metadata_warmup_eligible(&response.programs[*index], kind)
            })
            .collect();
        let target_indices: Vec<usize> = match kind {
            models::MetadataWarmupKind::Icons => eligible_indices.clone(),
            models::MetadataWarmupKind::Sizes => selected_indices.clone(),
        };
        let mut stats = MetadataWarmupStats {
            total: target_indices.len(),
            eligible: eligible_indices.len(),
            ..MetadataWarmupStats::default()
        };

        on_progress(MetadataWarmupProgress {
            kind,
            stage: MetadataWarmupStage::Started,
            current: 0,
            total: target_indices.len(),
            program_id: None,
            program_name: None,
            status: None,
            message: None,
            program: None,
        });

        for (position, index) in target_indices.iter().enumerate() {
            let program = response.programs[*index].clone();
            on_progress(MetadataWarmupProgress {
                kind,
                stage: MetadataWarmupStage::ItemStarted,
                current: position + 1,
                total: target_indices.len(),
                program_id: Some(program.id.clone()),
                program_name: Some(program.name.clone()),
                status: None,
                message: None,
                program: None,
            });

            let (status, message) =
                enrichment::warmup_program_metadata(&mut response.programs[*index], kind);
            stats.processed += 1;
            match status {
                MetadataWarmupItemStatus::Updated => stats.updated += 1,
                MetadataWarmupItemStatus::Skipped => stats.skipped += 1,
                MetadataWarmupItemStatus::Failed => stats.failed += 1,
            }

            on_progress(MetadataWarmupProgress {
                kind,
                stage: MetadataWarmupStage::ItemFinished,
                current: position + 1,
                total: target_indices.len(),
                program_id: Some(response.programs[*index].id.clone()),
                program_name: Some(response.programs[*index].name.clone()),
                status: Some(status),
                message,
                program: Some(response.programs[*index].clone()),
            });
        }

        on_progress(MetadataWarmupProgress {
            kind,
            stage: MetadataWarmupStage::Completed,
            current: target_indices.len(),
            total: target_indices.len(),
            program_id: None,
            program_name: None,
            status: None,
            message: None,
            program: None,
        });

        match kind {
            models::MetadataWarmupKind::Icons => summary.icons = Some(stats),
            models::MetadataWarmupKind::Sizes => summary.sizes = Some(stats),
        }
    }

    if cache_eligible {
        dedupe_and_sort(&mut response.programs);
        storage::save_scan_cache(&response.programs)?;
        summary.cache = ProgramListCacheState {
            cache_hit: false,
            cache_valid: true,
            refreshed: true,
            schema_version: storage::CACHE_SCHEMA_VERSION,
            generated_at: Some(Utc::now().to_rfc3339()),
            reason: Some("metadata_warmup".to_string()),
        };
    } else {
        summary.cache = ProgramListCacheState {
            cache_hit: false,
            cache_valid: false,
            refreshed: false,
            schema_version: storage::CACHE_SCHEMA_VERSION,
            generated_at: None,
            reason: Some("source_not_cacheable".to_string()),
        };
    }

    Ok(summary)
}

fn is_cache_eligible(source: InstallSourceSelector) -> bool {
    source.is_cache_eligible()
}

fn collect_program_sources(source: InstallSourceSelector) -> Vec<InstallSource> {
    source.install_sources().to_vec()
}

fn collect_programs(source: InstallSourceSelector) -> Vec<InstalledProgram> {
    let mut all_programs = Vec::new();
    let sources = collect_program_sources(source);

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
        programs.retain(|program| matches_normalized_search(program, &normalized_query));
    }
}

fn matches_search(program: &InstalledProgram, search: Option<&str>) -> bool {
    match search {
        Some(query) => matches_normalized_search(program, &query.to_lowercase()),
        None => true,
    }
}

fn matches_normalized_search(program: &InstalledProgram, normalized_query: &str) -> bool {
    utils::fuzzy_match(&program.name.to_lowercase(), normalized_query)
        || program
            .publisher
            .as_ref()
            .map(|publisher| utils::fuzzy_match(&publisher.to_lowercase(), normalized_query))
            .unwrap_or(false)
}

fn dedupe_and_sort(programs: &mut Vec<InstalledProgram>) {
    let mut seen = std::collections::HashSet::new();
    programs.retain(|program| seen.insert(program.name.to_lowercase()));
    programs.sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase()));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::lister::models::InstallSourceSelector;

    fn sample_program(name: &str, publisher: Option<&str>) -> InstalledProgram {
        let mut program = InstalledProgram::new(name.to_string(), InstallSource::Registry);
        program.publisher = publisher.map(str::to_string);
        program
    }

    #[test]
    fn apply_search_filter_matches_name_and_publisher_case_insensitively() {
        let mut programs = vec![
            sample_program("Demo Player", Some("Acme Tools")),
            sample_program("Other App", Some("Vendor Labs")),
        ];

        apply_search_filter(&mut programs, Some("vEnDoR"));

        assert_eq!(programs.len(), 1);
        assert_eq!(programs[0].name, "Other App");
    }

    #[test]
    fn dedupe_and_sort_collapses_case_insensitive_duplicates() {
        let mut programs = vec![
            sample_program("beta", None),
            sample_program("Alpha", None),
            sample_program("alpha", Some("Duplicate")),
        ];

        dedupe_and_sort(&mut programs);

        let names: Vec<_> = programs.into_iter().map(|program| program.name).collect();
        assert_eq!(names, vec!["Alpha".to_string(), "beta".to_string()]);
    }

    #[test]
    fn list_programs_with_cache_marks_all_source_as_non_cacheable() {
        let response = list_programs_with_cache(ListProgramsQuery {
            source: InstallSourceSelector::All,
            search: Some("__rust_yu_unmatched_query__".to_string()),
            refresh: false,
            cache_ttl_seconds: 60,
        })
        .unwrap_or_else(|error| panic!("unexpected error: {error}"));

        assert!(response.programs.is_empty());
        assert!(!response.cache.cache_hit);
        assert!(!response.cache.cache_valid);
        assert!(!response.cache.refreshed);
        assert_eq!(
            response.cache.reason.as_deref(),
            Some("source_not_cacheable")
        );
    }

    #[test]
    fn warmup_program_metadata_requires_selected_kind() {
        let result = warmup_program_metadata(MetadataWarmupQuery::default(), |_| {});

        assert!(
            matches!(result, Err(UninstallerError::Other(message)) if message.contains("至少需要选择一个元数据预热任务"))
        );
    }

    #[test]
    fn warmup_program_metadata_for_all_source_reports_progress_without_cache() {
        let mut progress_events = Vec::new();
        let summary = warmup_program_metadata(
            MetadataWarmupQuery {
                source: InstallSourceSelector::All,
                search: Some("__rust_yu_unmatched_query__".to_string()),
                refresh: false,
                cache_ttl_seconds: 60,
                icons: true,
                sizes: false,
            },
            |progress| progress_events.push(progress),
        )
        .unwrap_or_else(|error| panic!("unexpected error: {error}"));

        assert!(summary.total_programs >= summary.matched_programs);
        assert_eq!(summary.matched_programs, 0);
        assert_eq!(
            summary.cache.reason.as_deref(),
            Some("source_not_cacheable")
        );
        assert!(summary.icons.is_some());
        assert_eq!(progress_events.len(), 2);
        assert_eq!(progress_events[0].stage, MetadataWarmupStage::Started);
        assert_eq!(progress_events[1].stage, MetadataWarmupStage::Completed);
    }

    #[test]
    fn install_source_selector_all_expands_to_all_supported_sources() {
        assert_eq!(
            collect_program_sources(InstallSourceSelector::All),
            vec![
                InstallSource::Registry,
                InstallSource::Msi,
                InstallSource::Store
            ]
        );
        assert_eq!(
            collect_program_sources(InstallSourceSelector::Standard),
            vec![InstallSource::Registry]
        );
    }
}
