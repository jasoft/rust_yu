//! 程序信息持久化存储模块
//!
//! 用于：
//! - 在卸载程序前保存注册表信息（供卸载后搜索残留）
//! - 使用 SQLite 缓存安装软件扫描结果，减少重复全量扫描

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use rusqlite::{params, Connection};

use crate::modules::common::error::UninstallerError;

use super::models::InstalledProgram;

const STORAGE_DIR_ENV: &str = "RUST_YU_STORAGE_DIR";
const SNAPSHOT_FILE_NAME: &str = "programs.json";
const SCAN_CACHE_DB_FILE_NAME: &str = "installed_programs_cache_v4.sqlite3";
const ICON_CACHE_DIR_NAME: &str = "icon-cache";
const CACHE_TABLE_NAME: &str = "installed_programs_cache";
const CACHE_METADATA_TABLE_NAME: &str = "cache_metadata";
const META_KEY_SCHEMA_VERSION: &str = "schema_version";
const META_KEY_GENERATED_AT: &str = "generated_at";
pub const CACHE_SCHEMA_VERSION: u32 = 4;
pub const DEFAULT_CACHE_TTL_SECONDS: i64 = 900;

#[cfg(test)]
pub(crate) static TEST_STORAGE_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[derive(Debug, Clone)]
pub struct ScanCacheReadResult {
    pub entries: Option<Vec<InstalledProgram>>,
    pub cache_hit: bool,
    pub cache_valid: bool,
    pub schema_version: u32,
    pub generated_at: Option<String>,
    pub reason: Option<String>,
}

impl Default for ScanCacheReadResult {
    fn default() -> Self {
        Self {
            entries: None,
            cache_hit: false,
            cache_valid: false,
            schema_version: CACHE_SCHEMA_VERSION,
            generated_at: None,
            reason: None,
        }
    }
}

/// 获取存储目录
fn get_storage_dir() -> Result<PathBuf, UninstallerError> {
    let base_dir = if let Ok(override_dir) = std::env::var(STORAGE_DIR_ENV) {
        PathBuf::from(override_dir)
    } else {
        dirs::data_dir()
            .ok_or_else(|| UninstallerError::Other("无法获取 AppData 目录".to_string()))?
            .join("rust-yu")
    };

    std::fs::create_dir_all(&base_dir)?;
    Ok(base_dir)
}

/// 获取缓存根目录（用于对外展示路径）
pub fn get_storage_root_dir() -> Result<PathBuf, UninstallerError> {
    get_storage_dir()
}

/// 获取程序快照文件路径
fn get_snapshot_file() -> Result<PathBuf, UninstallerError> {
    Ok(get_storage_dir()?.join(SNAPSHOT_FILE_NAME))
}

/// 获取扫描缓存 SQLite 文件路径
fn get_scan_cache_file() -> Result<PathBuf, UninstallerError> {
    Ok(get_storage_dir()?.join(SCAN_CACHE_DB_FILE_NAME))
}

/// 获取扫描缓存文件路径（逻辑上等价于缓存数据库路径）
pub fn get_scan_cache_database_path() -> Result<PathBuf, UninstallerError> {
    get_scan_cache_file()
}

/// 获取图标缓存目录
pub fn get_icon_cache_dir() -> Result<PathBuf, UninstallerError> {
    let icon_cache_dir = get_storage_dir()?.join(ICON_CACHE_DIR_NAME);
    std::fs::create_dir_all(&icon_cache_dir)?;
    Ok(icon_cache_dir)
}

fn map_sqlite_error(context: &str, error: rusqlite::Error) -> UninstallerError {
    UninstallerError::Other(format!("{context}: {error}"))
}

