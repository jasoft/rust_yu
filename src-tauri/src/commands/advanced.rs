use super::error::CommandError;
use rust_yu_lib::modules::advanced::{
    self, CleanupPolicyProfile, HibernationPlan, HibernationResult, InventoryBaseline,
    InventoryComparison, ReconstructedEvidencePacket,
};
use rust_yu_lib::modules::lister::models::{
    InstallSourceSelector, InstalledProgram, ListProgramsQuery,
};

#[tauri::command]
pub async fn reconstruct_installation(
    program: InstalledProgram,
) -> Result<ReconstructedEvidencePacket, CommandError> {
    advanced::reconstruct_installation(program)
        .await
        .map_err(CommandError::from)
}

#[tauri::command]
pub async fn list_cleanup_policy_profiles() -> Result<Vec<CleanupPolicyProfile>, CommandError> {
    Ok(advanced::cleanup_profiles())
}

#[tauri::command]
pub async fn plan_program_hibernation(
    program: InstalledProgram,
) -> Result<HibernationPlan, CommandError> {
    tauri::async_runtime::spawn_blocking(move || advanced::plan_hibernation(program))
        .await
        .map_err(|error| CommandError::new(format!("休眠影响分析任务失败：{error}")))?
        .map_err(CommandError::from)
}

#[tauri::command]
pub async fn apply_program_hibernation(
    plan: HibernationPlan,
    item_ids: Vec<String>,
    confirm: bool,
) -> Result<HibernationResult, CommandError> {
    tauri::async_runtime::spawn_blocking(move || {
        advanced::apply_hibernation(&plan, &item_ids, confirm)
    })
    .await
    .map_err(|error| CommandError::new(format!("休眠执行任务失败：{error}")))?
    .map_err(CommandError::from)
}

#[tauri::command]
pub async fn wake_program_hibernation(
    change_ids: Vec<String>,
    confirm: bool,
) -> Result<HibernationResult, CommandError> {
    tauri::async_runtime::spawn_blocking(move || advanced::wake_hibernation(&change_ids, confirm))
        .await
        .map_err(|error| CommandError::new(format!("软件唤醒任务失败：{error}")))?
        .map_err(CommandError::from)
}

#[tauri::command]
pub async fn create_inventory_baseline(
    machine_label: String,
) -> Result<InventoryBaseline, CommandError> {
    tauri::async_runtime::spawn_blocking(move || {
        let baseline = advanced::create_inventory_baseline(machine_label)?;
        advanced::save_inventory_baseline(&baseline)?;
        Ok::<_, rust_yu_lib::modules::common::error::UninstallerError>(baseline)
    })
    .await
    .map_err(|error| CommandError::new(format!("软件基线任务失败：{error}")))?
    .map_err(CommandError::from)
}

#[tauri::command]
pub async fn compare_inventory_baseline(
    baseline: InventoryBaseline,
) -> Result<InventoryComparison, CommandError> {
    tauri::async_runtime::spawn_blocking(move || {
        let response = rust_yu_lib::modules::lister::list_programs_with_cache(ListProgramsQuery {
            source: InstallSourceSelector::All,
            search: None,
            refresh: true,
            cache_ttl_seconds: 0,
        })?;
        advanced::compare_inventory(&baseline, &response.programs)
    })
    .await
    .map_err(|error| CommandError::new(format!("软件基线比较任务失败：{error}")))?
    .map_err(CommandError::from)
}
