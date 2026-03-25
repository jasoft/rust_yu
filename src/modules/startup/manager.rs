use chrono::Utc;

use crate::modules::common::utils::ensure_running_as_administrator;

use super::models::{
    StartupAction, StartupActionPlan, StartupActionResult, StartupCapabilities, StartupChangeLog,
    StartupError, StartupErrorCode, StartupItem, StartupListQuery, StartupListResponse,
    StartupSnapshot, StartupSource, StartupSourceDescriptor, StartupState,
};
use super::{registry_run, rollback, scheduled_tasks, services, startup_folder};

pub fn list_startup_items(query: StartupListQuery) -> Result<StartupListResponse, StartupError> {
    let mut items = collect_all_items(query.include_raw)?;
    items = filter_items(items, &query);
    sort_items(&mut items, query.sort_by.as_deref(), query.descending);

    let total = items.len();
    let offset = query.offset.min(total);
    let limited = if let Some(limit) = query.limit {
        items
            .into_iter()
            .skip(offset)
            .take(limit)
            .collect::<Vec<_>>()
    } else {
        items.into_iter().skip(offset).collect::<Vec<_>>()
    };

    Ok(StartupListResponse {
        items: limited,
        total,
        applied_limit: query.limit,
        applied_offset: offset,
    })
}

pub fn get_startup_item(id: &str, include_raw: bool) -> Result<StartupItem, StartupError> {
    collect_all_items(include_raw)?
        .into_iter()
        .find(|item| item.id == id)
        .ok_or_else(|| {
            StartupError::new(StartupErrorCode::NotFound, format!("未找到自启动项: {id}"))
        })
}

pub fn list_sources() -> Vec<StartupSourceDescriptor> {
    vec![
        StartupSourceDescriptor {
            source: StartupSource::RegistryRun,
            label: "注册表 Run".to_string(),
            supports_user_scope: true,
            supports_machine_scope: true,
            capabilities: StartupCapabilities::for_source(StartupSource::RegistryRun),
            notes: "使用 StartupApproved 维护启停状态".to_string(),
        },
        StartupSourceDescriptor {
            source: StartupSource::RegistryRunOnce,
            label: "注册表 RunOnce".to_string(),
            supports_user_scope: true,
            supports_machine_scope: true,
            capabilities: StartupCapabilities::for_source(StartupSource::RegistryRunOnce),
            notes: "仅支持列出与删除".to_string(),
        },
        StartupSourceDescriptor {
            source: StartupSource::RegistryPolicyRun,
            label: "策略 Run".to_string(),
            supports_user_scope: true,
            supports_machine_scope: true,
            capabilities: StartupCapabilities::for_source(StartupSource::RegistryPolicyRun),
            notes: "禁用通过应用内回收站存储".to_string(),
        },
        StartupSourceDescriptor {
            source: StartupSource::StartupFolder,
            label: "启动文件夹".to_string(),
            supports_user_scope: true,
            supports_machine_scope: true,
            capabilities: StartupCapabilities::for_source(StartupSource::StartupFolder),
            notes: "使用 StartupApproved 维护启停状态".to_string(),
        },
        StartupSourceDescriptor {
            source: StartupSource::ScheduledTask,
            label: "计划任务".to_string(),
            supports_user_scope: true,
            supports_machine_scope: true,
            capabilities: StartupCapabilities::for_source(StartupSource::ScheduledTask),
            notes: "筛选登录/开机触发任务".to_string(),
        },
        StartupSourceDescriptor {
            source: StartupSource::Service,
            label: "服务".to_string(),
            supports_user_scope: false,
            supports_machine_scope: true,
            capabilities: StartupCapabilities::for_source(StartupSource::Service),
            notes: "仅支持启用/禁用启动类型".to_string(),
        },
    ]
}

pub fn plan_action(
    id: &str,
    action: StartupAction,
    reason: Option<String>,
    apply_requested: bool,
) -> Result<StartupActionPlan, StartupError> {
    let item = get_startup_item(id, true)?;
    validate_action(&item, action)?;
    if apply_requested && item.requires_admin {
        ensure_running_as_administrator().map_err(|error| {
            StartupError::new(StartupErrorCode::RequiresAdmin, format!("{error}"))
        })?;
    }

    let mut warnings = item.warnings.clone();
    if reason.is_none() {
        warnings.push("未提供 reason，建议在自动化场景填写变更原因".to_string());
    }
    let operations = planned_operations(&item, action);

    Ok(StartupActionPlan {
        item_id: item.id,
        action,
        apply_requested,
        will_apply: apply_requested,
        requires_admin: item.requires_admin,
        change_id: None,
        warnings,
        operations,
        snapshot_available: true,
    })
}

