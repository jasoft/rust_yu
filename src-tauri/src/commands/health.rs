use rust_yu_lib::health::{self, HealthReport};
use rust_yu_lib::lister::{self, models::InstallSourceSelector};
use rust_yu_lib::startup::{manager, models::StartupListQuery};

use super::CommandError;

/// 在后台完成本机健康检查；不会下载版本信息，也不会修改系统状态。
#[tauri::command]
pub async fn get_program_health() -> Result<HealthReport, CommandError> {
    tauri::async_runtime::spawn_blocking(|| {
        let response = lister::list_programs_with_cache(lister::models::ListProgramsQuery {
            source: InstallSourceSelector::All,
            search: None,
            refresh: false,
            cache_ttl_seconds: lister::storage::DEFAULT_CACHE_TTL_SECONDS,
        })
        .map_err(CommandError::from)?;

        let (startup_items, mut warnings) = match manager::list_startup_items(StartupListQuery {
            include_raw: false,
            ..StartupListQuery::default()
        }) {
            Ok(response) => (response.items, Vec::new()),
            Err(error) => (
                Vec::new(),
                vec![format!(
                    "自启动项读取不完整，启动影响仅供参考: {}",
                    error.message
                )],
            ),
        };

        if !response.cache.cache_valid {
            warnings
                .push("程序列表来自实时扫描或缓存未命中，健康结果已按当前读取结果计算".to_string());
        }
        let usage = health::load_usage_snapshots(&response.programs);
        Ok(health::analyze_programs(
            &response.programs,
            &startup_items,
            &usage,
            warnings,
        ))
    })
    .await
    .map_err(|error| CommandError::new(format!("软件健康检查任务失败: {error}")))?
}
