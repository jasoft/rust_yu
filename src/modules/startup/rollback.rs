use std::path::PathBuf;

use chrono::Utc;
use rusqlite::{params, Connection};

use crate::modules::lister::storage::get_storage_root_dir;

use super::models::{
    DisabledStartupRecord, StartupChangeLog, StartupError, StartupErrorCode, StartupScope,
    StartupSource,
};

const STARTUP_STATE_DB_FILE_NAME: &str = "startup_state_v1.sqlite3";
const CHANGE_LOG_TABLE: &str = "startup_change_log";
const DISABLED_ENTRY_TABLE: &str = "startup_disabled_entry";

#[cfg(test)]
static TEST_STARTUP_STORAGE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn map_sqlite_error(context: &str, error: rusqlite::Error) -> StartupError {
    StartupError::new(
        StartupErrorCode::IoError,
        format!("{context}: {error}"),
    )
}

pub fn get_state_database_path() -> Result<PathBuf, StartupError> {
    let root = get_storage_root_dir().map_err(|error| {
        StartupError::new(
            StartupErrorCode::IoError,
            format!("获取自启动存储目录失败: {error}"),
        )
    })?;
    Ok(root.join(STARTUP_STATE_DB_FILE_NAME))
}

fn open_connection() -> Result<Connection, StartupError> {
    let path = get_state_database_path()?;
    let connection =
        Connection::open(path).map_err(|error| map_sqlite_error("打开自启动状态数据库失败", error))?;

    connection
        .execute_batch(&format!(
            r#"
            PRAGMA journal_mode=WAL;
            PRAGMA synchronous=NORMAL;
            CREATE TABLE IF NOT EXISTS {change_table} (
                change_id TEXT PRIMARY KEY,
                item_id TEXT NOT NULL,
                action TEXT NOT NULL,
                source TEXT NOT NULL,
                scope TEXT NOT NULL,
                created_at TEXT NOT NULL,
                reason TEXT,
                snapshot_json TEXT NOT NULL,
                restored_at TEXT
            );
            CREATE TABLE IF NOT EXISTS {disabled_table} (
                item_id TEXT PRIMARY KEY,
                source TEXT NOT NULL,
                scope TEXT NOT NULL,
                snapshot_json TEXT NOT NULL,
                disabled_at TEXT NOT NULL
            );
            "#,
            change_table = CHANGE_LOG_TABLE,
            disabled_table = DISABLED_ENTRY_TABLE
        ))
        .map_err(|error| map_sqlite_error("初始化自启动状态数据库结构失败", error))?;

    Ok(connection)
}

pub fn save_change_log(change_log: &StartupChangeLog) -> Result<(), StartupError> {
    let connection = open_connection()?;
    connection
        .execute(
            &format!(
                "INSERT OR REPLACE INTO {} \
                 (change_id, item_id, action, source, scope, created_at, reason, snapshot_json, restored_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                CHANGE_LOG_TABLE
            ),
            params![
                change_log.change_id,
                change_log.item_id,
                change_log.action.as_str(),
                change_log.source.as_str(),
                change_log.scope.as_str(),
                change_log.created_at,
                change_log.reason,
                change_log.snapshot_json,
                change_log.restored_at,
            ],
        )
        .map_err(|error| map_sqlite_error("写入变更日志失败", error))?;

    Ok(())
}