fn open_scan_cache_connection() -> Result<Connection, UninstallerError> {
    let db_path = get_scan_cache_file()?;
    let connection = Connection::open(&db_path)
        .map_err(|error| map_sqlite_error("打开缓存数据库失败", error))?;

    connection
        .execute_batch(&format!(
            r#"
            PRAGMA journal_mode=WAL;
            PRAGMA synchronous=NORMAL;
            CREATE TABLE IF NOT EXISTS {cache_table} (
                cache_key TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                publisher TEXT,
                version TEXT,
                install_date TEXT,
                install_location TEXT,
                uninstall_string TEXT,
                install_source TEXT NOT NULL,
                size_bytes INTEGER,
                estimated_size_bytes INTEGER,
                icon_path TEXT,
                icon_cache_path_32 TEXT,
                icon_cache_path_48 TEXT,
                size_last_updated_at TEXT,
                payload_json TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_installed_programs_cache_name
                ON {cache_table}(name);
            CREATE TABLE IF NOT EXISTS {metadata_table} (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            "#,
            cache_table = CACHE_TABLE_NAME,
            metadata_table = CACHE_METADATA_TABLE_NAME
        ))
        .map_err(|error| map_sqlite_error("初始化缓存数据库结构失败", error))?;

    Ok(connection)
}

fn read_cache_metadata(
    connection: &Connection,
    key: &str,
) -> Result<Option<String>, UninstallerError> {
    let mut statement = connection
        .prepare(&format!(
            "SELECT value FROM {} WHERE key = ?1 LIMIT 1",
            CACHE_METADATA_TABLE_NAME
        ))
        .map_err(|error| map_sqlite_error("准备读取缓存元数据失败", error))?;

    let mut rows = statement
        .query(params![key])
        .map_err(|error| map_sqlite_error("读取缓存元数据失败", error))?;

    if let Some(row) = rows
        .next()
        .map_err(|error| map_sqlite_error("读取缓存元数据结果失败", error))?
    {
        let value = row
            .get::<usize, String>(0)
            .map_err(|error| map_sqlite_error("解析缓存元数据失败", error))?;
        return Ok(Some(value));
    }

    Ok(None)
}

fn write_cache_metadata(
    connection: &Connection,
    key: &str,
    value: &str,
) -> Result<(), UninstallerError> {
    connection
        .execute(
            &format!(
                "INSERT INTO {} (key, value, updated_at)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
                CACHE_METADATA_TABLE_NAME
            ),
            params![key, value, Utc::now().to_rfc3339()],
        )
        .map_err(|error| map_sqlite_error("写入缓存元数据失败", error))?;
    Ok(())
}

fn build_program_cache_key(program: &InstalledProgram) -> String {
    let mut hasher = DefaultHasher::new();
    program.name.to_lowercase().hash(&mut hasher);
    program
        .publisher
        .as_deref()
        .unwrap_or_default()
        .to_lowercase()
        .hash(&mut hasher);
    program
        .uninstall_string
        .as_deref()
        .unwrap_or_default()
        .to_lowercase()
        .hash(&mut hasher);
    program
        .install_location
        .as_deref()
        .unwrap_or_default()
        .to_lowercase()
        .hash(&mut hasher);
    program.install_source.to_string().hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// 保存程序快照
pub fn save_program_snapshot(programs: &[InstalledProgram]) -> Result<(), UninstallerError> {
    let path = get_snapshot_file()?;

    let mut all_programs: Vec<InstalledProgram> = if path.exists() {
        let content = std::fs::read_to_string(&path)?;
        serde_json::from_str(&content).unwrap_or_default()
    } else {
        Vec::new()
    };

    for program in programs {
        all_programs.retain(|p| p.name.to_lowercase() != program.name.to_lowercase());
        all_programs.push(program.clone());
    }

    let content = serde_json::to_string_pretty(&all_programs)
        .map_err(|error| UninstallerError::Serde(error.to_string()))?;
    std::fs::write(&path, content)?;

    tracing::info!("已保存 {} 个程序信息到快照", programs.len());
    Ok(())
}

/// 获取所有保存的程序
#[allow(dead_code)]
pub fn get_saved_programs() -> Result<Vec<InstalledProgram>, UninstallerError> {
    let path = get_snapshot_file()?;

    if !path.exists() {
        return Ok(Vec::new());
    }

    let content = std::fs::read_to_string(&path)?;
    let programs: Vec<InstalledProgram> = serde_json::from_str(&content).unwrap_or_default();

    Ok(programs)
}

/// 根据名称获取保存的程序
#[allow(dead_code)]
pub fn get_saved_program(name: &str) -> Result<Option<InstalledProgram>, UninstallerError> {
    let programs = get_saved_programs()?;
    let name_lower = name.to_lowercase();

    Ok(programs
        .into_iter()
        .find(|program| program.name.to_lowercase().contains(&name_lower)))
}

/// 删除保存的程序信息
pub fn delete_saved_program(name: &str) -> Result<(), UninstallerError> {
    let path = get_snapshot_file()?;

    if !path.exists() {
        return Ok(());
    }

    let content = std::fs::read_to_string(&path)?;
    let mut programs: Vec<InstalledProgram> = serde_json::from_str(&content).unwrap_or_default();

    let name_lower = name.to_lowercase();
    programs.retain(|program| !program.name.to_lowercase().contains(&name_lower));

    let content = serde_json::to_string_pretty(&programs)
        .map_err(|error| UninstallerError::Serde(error.to_string()))?;
    std::fs::write(&path, content)?;

    Ok(())
}

/// 搜索时优先查询保存的数据
#[allow(dead_code)]
pub fn search_programs_with_fallback(
    query: &str,
) -> Result<Vec<InstalledProgram>, UninstallerError> {
    let saved = get_saved_programs()?;
    let query_lower = query.to_lowercase();

    let matched: Vec<InstalledProgram> = saved
        .into_iter()
        .filter(|program| {
            program.name.to_lowercase().contains(&query_lower)
                || program
                    .publisher
                    .as_ref()
                    .map(|publisher| publisher.to_lowercase().contains(&query_lower))
                    .unwrap_or(false)
        })
        .collect();

    Ok(matched)
}

/// 保存扫描缓存（SQLite）
pub fn save_scan_cache(entries: &[InstalledProgram]) -> Result<(), UninstallerError> {
    let mut connection = open_scan_cache_connection()?;
    let transaction = connection
        .transaction()
        .map_err(|error| map_sqlite_error("开启缓存事务失败", error))?;

    transaction
        .execute(&format!("DELETE FROM {}", CACHE_TABLE_NAME), [])
        .map_err(|error| map_sqlite_error("清空旧缓存失败", error))?;

    let now = Utc::now().to_rfc3339();

    {
        let mut statement = transaction
            .prepare(&format!(
                "INSERT INTO {} (
                    cache_key,
                    name,
                    publisher,
                    version,
                    install_date,
                    install_location,
                    uninstall_string,
                    install_source,
                    size_bytes,
                    estimated_size_bytes,
                    icon_path,
                    icon_cache_path_32,
                    icon_cache_path_48,
                    size_last_updated_at,
                    payload_json,
                    updated_at
                ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
                CACHE_TABLE_NAME
            ))
            .map_err(|error| map_sqlite_error("准备写入缓存失败", error))?;

        for program in entries {
            let payload_json = serde_json::to_string(program)
                .map_err(|error| UninstallerError::Serde(error.to_string()))?;
            statement
                .execute(params![
                    build_program_cache_key(program),
                    program.name,
                    program.publisher,
                    program.version,
                    program.install_date,
                    program.install_location,
                    program.uninstall_string,
                    program.install_source.to_string(),
                    program.size,
                    program.estimated_size,
                    program.icon_path,
                    program.icon_cache_path_32,
                    program.icon_cache_path_48,
                    program.size_last_updated_at,
                    payload_json,
                    now,
                ])
                .map_err(|error| map_sqlite_error("写入缓存记录失败", error))?;
        }
    }

    write_cache_metadata(
        &transaction,
        META_KEY_SCHEMA_VERSION,
        &CACHE_SCHEMA_VERSION.to_string(),
    )?;
    write_cache_metadata(&transaction, META_KEY_GENERATED_AT, &now)?;

    transaction
        .commit()
        .map_err(|error| map_sqlite_error("提交缓存事务失败", error))?;
    Ok(())
}

/// 读取扫描缓存（包含有效性校验）
pub fn read_scan_cache(ttl_seconds: i64) -> Result<ScanCacheReadResult, UninstallerError> {
    let cache_db_path = get_scan_cache_file()?;
    if !cache_db_path.exists() {
        return Ok(ScanCacheReadResult {
            reason: Some("cache_missing".to_string()),
            ..ScanCacheReadResult::default()
        });
    }

    let connection = open_scan_cache_connection()?;

    let schema_version = read_cache_metadata(&connection, META_KEY_SCHEMA_VERSION)?
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or_default();
    let generated_at = read_cache_metadata(&connection, META_KEY_GENERATED_AT)?;

    if schema_version != CACHE_SCHEMA_VERSION {
        return Ok(ScanCacheReadResult {
            schema_version,
            generated_at,
            reason: Some("schema_mismatch".to_string()),
            ..ScanCacheReadResult::default()
        });
    }

    let generated_at_value = match generated_at.clone() {
        Some(value) => value,
        None => {
            return Ok(ScanCacheReadResult {
                schema_version,
                reason: Some("cache_missing_generated_at".to_string()),
                ..ScanCacheReadResult::default()
            });
        }
    };

    let generated_at_time = match DateTime::parse_from_rfc3339(&generated_at_value) {
        Ok(value) => value.with_timezone(&Utc),
        Err(_) => {
            return Ok(ScanCacheReadResult {
                schema_version,
                generated_at: Some(generated_at_value),
                reason: Some("cache_invalid_generated_at".to_string()),
                ..ScanCacheReadResult::default()
            });
        }
    };

    let ttl = ttl_seconds.max(1);
    if Utc::now()
        .signed_duration_since(generated_at_time)
        .num_seconds()
        > ttl
    {
        return Ok(ScanCacheReadResult {
            schema_version,
            generated_at: Some(generated_at_value),
            reason: Some("cache_expired".to_string()),
            ..ScanCacheReadResult::default()
        });
    }

    let mut statement = connection
        .prepare(&format!(
            "SELECT payload_json FROM {} ORDER BY name COLLATE NOCASE",
            CACHE_TABLE_NAME
        ))
        .map_err(|error| map_sqlite_error("准备读取缓存列表失败", error))?;

    let mut rows = statement
        .query([])
        .map_err(|error| map_sqlite_error("读取缓存列表失败", error))?;

    let mut programs = Vec::new();
    while let Some(row) = rows
        .next()
        .map_err(|error| map_sqlite_error("读取缓存行失败", error))?
    {
        let payload_json = row
            .get::<usize, String>(0)
            .map_err(|error| map_sqlite_error("读取缓存负载失败", error))?;
        if let Ok(program) = serde_json::from_str::<InstalledProgram>(&payload_json) {
            programs.push(program);
        }
    }

    if programs.is_empty() {
        return Ok(ScanCacheReadResult {
            schema_version,
            generated_at: Some(generated_at_value),
            reason: Some("cache_empty".to_string()),
            ..ScanCacheReadResult::default()
        });
    }

    Ok(ScanCacheReadResult {
        entries: Some(programs),
        cache_hit: true,
        cache_valid: true,
        schema_version,
        generated_at: Some(generated_at_value),
        reason: None,
    })
}

/// 使扫描缓存失效
pub fn invalidate_scan_cache() -> Result<(), UninstallerError> {
    let path = get_scan_cache_file()?;
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    Ok(())
}

/// 按程序名称使缓存失效（当前实现保守地整表失效）
pub fn invalidate_scan_cache_for_program(_program_name: &str) -> Result<(), UninstallerError> {
    invalidate_scan_cache()
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::modules::lister::models::InstallSource;

    fn with_storage_root(test_name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "rust-yu-storage-test-{}-{}",
            test_name,
            uuid::Uuid::new_v4()
        ));
        let _ = fs::create_dir_all(&root);
        std::env::set_var(STORAGE_DIR_ENV, &root);
        root
    }

    fn cleanup_storage_root(root: &PathBuf) {
        std::env::remove_var(STORAGE_DIR_ENV);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn read_scan_cache_returns_miss_when_file_not_exists() {
        let _guard = super::TEST_STORAGE_ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let root = with_storage_root("miss");
        let result = read_scan_cache(DEFAULT_CACHE_TTL_SECONDS).unwrap_or_default();
        assert!(!result.cache_hit);
        assert_eq!(result.reason, Some("cache_missing".to_string()));
        cleanup_storage_root(&root);
    }

    #[test]
    fn read_scan_cache_returns_hit_after_save() {
        let _guard = super::TEST_STORAGE_ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let root = with_storage_root("hit");

        let mut program = InstalledProgram::new("Demo".to_string(), InstallSource::Registry);
        program.version = Some("1.0.0".to_string());
        assert!(save_scan_cache(&[program]).is_ok());

        let result = read_scan_cache(DEFAULT_CACHE_TTL_SECONDS).unwrap_or_default();
        assert!(result.cache_hit);
        assert!(result.cache_valid);
        assert!(result.entries.unwrap_or_default().len() == 1);

        cleanup_storage_root(&root);
    }

    #[test]
    fn read_scan_cache_restores_icon_paths_and_size_timestamp() {
        let _guard = super::TEST_STORAGE_ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let root = with_storage_root("metadata-fields");

        let mut program =
            InstalledProgram::new("DemoCacheMeta".to_string(), InstallSource::Registry);
        program.icon_cache_path_32 = Some(r"C:\cache\icon\32\demo.png".to_string());
        program.icon_cache_path_48 = Some(r"C:\cache\icon\48\demo.png".to_string());
        program.size_last_updated_at = Some("2026-02-15T00:00:00Z".to_string());
        program.size = Some(4096);
        assert!(save_scan_cache(&[program]).is_ok());

        let result = read_scan_cache(DEFAULT_CACHE_TTL_SECONDS).unwrap_or_default();
        assert!(result.cache_hit);
        let entries = result.entries.unwrap_or_default();
        assert_eq!(entries.len(), 1);
        let cached = entries.first().cloned().unwrap_or_else(|| {
            InstalledProgram::new("invalid".to_string(), InstallSource::Unknown)
        });
        assert_eq!(
            cached.icon_cache_path_32,
            Some(r"C:\cache\icon\32\demo.png".to_string())
        );
        assert_eq!(
            cached.icon_cache_path_48,
            Some(r"C:\cache\icon\48\demo.png".to_string())
        );
        assert_eq!(
            cached.size_last_updated_at,
            Some("2026-02-15T00:00:00Z".to_string())
        );
        assert_eq!(cached.size, Some(4096));

        cleanup_storage_root(&root);
    }

    #[test]
    fn read_scan_cache_detects_schema_mismatch() {
        let _guard = super::TEST_STORAGE_ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let root = with_storage_root("schema");

        let connection = open_scan_cache_connection().unwrap_or_else(|_| panic!("open db failed"));
        assert!(write_cache_metadata(&connection, META_KEY_SCHEMA_VERSION, "999").is_ok());
        assert!(
            write_cache_metadata(&connection, META_KEY_GENERATED_AT, &Utc::now().to_rfc3339())
                .is_ok()
        );

        let result = read_scan_cache(DEFAULT_CACHE_TTL_SECONDS).unwrap_or_default();
        assert!(!result.cache_hit);
        assert_eq!(result.reason, Some("schema_mismatch".to_string()));

        cleanup_storage_root(&root);
    }

    #[test]
    fn force_cache_invalidation_removes_cache_file() {
        let _guard = super::TEST_STORAGE_ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let root = with_storage_root("invalidate");
        let program = InstalledProgram::new("Demo".to_string(), InstallSource::Registry);
        assert!(save_scan_cache(&[program]).is_ok());
        assert!(get_scan_cache_file()
            .map(|path| path.exists())
            .unwrap_or(false));

        assert!(invalidate_scan_cache().is_ok());
        assert!(!get_scan_cache_file()
            .map(|path| path.exists())
            .unwrap_or(true));

        cleanup_storage_root(&root);
    }

    #[test]
    fn cache_paths_are_under_storage_root() {
        let _guard = super::TEST_STORAGE_ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let root = with_storage_root("paths");

        let storage_root = get_storage_root_dir().unwrap_or_else(|_| PathBuf::new());
        let icon_cache_dir = get_icon_cache_dir().unwrap_or_else(|_| PathBuf::new());
        let scan_db_path = get_scan_cache_database_path().unwrap_or_else(|_| PathBuf::new());

        assert!(storage_root.starts_with(&root));
        assert!(icon_cache_dir.starts_with(&root));
        assert!(scan_db_path.starts_with(&root));
        assert!(icon_cache_dir.exists());
        assert_eq!(
            scan_db_path.file_name().and_then(|name| name.to_str()),
            Some(SCAN_CACHE_DB_FILE_NAME)
        );

        cleanup_storage_root(&root);
    }
}