pub fn apply_action(
    id: &str,
    action: StartupAction,
    reason: Option<String>,
) -> Result<StartupActionResult, StartupError> {
    let item = get_startup_item(id, true)?;
    validate_action(&item, action)?;
    if item.requires_admin {
        ensure_running_as_administrator().map_err(|error| {
            StartupError::new(StartupErrorCode::RequiresAdmin, format!("{error}"))
        })?;
    }

    let snapshot = capture_snapshot(&item)?;
    let change_id = uuid::Uuid::new_v4().to_string();
    let change_log = StartupChangeLog {
        change_id: change_id.clone(),
        item_id: item.id.clone(),
        action,
        source: item.source,
        scope: item.scope,
        created_at: Utc::now().to_rfc3339(),
        reason,
        snapshot_json: serde_json::to_string(&snapshot).map_err(|error| {
            StartupError::new(
                StartupErrorCode::IoError,
                format!("序列化变更快照失败: {error}"),
            )
        })?,
        restored_at: None,
    };
    rollback::save_change_log(&change_log)?;

    let operations = match item.source {
        StartupSource::RegistryRun
        | StartupSource::RegistryRunOnce
        | StartupSource::RegistryPolicyRun => registry_run::apply_action(&item, action, &snapshot)?,
        StartupSource::StartupFolder => startup_folder::apply_action(&item, action, &snapshot)?,
        StartupSource::ScheduledTask => scheduled_tasks::apply_action(&item, action, &snapshot)?,
        StartupSource::Service => services::apply_action(&item, action, &snapshot)?,
    };

    Ok(StartupActionResult {
        item_id: Some(item.id),
        action,
        applied: true,
        change_id: Some(change_id),
        warnings: item.warnings,
        operations,
    })
}

pub fn rollback_action(
    change_id: &str,
    _reason: Option<String>,
) -> Result<StartupActionResult, StartupError> {
    let change_log = rollback::get_change_log(change_id)?.ok_or_else(|| {
        StartupError::new(
            StartupErrorCode::NotFound,
            format!("未找到变更记录: {change_id}"),
        )
    })?;

    if change_log.restored_at.is_some() {
        return Err(StartupError::new(
            StartupErrorCode::Conflict,
            "该变更已回滚过",
        ));
    }

    let snapshot: StartupSnapshot =
        serde_json::from_str(&change_log.snapshot_json).map_err(|error| {
            StartupError::new(
                StartupErrorCode::IoError,
                format!("解析变更快照失败: {error}"),
            )
        })?;

    if snapshot.item.requires_admin {
        ensure_running_as_administrator().map_err(|error| {
            StartupError::new(StartupErrorCode::RequiresAdmin, format!("{error}"))
        })?;
    }

    let operations = match snapshot.item.source {
        StartupSource::RegistryRun
        | StartupSource::RegistryRunOnce
        | StartupSource::RegistryPolicyRun => registry_run::restore_snapshot(&snapshot)?,
        StartupSource::StartupFolder => startup_folder::restore_snapshot(&snapshot)?,
        StartupSource::ScheduledTask => scheduled_tasks::restore_snapshot(&snapshot)?,
        StartupSource::Service => services::restore_snapshot(&snapshot)?,
    };
    rollback::mark_change_log_restored(change_id)?;

    if snapshot.item.source == StartupSource::RegistryPolicyRun
        && snapshot.item.state == StartupState::Disabled
    {
        rollback::remove_disabled_entry(&snapshot.item.id).ok();
    }

    Ok(StartupActionResult {
        item_id: Some(snapshot.item.id),
        action: StartupAction::Rollback,
        applied: true,
        change_id: Some(change_id.to_string()),
        warnings: Vec::new(),
        operations,
    })
}

fn collect_all_items(include_raw: bool) -> Result<Vec<StartupItem>, StartupError> {
    let mut items = Vec::new();
    items.extend(registry_run::collect_items(include_raw)?);
    items.extend(startup_folder::collect_items(include_raw)?);
    items.extend(scheduled_tasks::collect_items(include_raw)?);
    items.extend(services::collect_items(include_raw)?);
    Ok(items)
}