pub fn get_change_log(change_id: &str) -> Result<Option<StartupChangeLog>, StartupError> {
    let connection = open_connection()?;
    let mut statement = connection
        .prepare(&format!(
            "SELECT change_id, item_id, action, source, scope, created_at, reason, snapshot_json, restored_at \
             FROM {} WHERE change_id = ?1 LIMIT 1",
            CHANGE_LOG_TABLE
        ))
        .map_err(|error| map_sqlite_error("准备读取变更日志失败", error))?;

    let mut rows = statement
        .query(params![change_id])
        .map_err(|error| map_sqlite_error("读取变更日志失败", error))?;

    let Some(row) = rows
        .next()
        .map_err(|error| map_sqlite_error("遍历变更日志失败", error))?
    else {
        return Ok(None);
    };

    Ok(Some(StartupChangeLog {
        change_id: row
            .get(0)
            .map_err(|error| map_sqlite_error("读取变更 ID 失败", error))?,
        item_id: row
            .get(1)
            .map_err(|error| map_sqlite_error("读取项目 ID 失败", error))?,
        action: parse_action(
            &row.get::<usize, String>(2)
                .map_err(|error| map_sqlite_error("读取动作失败", error))?,
        )?,
        source: parse_source(
            &row.get::<usize, String>(3)
                .map_err(|error| map_sqlite_error("读取来源失败", error))?,
        )?,
        scope: parse_scope(
            &row.get::<usize, String>(4)
                .map_err(|error| map_sqlite_error("读取作用域失败", error))?,
        )?,
        created_at: row
            .get(5)
            .map_err(|error| map_sqlite_error("读取创建时间失败", error))?,
        reason: row
            .get(6)
            .map_err(|error| map_sqlite_error("读取原因失败", error))?,
        snapshot_json: row
            .get(7)
            .map_err(|error| map_sqlite_error("读取快照失败", error))?,
        restored_at: row
            .get(8)
            .map_err(|error| map_sqlite_error("读取恢复时间失败", error))?,
    }))
}

pub fn mark_change_log_restored(change_id: &str) -> Result<(), StartupError> {
    let connection = open_connection()?;
    connection
        .execute(
            &format!(
                "UPDATE {} SET restored_at = ?1 WHERE change_id = ?2",
                CHANGE_LOG_TABLE
            ),
            params![Utc::now().to_rfc3339(), change_id],
        )
        .map_err(|error| map_sqlite_error("更新变更恢复状态失败", error))?;
    Ok(())
}

pub fn save_disabled_entry(record: &DisabledStartupRecord) -> Result<(), StartupError> {
    let connection = open_connection()?;
    connection
        .execute(
            &format!(
                "INSERT OR REPLACE INTO {} (item_id, source, scope, snapshot_json, disabled_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                DISABLED_ENTRY_TABLE
            ),
            params![
                record.item_id,
                record.source.as_str(),
                record.scope.as_str(),
                record.snapshot_json,
                record.disabled_at,
            ],
        )
        .map_err(|error| map_sqlite_error("写入已禁用记录失败", error))?;
    Ok(())
}

pub fn get_disabled_entry(item_id: &str) -> Result<Option<DisabledStartupRecord>, StartupError> {
    let connection = open_connection()?;
    let mut statement = connection
        .prepare(&format!(
            "SELECT item_id, source, scope, snapshot_json, disabled_at FROM {} WHERE item_id = ?1 LIMIT 1",
            DISABLED_ENTRY_TABLE
        ))
        .map_err(|error| map_sqlite_error("准备读取已禁用记录失败", error))?;
    let mut rows = statement
        .query(params![item_id])
        .map_err(|error| map_sqlite_error("读取已禁用记录失败", error))?;

    let Some(row) = rows
        .next()
        .map_err(|error| map_sqlite_error("遍历已禁用记录失败", error))?
    else {
        return Ok(None);
    };

    Ok(Some(DisabledStartupRecord {
        item_id: row
            .get(0)
            .map_err(|error| map_sqlite_error("读取项目 ID 失败", error))?,
        source: parse_source(
            &row.get::<usize, String>(1)
                .map_err(|error| map_sqlite_error("读取来源失败", error))?,
        )?,
        scope: parse_scope(
            &row.get::<usize, String>(2)
                .map_err(|error| map_sqlite_error("读取作用域失败", error))?,
        )?,
        snapshot_json: row
            .get(3)
            .map_err(|error| map_sqlite_error("读取快照失败", error))?,
        disabled_at: row
            .get(4)
            .map_err(|error| map_sqlite_error("读取禁用时间失败", error))?,
    }))
}

