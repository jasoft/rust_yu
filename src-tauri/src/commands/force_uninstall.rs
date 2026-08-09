use rust_yu_lib::application::force_uninstall::{
    clean_force_uninstall as run_clean_force_uninstall,
    plan_force_uninstall as run_plan_force_uninstall, ForceCleanupSelection, ForceUninstallPlan,
    ForceUninstallResult,
};
use serde::Deserialize;

use super::{require_administrator, CommandError};

#[derive(Debug, Clone, Deserialize)]
pub struct PlanForceUninstallRequest {
    pub path: String,
    pub name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CleanForceUninstallRequest {
    pub plan: ForceUninstallPlan,
    pub selection: ForceCleanupSelection,
}

#[tauri::command]
pub async fn plan_force_uninstall(
    request: PlanForceUninstallRequest,
) -> Result<ForceUninstallPlan, CommandError> {
    require_administrator()?;
    let path = request.path;
    let name = request.name;
    tokio::task::spawn_blocking(move || run_plan_force_uninstall(&path, name.as_deref()))
        .await
        .map_err(|error| CommandError::new(format!("创建强制卸载计划失败: {error}")))?
        .map_err(CommandError::from)
}

#[tauri::command]
pub async fn clean_force_uninstall(
    request: CleanForceUninstallRequest,
) -> Result<ForceUninstallResult, CommandError> {
    require_administrator()?;
    run_clean_force_uninstall(&request.plan, request.selection)
        .await
        .map_err(CommandError::from)
}