fn filter_items(items: Vec<StartupItem>, query: &StartupListQuery) -> Vec<StartupItem> {
    items
        .into_iter()
        .filter(|item| {
            query
                .source
                .map(|value| value == item.source)
                .unwrap_or(true)
        })
        .filter(|item| query.scope.map(|value| value == item.scope).unwrap_or(true))
        .filter(|item| query.state.map(|value| value == item.state).unwrap_or(true))
        .filter(|item| {
            query
                .search
                .as_deref()
                .map(|search| matches_search(item, search))
                .unwrap_or(true)
        })
        .collect()
}

fn matches_search(item: &StartupItem, search: &str) -> bool {
    let pattern = search.to_lowercase();
    item.name.to_lowercase().contains(&pattern)
        || item
            .command
            .as_deref()
            .unwrap_or_default()
            .to_lowercase()
            .contains(&pattern)
        || item.locator.location.to_lowercase().contains(&pattern)
}

fn sort_items(items: &mut [StartupItem], sort_by: Option<&str>, descending: bool) {
    match sort_by.unwrap_or("name") {
        "source" => items.sort_by(|left, right| left.source.as_str().cmp(right.source.as_str())),
        "scope" => items.sort_by(|left, right| left.scope.as_str().cmp(right.scope.as_str())),
        "state" => {
            items.sort_by(|left, right| state_rank(left.state).cmp(&state_rank(right.state)))
        }
        _ => items.sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase())),
    }

    if descending {
        items.reverse();
    }
}

fn state_rank(state: StartupState) -> u8 {
    match state {
        StartupState::Enabled => 1,
        StartupState::Disabled => 2,
        StartupState::Broken => 3,
    }
}

fn validate_action(item: &StartupItem, action: StartupAction) -> Result<(), StartupError> {
    match action {
        StartupAction::Enable if !item.capabilities.can_enable => Err(StartupError::new(
            StartupErrorCode::Unsupported,
            "当前来源不支持启用",
        )),
        StartupAction::Disable if !item.capabilities.can_disable => Err(StartupError::new(
            StartupErrorCode::Unsupported,
            "当前来源不支持禁用",
        )),
        StartupAction::Delete if !item.capabilities.can_delete => Err(StartupError::new(
            StartupErrorCode::Unsupported,
            "当前来源不支持删除",
        )),
        _ => Ok(()),
    }
}

fn planned_operations(item: &StartupItem, action: StartupAction) -> Vec<String> {
    match (item.source, action) {
        (StartupSource::RegistryRun, StartupAction::Disable) => vec![
            "写入 StartupApproved\\Run 禁用状态".to_string(),
            format!("保留原始 Run 值 {}", item.name),
        ],
        (StartupSource::RegistryRun, StartupAction::Enable) => vec![
            "写入 StartupApproved\\Run 启用状态".to_string(),
            format!("保留原始 Run 值 {}", item.name),
        ],
        (StartupSource::RegistryPolicyRun, StartupAction::Disable) => vec![
            "写入本地禁用回收站快照".to_string(),
            format!("删除策略 Run 值 {}", item.name),
        ],
        (StartupSource::StartupFolder, StartupAction::Delete) => vec![
            "保存启动目录文件快照".to_string(),
            format!("删除启动目录文件 {}", item.locator.location),
        ],
        (StartupSource::ScheduledTask, StartupAction::Delete) => vec![
            "导出计划任务 XML 快照".to_string(),
            format!("删除计划任务 {}", item.locator.location),
        ],
        (StartupSource::Service, StartupAction::Disable) => vec![
            "保存服务启动类型快照".to_string(),
            format!("将服务 {} 设为 Disabled", item.locator.location),
        ],
        _ => vec![format!("执行 {} 操作", action.as_str())],
    }
}

fn capture_snapshot(item: &StartupItem) -> Result<StartupSnapshot, StartupError> {
    match item.source {
        StartupSource::RegistryRun
        | StartupSource::RegistryRunOnce
        | StartupSource::RegistryPolicyRun => registry_run::capture_snapshot(item),
        StartupSource::StartupFolder => startup_folder::capture_snapshot(item),
        StartupSource::ScheduledTask => scheduled_tasks::capture_snapshot(item),
        StartupSource::Service => services::capture_snapshot(item),
    }
}

#[cfg(test)]
mod tests {
    use super::list_sources;

    #[test]
    fn source_descriptors_cover_all_v1_sources() {
        let descriptors = list_sources();
        assert_eq!(descriptors.len(), 6);
        assert!(descriptors.iter().any(|value| value.label == "计划任务"));
    }
}