pub fn list_disabled_entries(
    source: Option<StartupSource>,
    scope: Option<StartupScope>,
) -> Result<Vec<DisabledStartupRecord>, StartupError> {
    let connection = open_connection()?;
    let mut statement = connection
        .prepare(&format!(
            "SELECT item_id, source, scope, snapshot_json, disabled_at FROM {}",
            DISABLED_ENTRY_TABLE
        ))
        .map_err(|error| map_sqlite_error("准备读取已禁用记录列表失败", error))?;
    let mut rows = statement
        .query([])
        .map_err(|error| map_sqlite_error("读取已禁用记录列表失败", error))?;

    let mut records = Vec::new();
    while let Some(row) = rows
        .next()
        .map_err(|error| map_sqlite_error("遍历已禁用记录列表失败", error))?
    {
        let record = DisabledStartupRecord {
            item_id: row
                .get(0)
                .map_err(|error| map_sqlite_error("读取项目 ID 失败", error))?,
            source: parse_source(
                &row.get::<usize, String>(1)
                    .map_err(|error| map_sqlite_error("读取来源失败", error))?,
            )?,
            scope: parse_scope(
                &row.get::<usize, String>(2)
                    .map_err(|error| map_sqlite_error("读取作用域失败", error))?,
            )?,
            snapshot_json: row
                .get(3)
                .map_err(|error| map_sqlite_error("读取快照失败", error))?,
            disabled_at: row
                .get(4)
                .map_err(|error| map_sqlite_error("读取禁用时间失败", error))?,
        };

        let source_matches = source.map(|value| value == record.source).unwrap_or(true);
        let scope_matches = scope.map(|value| value == record.scope).unwrap_or(true);
        if source_matches && scope_matches {
            records.push(record);
        }
    }

    Ok(records)
}

pub fn remove_disabled_entry(item_id: &str) -> Result<(), StartupError> {
    let connection = open_connection()?;
    connection
        .execute(
            &format!("DELETE FROM {} WHERE item_id = ?1", DISABLED_ENTRY_TABLE),
            params![item_id],
        )
        .map_err(|error| map_sqlite_error("删除已禁用记录失败", error))?;
    Ok(())
}

fn parse_action(value: &str) -> Result<super::models::StartupAction, StartupError> {
    match value {
        "enable" => Ok(super::models::StartupAction::Enable),
        "disable" => Ok(super::models::StartupAction::Disable),
        "delete" => Ok(super::models::StartupAction::Delete),
        "rollback" => Ok(super::models::StartupAction::Rollback),
        _ => Err(StartupError::new(
            StartupErrorCode::IoError,
            format!("未知动作: {value}"),
        )),
    }
}

fn parse_source(value: &str) -> Result<StartupSource, StartupError> {
    match value {
        "registry_run" => Ok(StartupSource::RegistryRun),
        "registry_run_once" => Ok(StartupSource::RegistryRunOnce),
        "registry_policy_run" => Ok(StartupSource::RegistryPolicyRun),
        "startup_folder" => Ok(StartupSource::StartupFolder),
        "scheduled_task" => Ok(StartupSource::ScheduledTask),
        "service" => Ok(StartupSource::Service),
        _ => Err(StartupError::new(
            StartupErrorCode::IoError,
            format!("未知来源: {value}"),
        )),
    }
}

