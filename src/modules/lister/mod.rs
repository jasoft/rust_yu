pub mod analyzer;
pub mod enrichment;
pub mod models;
pub mod msi;
pub mod registry;
pub mod storage;
pub mod store;

use chrono::Utc;
use std::collections::HashMap;

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
        let cached = storage::read_scan_cache(query.source, query.cache_ttl_seconds)?;
        cache_state.schema_version = cached.schema_version;
        cache_state.generated_at = cached.generated_at.clone();
        cache_state.reason = cached.reason.clone();

        if cached.cache_hit {
            let mut cached_programs = cached.entries.unwrap_or_default();
            apply_search_filter(&mut cached_programs, query.search.as_deref());
            cache_state.cache_hit = true;
            cache_state.cache_valid = cached.cache_valid;
            cache_state.refreshed = false;
            return Ok(ProgramListResponse {
                programs: cached_programs,
                cache: cache_state,
            });
        }
    }

    // 即使列表 TTL 已到期或用户主动刷新，也保留仍然存在的图标资产。
    // 扫描列表与图标提取的生命周期不同，不能因刷新注册表清单而丢弃磁盘缓存索引。
    let cached_programs = if cache_eligible {
        storage::read_scan_cache(query.source, i64::MAX)?
            .entries
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    let mut all_programs = collect_programs(query.source);
    enrichment::enrich_programs(&mut all_programs);
    restore_cached_icon_metadata(&mut all_programs, &cached_programs);
    dedupe_and_sort(&mut all_programs);

    if cache_eligible {
        storage::save_scan_cache(query.source, &all_programs)?;
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
    tracing::info!(
        source = query.source.as_str(),
        refresh = query.refresh,
        icons = query.icons,
        sizes = query.sizes,
        search = ?query.search,
        "开始预热已安装程序元数据"
    );
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
        storage::save_scan_cache(query.source, &response.programs)?;
        tracing::info!(
            program_count = response.programs.len(),
            "已保存元数据预热结果"
        );
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

    // Registry 与 MSI 是同一套 ARP 数据的两个分类视图。旧实现分别扫描后再按
    // 显示名称去重，会丢失同名应用并让 MSI 子组件重新进入列表；这里一次扫描，
    // 再按稳定来源过滤，确保 All 不产生重复条目。
    let registry_programs = if sources
        .iter()
        .any(|source| matches!(source, InstallSource::Registry | InstallSource::Msi))
    {
        match registry::list_registry_programs() {
            Ok(programs) => Some(programs),
            Err(error) => {
                tracing::warn!("读取注册表程序失败: {}", error);
                None
            }
        }
    } else {
        None
    };

    for src in &sources {
        match src {
            InstallSource::Registry => {
                if let Some(programs) = registry_programs.as_ref() {
                    all_programs.extend(
                        programs
                            .iter()
                            .filter(|program| program.install_source == InstallSource::Registry)
                            .cloned(),
                    );
                }
            }
            InstallSource::Msi => {
                if let Some(programs) = registry_programs.as_ref() {
                    all_programs.extend(
                        programs
                            .iter()
                            .filter(|program| program.install_source == InstallSource::Msi)
                            .cloned(),
                    );
                }
            }
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

fn restore_cached_icon_metadata(
    programs: &mut [InstalledProgram],
    cached_programs: &[InstalledProgram],
) {
    let cached_by_key: HashMap<_, _> = cached_programs
        .iter()
        .map(|program| (storage::build_program_cache_key(program), program))
        .collect();

    for program in programs {
        let key = storage::build_program_cache_key(program);
        let Some(cached) = cached_by_key.get(&key) else {
            continue;
        };
        program.icon_cache_path_32 = cached.icon_cache_path_32.clone();
        program.icon_cache_path_48 = cached.icon_cache_path_48.clone();
        program.icon_data_url = cached.icon_data_url.clone();
        program.icon_data_url_32 = cached.icon_data_url_32.clone();
        program.icon_data_url_48 = cached.icon_data_url_48.clone();
    }
}

fn dedupe_and_sort(programs: &mut Vec<InstalledProgram>) {
    let mut seen = std::collections::HashSet::new();
    let mut seen_display_aliases = std::collections::HashSet::new();
    // 只有稳定身份相同时才合并；同名不同厂商/来源的应用必须同时保留。
    programs.retain(|program| {
        let display_alias = format!(
            "{}|{}",
            program.name.trim().to_ascii_lowercase(),
            program
                .publisher
                .as_deref()
                .unwrap_or_default()
                .trim()
                .to_ascii_lowercase()
        );
        // 同一产品可能同时注册 ARP 和 Appx（典型例子是 Microsoft Edge）。
        // Windows 设置合并为一个条目；优先保留 ARP/ MSI 的可卸载身份，跳过后来的
        // Store 别名。不同发布者的同名应用仍由稳定身份逻辑分别保留。
        if program.install_source == InstallSource::Store
            && seen_display_aliases.contains(&display_alias)
        {
            return false;
        }
        seen_display_aliases.insert(display_alias);
        let identity = if program.id.starts_with("registry:")
            || program.install_source == InstallSource::Store
        {
            program.id.to_ascii_lowercase()
        } else {
            storage::build_program_cache_key(program)
        };
        seen.insert(identity)
    });
    programs.sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase()));
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use super::*;
    use crate::modules::lister::{models::InstallSourceSelector, storage};

    fn sample_program(name: &str, publisher: Option<&str>) -> InstalledProgram {
        let mut program = InstalledProgram::new(name.to_string(), InstallSource::Registry);
        program.publisher = publisher.map(str::to_string);
        program
    }

    fn with_storage_root(test_name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "rust-yu-lister-test-{}-{}",
            test_name,
            uuid::Uuid::new_v4()
        ));
        let _ = fs::create_dir_all(&root);
        std::env::set_var("RUST_YU_STORAGE_DIR", &root);
        root
    }

    fn cleanup_storage_root(root: &PathBuf) {
        std::env::remove_var("RUST_YU_STORAGE_DIR");
        let _ = fs::remove_dir_all(root);
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
    fn apply_search_filter_requires_normalized_substring_match() {
        let mut programs = vec![
            sample_program("OpenAI.ChatGPT-Desktop", Some("CN=OpenAI")),
            sample_program(
                "Shapr3D.Shapr3D",
                Some("CN=Shapr3D Zártkörűen Működő Részvénytársaság"),
            ),
        ];

        apply_search_filter(&mut programs, Some("chat gpt"));

        assert_eq!(programs.len(), 1);
        assert_eq!(programs[0].name, "OpenAI.ChatGPT-Desktop");
    }

    #[test]
    fn dedupe_and_sort_preserves_same_name_from_different_publishers() {
        let mut programs = vec![
            sample_program("beta", None),
            sample_program("Alpha", None),
            sample_program("alpha", Some("Duplicate")),
        ];

        dedupe_and_sort(&mut programs);

        let names: Vec<_> = programs.into_iter().map(|program| program.name).collect();
        assert_eq!(
            names,
            vec!["Alpha".to_string(), "alpha".to_string(), "beta".to_string()]
        );
    }

    #[test]
    fn restore_cached_icon_metadata_matches_stable_program_identity() {
        let mut cached = sample_program("Demo", Some("Acme"));
        cached.uninstall_string = Some(r#""C:\Program Files\Demo\uninstall.exe""#.to_string());
        cached.install_location = Some(r"C:\Program Files\Demo".to_string());
        cached.icon_cache_path_32 = Some(r"C:\cache\demo-32.png".to_string());
        cached.icon_cache_path_48 = Some(r"C:\cache\demo-48.png".to_string());

        let mut refreshed = cached.clone();
        refreshed.id = "new-scan-id".to_string();
        refreshed.icon_cache_path_32 = None;
        refreshed.icon_cache_path_48 = None;

        restore_cached_icon_metadata(std::slice::from_mut(&mut refreshed), &[cached]);

        assert_eq!(
            refreshed.icon_cache_path_32.as_deref(),
            Some(r"C:\cache\demo-32.png")
        );
        assert_eq!(
            refreshed.icon_cache_path_48.as_deref(),
            Some(r"C:\cache\demo-48.png")
        );
    }

    #[test]
    fn list_programs_with_cache_rebuilds_cache_for_all_source() {
        let _guard = storage::TEST_STORAGE_ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let root = with_storage_root("all-source-cache");

        let response = list_programs_with_cache(ListProgramsQuery {
            source: InstallSourceSelector::All,
            search: Some("__rust_yu_unmatched_query__".to_string()),
            refresh: false,
            cache_ttl_seconds: 60,
        })
        .unwrap_or_else(|error| panic!("unexpected error: {error}"));

        assert!(response.programs.is_empty());
        assert!(!response.cache.cache_hit);
        assert!(response.cache.cache_valid);
        assert!(response.cache.refreshed);
        assert_eq!(response.cache.reason.as_deref(), Some("cache_missing"));

        let cached_response = list_programs_with_cache(ListProgramsQuery {
            source: InstallSourceSelector::All,
            search: Some("__rust_yu_unmatched_query__".to_string()),
            refresh: false,
            cache_ttl_seconds: 60,
        })
        .unwrap_or_else(|error| panic!("unexpected error: {error}"));

        assert!(cached_response.cache.cache_hit);
        assert!(cached_response.cache.cache_valid);
        assert!(!cached_response.cache.refreshed);
        assert!(cached_response.programs.is_empty());

        cleanup_storage_root(&root);
    }

    #[test]
    fn warmup_program_metadata_requires_selected_kind() {
        let result = warmup_program_metadata(MetadataWarmupQuery::default(), |_| {});

        assert!(
            matches!(result, Err(UninstallerError::Other(message)) if message.contains("至少需要选择一个元数据预热任务"))
        );
    }

    #[test]
    fn warmup_program_metadata_for_all_source_updates_cache_and_reports_progress() {
        let _guard = storage::TEST_STORAGE_ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
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
        assert_eq!(summary.cache.reason.as_deref(), Some("metadata_warmup"));
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
