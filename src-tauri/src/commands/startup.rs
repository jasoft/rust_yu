use rust_yu_lib::startup::manager;
use rust_yu_lib::startup::models::{
    StartupAction, StartupActionPlan, StartupActionResult, StartupAddPlan, StartupAddResult,
    StartupEnvelope, StartupItem, StartupListQuery, StartupListResponse, StartupScope,
    StartupSource, StartupSourceDescriptor, StartupState,
};
use serde::{Deserialize, Serialize};

use super::CommandError;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartupListOptions {
    pub source: Option<String>,
    pub scope: Option<String>,
    pub state: Option<String>,
    pub search: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    pub sort_by: Option<String>,
    pub descending: Option<bool>,
    pub include_raw: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartupActionOptions {
    pub id: String,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartupRollbackOptions {
    pub change_id: String,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartupAddOptions {
    pub name: String,
    pub command: String,
    pub reason: Option<String>,
}

#[tauri::command]
pub async fn list_startup_items(
    options: Option<StartupListOptions>,
) -> Result<StartupEnvelope<StartupListResponse>, CommandError> {
    let query = StartupListQuery {
        source: parse_source(options.as_ref().and_then(|value| value.source.as_deref()))?,
        scope: parse_scope(options.as_ref().and_then(|value| value.scope.as_deref()))?,
        state: parse_state(options.as_ref().and_then(|value| value.state.as_deref()))?,
        search: options.as_ref().and_then(|value| value.search.clone()),
        limit: options.as_ref().and_then(|value| value.limit),
        offset: options
            .as_ref()
            .and_then(|value| value.offset)
            .unwrap_or_default(),
        sort_by: options.as_ref().and_then(|value| value.sort_by.clone()),
        descending: options
            .as_ref()
            .and_then(|value| value.descending)
            .unwrap_or(false),
        include_raw: options
            .as_ref()
            .and_then(|value| value.include_raw)
            .unwrap_or(false),
    };

    let result = tauri::async_runtime::spawn_blocking(move || manager::list_startup_items(query))
        .await
        .map_err(|error| CommandError::new(format!("startup list 任务执行失败: {error}")))?;

    Ok(match result {
        Ok(response) => StartupEnvelope::success(response),
        Err(error) => StartupEnvelope::from_startup_error(&error),
    })
}

#[tauri::command]
pub async fn get_startup_item(
    id: String,
    include_raw: Option<bool>,
) -> Result<StartupEnvelope<StartupItem>, CommandError> {
    let include_raw = include_raw.unwrap_or(false);
    let result =
        tauri::async_runtime::spawn_blocking(move || manager::get_startup_item(&id, include_raw))
            .await
            .map_err(|error| CommandError::new(format!("startup show 任务执行失败: {error}")))?;

    Ok(match result {
        Ok(response) => StartupEnvelope::success(response),
        Err(error) => StartupEnvelope::from_startup_error(&error),
    })
}

#[tauri::command]
pub async fn list_startup_sources(
) -> Result<StartupEnvelope<Vec<StartupSourceDescriptor>>, CommandError> {
    Ok(StartupEnvelope::success(manager::list_sources()))
}

#[tauri::command]
pub async fn plan_startup_action(
    action: String,
    options: StartupActionOptions,
) -> Result<StartupEnvelope<StartupActionPlan>, CommandError> {
    let action = parse_action(&action)?;
    let result = tauri::async_runtime::spawn_blocking(move || {
        // 预演只生成操作清单，不修改系统，也不应要求当前进程已经提权。
        manager::plan_action(&options.id, action, options.reason, false)
    })
    .await
    .map_err(|error| CommandError::new(format!("startup plan 任务执行失败: {error}")))?;

    Ok(match result {
        Ok(response) => StartupEnvelope::success(response),
        Err(error) => StartupEnvelope::from_startup_error(&error),
    })
}

#[tauri::command]
pub async fn apply_startup_action(
    action: String,
    options: StartupActionOptions,
) -> Result<StartupEnvelope<StartupActionResult>, CommandError> {
    let action = parse_action(&action)?;
    let result = tauri::async_runtime::spawn_blocking(move || {
        manager::apply_action(&options.id, action, options.reason)
    })
    .await
    .map_err(|error| CommandError::new(format!("startup apply 任务执行失败: {error}")))?;

    Ok(match result {
        Ok(response) => StartupEnvelope::success(response),
        Err(error) => StartupEnvelope::from_startup_error(&error),
    })
}

#[tauri::command]
pub async fn rollback_startup_action(
    options: StartupRollbackOptions,
) -> Result<StartupEnvelope<StartupActionResult>, CommandError> {
    let result = tauri::async_runtime::spawn_blocking(move || {
        manager::rollback_action(&options.change_id, options.reason)
    })
    .await
    .map_err(|error| CommandError::new(format!("startup rollback 任务执行失败: {error}")))?;

    Ok(match result {
        Ok(response) => StartupEnvelope::success(response),
        Err(error) => StartupEnvelope::from_startup_error(&error),
    })
}

#[tauri::command]
pub async fn plan_add_startup_item(
    options: StartupAddOptions,
) -> Result<StartupEnvelope<StartupAddPlan>, CommandError> {
    let result = tauri::async_runtime::spawn_blocking(move || {
        manager::plan_add_registry_run_item(&options.name, &options.command, options.reason, false)
    })
    .await
    .map_err(|error| CommandError::new(format!("startup add plan 任务执行失败: {error}")))?;

    Ok(match result {
        Ok(response) => StartupEnvelope::success(response),
        Err(error) => StartupEnvelope::from_startup_error(&error),
    })
}

#[tauri::command]
pub async fn add_startup_item(
    options: StartupAddOptions,
) -> Result<StartupEnvelope<StartupAddResult>, CommandError> {
    let result = tauri::async_runtime::spawn_blocking(move || {
        manager::add_registry_run_item(&options.name, &options.command, options.reason)
    })
    .await
    .map_err(|error| CommandError::new(format!("startup add 任务执行失败: {error}")))?;

    Ok(match result {
        Ok(response) => StartupEnvelope::success(response),
        Err(error) => StartupEnvelope::from_startup_error(&error),
    })
}

fn parse_source(value: Option<&str>) -> Result<Option<StartupSource>, CommandError> {
    match value.map(|text| text.to_lowercase()) {
        None => Ok(None),
        Some(text) if text == "all" => Ok(None),
        Some(text) if text == "registry_run" || text == "run" => {
            Ok(Some(StartupSource::RegistryRun))
        }
        Some(text) if text == "registry_run_once" || text == "run_once" => {
            Ok(Some(StartupSource::RegistryRunOnce))
        }
        Some(text) if text == "registry_policy_run" || text == "policy_run" => {
            Ok(Some(StartupSource::RegistryPolicyRun))
        }
        Some(text) if text == "startup_folder" || text == "folder" => {
            Ok(Some(StartupSource::StartupFolder))
        }
        Some(text) if text == "scheduled_task" || text == "task" => {
            Ok(Some(StartupSource::ScheduledTask))
        }
        Some(text) if text == "service" || text == "services" => Ok(Some(StartupSource::Service)),
        Some(text) => Err(CommandError::with_code(
            "invalid_selector",
            format!("未知来源: {text}"),
        )),
    }
}

fn parse_scope(value: Option<&str>) -> Result<Option<StartupScope>, CommandError> {
    match value.map(|text| text.to_lowercase()) {
        None => Ok(None),
        Some(text) if text == "all" => Ok(None),
        Some(text) if text == "user" => Ok(Some(StartupScope::User)),
        Some(text) if text == "machine" => Ok(Some(StartupScope::Machine)),
        Some(text) => Err(CommandError::with_code(
            "invalid_selector",
            format!("未知作用域: {text}"),
        )),
    }
}

fn parse_state(value: Option<&str>) -> Result<Option<StartupState>, CommandError> {
    match value.map(|text| text.to_lowercase()) {
        None => Ok(None),
        Some(text) if text == "all" => Ok(None),
        Some(text) if text == "enabled" => Ok(Some(StartupState::Enabled)),
        Some(text) if text == "disabled" => Ok(Some(StartupState::Disabled)),
        Some(text) if text == "broken" => Ok(Some(StartupState::Broken)),
        Some(text) => Err(CommandError::with_code(
            "invalid_selector",
            format!("未知状态: {text}"),
        )),
    }
}

fn parse_action(value: &str) -> Result<StartupAction, CommandError> {
    match value.to_lowercase().as_str() {
        "enable" => Ok(StartupAction::Enable),
        "disable" => Ok(StartupAction::Disable),
        "delete" => Ok(StartupAction::Delete),
        "rollback" => Ok(StartupAction::Rollback),
        other => Err(CommandError::with_code(
            "invalid_selector",
            format!("未知动作: {other}"),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_action, parse_scope, parse_source, parse_state};
    use rust_yu_lib::startup::models::{StartupAction, StartupScope, StartupSource, StartupState};

    #[test]
    fn parse_source_accepts_aliases() {
        assert_eq!(
            parse_source(Some("run")).ok(),
            Some(Some(StartupSource::RegistryRun))
        );
        assert_eq!(
            parse_source(Some("services")).ok(),
            Some(Some(StartupSource::Service))
        );
        assert_eq!(parse_source(Some("all")).ok(), Some(None));
    }

    #[test]
    fn parse_source_rejects_unknown_values() {
        let error = parse_source(Some("mystery")).expect_err("expected invalid selector");

        assert_eq!(error.code.as_deref(), Some("invalid_selector"));
    }

    #[test]
    fn parse_scope_and_state_accept_supported_values() {
        assert_eq!(
            parse_scope(Some("machine")).ok(),
            Some(Some(StartupScope::Machine))
        );
        assert_eq!(parse_scope(Some("all")).ok(), Some(None));
        assert_eq!(
            parse_state(Some("broken")).ok(),
            Some(Some(StartupState::Broken))
        );
        assert_eq!(parse_state(Some("all")).ok(), Some(None));
    }

    #[test]
    fn parse_action_rejects_unknown_values() {
        assert_eq!(parse_action("enable").ok(), Some(StartupAction::Enable));

        let error = parse_action("noop").expect_err("expected invalid selector");
        assert_eq!(error.code.as_deref(), Some("invalid_selector"));
    }
}