fn parse_scope(value: &str) -> Result<StartupScope, StartupError> {
    match value {
        "user" => Ok(StartupScope::User),
        "machine" => Ok(StartupScope::Machine),
        _ => Err(StartupError::new(
            StartupErrorCode::IoError,
            format!("未知作用域: {value}"),
        )),
    }
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::{
        get_change_log, get_disabled_entry, list_disabled_entries, mark_change_log_restored,
        remove_disabled_entry, save_change_log, save_disabled_entry, TEST_STARTUP_STORAGE_LOCK,
    };
    use crate::modules::startup::models::{
        DisabledStartupRecord, StartupAction, StartupChangeLog, StartupItem, StartupLocator,
        StartupScope, StartupSnapshot, StartupSource, StartupState,
    };
    use crate::modules::startup::TEST_STARTUP_ENV_LOCK;

    fn prepare_storage_root(test_name: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "rust-yu-startup-storage-{}-{}",
            test_name,
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).ok();
        std::env::set_var("RUST_YU_STORAGE_DIR", &root);
        root
    }

    fn cleanup_storage_root(root: &std::path::PathBuf) {
        std::env::remove_var("RUST_YU_STORAGE_DIR");
        std::fs::remove_dir_all(root).ok();
    }

    fn sample_snapshot() -> StartupSnapshot {
        let mut item = StartupItem::new(
            "Demo",
            StartupSource::RegistryPolicyRun,
            StartupScope::User,
            StartupLocator {
                location: "HKCU\\Software\\Demo\\Run\\Demo".to_string(),
                bucket: Some("run".to_string()),
            },
        );
        item.state = StartupState::Disabled;
        StartupSnapshot {
            item,
            source_payload: serde_json::json!({"value_name": "Demo"}),
        }
    }

    #[test]
    fn change_log_roundtrip_and_restore_mark_work() {
        let _guard = TEST_STARTUP_STORAGE_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let _env_guard = TEST_STARTUP_ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let root = prepare_storage_root("change-log");
        let snapshot_json =
            serde_json::to_string(&sample_snapshot()).unwrap_or_else(|_| "{}".to_string());
        let change_log = StartupChangeLog {
            change_id: "change-1".to_string(),
            item_id: "item-1".to_string(),
            action: StartupAction::Disable,
            source: StartupSource::RegistryPolicyRun,
            scope: StartupScope::User,
            created_at: Utc::now().to_rfc3339(),
            reason: Some("test".to_string()),
            snapshot_json,
            restored_at: None,
        };

        assert!(save_change_log(&change_log).is_ok());
        let stored = get_change_log("change-1")
            .ok()
            .flatten()
            .unwrap_or_else(|| panic!("expected change log"));
        assert_eq!(stored.item_id, "item-1");
        assert!(mark_change_log_restored("change-1").is_ok());
        let restored = get_change_log("change-1")
            .ok()
            .flatten()
            .unwrap_or_else(|| panic!("expected restored change log"));
        assert!(restored.restored_at.is_some());

        cleanup_storage_root(&root);
    }

    #[test]
    fn disabled_entry_roundtrip_supports_listing_and_removal() {
        let _guard = TEST_STARTUP_STORAGE_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let _env_guard = TEST_STARTUP_ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let root = prepare_storage_root("disabled-entry");
        let snapshot_json =
            serde_json::to_string(&sample_snapshot()).unwrap_or_else(|_| "{}".to_string());
        let record = DisabledStartupRecord {
            item_id: "item-2".to_string(),
            source: StartupSource::RegistryPolicyRun,
            scope: StartupScope::User,
            snapshot_json,
            disabled_at: Utc::now().to_rfc3339(),
        };

        assert!(save_disabled_entry(&record).is_ok());
        let stored = get_disabled_entry("item-2")
            .ok()
            .flatten()
            .unwrap_or_else(|| panic!("expected disabled entry"));
        assert_eq!(stored.item_id, "item-2");
        let listed = list_disabled_entries(
            Some(StartupSource::RegistryPolicyRun),
            Some(StartupScope::User),
        )
        .unwrap_or_default();
        assert_eq!(listed.len(), 1);
        assert!(remove_disabled_entry("item-2").is_ok());
        assert!(get_disabled_entry("item-2").unwrap_or(None).is_none());

        cleanup_storage_root(&root);
    }
}
