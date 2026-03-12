use winreg::enums::{
    HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, RegType, KEY_READ, KEY_WRITE, REG_EXPAND_SZ, REG_SZ,
};
use winreg::{RegKey, RegValue, HKEY};

use crate::modules::common::utils::split_command_for_spawn;

use super::models::{
    hex_decode, hex_encode, DisabledStartupRecord, StartupError, StartupErrorCode, StartupItem,
    StartupLocator, StartupScope, StartupSnapshot, StartupSource, StartupState,
};
use super::rollback;
use super::startup_approved;

const DEFAULT_RUN_PATH: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const DEFAULT_RUN_ONCE_PATH: &str = r"Software\Microsoft\Windows\CurrentVersion\RunOnce";
const DEFAULT_POLICY_RUN_PATH: &str =
    r"Software\Microsoft\Windows\CurrentVersion\Policies\Explorer\Run";
const DEFAULT_MACHINE_WOW64_RUN_PATH: &str =
    r"Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Run";
const DEFAULT_MACHINE_WOW64_RUN_ONCE_PATH: &str =
    r"Software\WOW6432Node\Microsoft\Windows\CurrentVersion\RunOnce";
const DEFAULT_MACHINE_WOW64_POLICY_RUN_PATH: &str =
    r"Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Policies\Explorer\Run";

#[derive(Debug, Clone)]
struct RegistrySourceConfig {
    source: StartupSource,
    scope: StartupScope,
    hive: HKEY,
    path: String,
    locator_bucket: String,
    approved_bucket: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct RegistrySnapshotPayload {
    hive: String,
    path: String,
    value_name: String,
    value_hex: String,
    value_type: u32,
    approved_bucket: Option<String>,
    approved_value_name: Option<String>,
    approved_state_hex: Option<String>,
}

pub fn collect_items(include_raw: bool) -> Result<Vec<StartupItem>, StartupError> {
    let mut items = Vec::new();

    for config in build_configs() {
        items.extend(read_config_items(&config, include_raw)?);
    }

    items.extend(read_disabled_policy_items(include_raw)?);
    Ok(items)
}

pub fn preview_create_user_run_item(
    name: &str,
    command: &str,
    include_raw: bool,
) -> Result<StartupItem, StartupError> {
    let config = build_configs()
        .into_iter()
        .find(|value| {
            value.source == StartupSource::RegistryRun
                && value.scope == StartupScope::User
                && value.hive == HKEY_CURRENT_USER
        })
        .ok_or_else(|| StartupError::new(StartupErrorCode::IoError, "未找到 HKCU Run 配置"))?;

    validate_create_user_run_item(&config, name, command)?;

    Ok(build_user_run_item(&config, name, command, include_raw))
}

pub fn create_user_run_item(name: &str, command: &str) -> Result<StartupItem, StartupError> {
    let item = preview_create_user_run_item(name, command, true)?;
    let config = resolve_config_for_item(&item)?;
    let hive = RegKey::predef(config.hive);
    let (subkey, _) = hive.create_subkey(&config.path).map_err(|error| {
        StartupError::new(
            StartupErrorCode::IoError,
            format!("创建注册表键失败: {error}"),
        )
    })?;

    subkey
        .set_value(name, &command.to_string())
        .map_err(|error| {
            StartupError::new(
                StartupErrorCode::IoError,
                format!("写入注册表启动项失败: {error}"),
            )
        })?;

    if let Some(bucket) = config.approved_bucket.as_deref() {
        startup_approved::write_state(StartupScope::User, bucket, name, true)?;
    }

    Ok(build_user_run_item(&config, name, command, true))
}

pub fn capture_snapshot(item: &StartupItem) -> Result<StartupSnapshot, StartupError> {
    if item.source == StartupSource::RegistryPolicyRun && item.state == StartupState::Disabled {
        let disabled_entry = rollback::get_disabled_entry(&item.id)?.ok_or_else(|| {
            StartupError::new(StartupErrorCode::NotFound, "未找到禁用状态快照")
        })?;
        return serde_json::from_str::<StartupSnapshot>(&disabled_entry.snapshot_json).map_err(
            |error| {
                StartupError::new(
                    StartupErrorCode::IoError,
                    format!("解析禁用状态快照失败: {error}"),
                )
            },
        );
    }

    let payload = snapshot_payload_from_item(item)?;
    Ok(StartupSnapshot {
        item: item.clone(),
        source_payload: serde_json::to_value(payload).map_err(|error| {
            StartupError::new(
                StartupErrorCode::IoError,
                format!("序列化注册表快照失败: {error}"),
            )
        })?,
    })
}

pub fn apply_action(
    item: &StartupItem,
    action: super::models::StartupAction,
    snapshot: &StartupSnapshot,
) -> Result<Vec<String>, StartupError> {
    let payload: RegistrySnapshotPayload =
        serde_json::from_value(snapshot.source_payload.clone()).map_err(|error| {
            StartupError::new(
                StartupErrorCode::IoError,
                format!("解析注册表来源快照失败: {error}"),
            )
        })?;
    let hive = parse_hive(&payload.hive)?;
    let key = RegKey::predef(hive);

    match action {
        super::models::StartupAction::Enable => {
            if item.source == StartupSource::RegistryRun {
                let bucket = payload
                    .approved_bucket
                    .clone()
                    .ok_or_else(|| StartupError::new(StartupErrorCode::Unsupported, "缺少 StartupApproved 桶"))?;
                startup_approved::write_state(item.scope, &bucket, &payload.value_name, true)?;
                return Ok(vec![format!("设置 {} 为已启用", payload.value_name)]);
            }

            if item.source == StartupSource::RegistryPolicyRun {
                if item.state == StartupState::Disabled {
                    restore_snapshot(snapshot)?;
                    rollback::remove_disabled_entry(&item.id)?;
                    return Ok(vec![format!("恢复策略启动项 {}", payload.value_name)]);
                }
            }

            Err(StartupError::new(
                StartupErrorCode::Unsupported,
                "当前注册表启动项不支持启用",
            ))
        }
        super::models::StartupAction::Disable => {
            if item.source == StartupSource::RegistryRun {
                let bucket = payload
                    .approved_bucket
                    .clone()
                    .ok_or_else(|| StartupError::new(StartupErrorCode::Unsupported, "缺少 StartupApproved 桶"))?;
                startup_approved::write_state(item.scope, &bucket, &payload.value_name, false)?;
                return Ok(vec![format!("设置 {} 为已禁用", payload.value_name)]);
            }

            if item.source == StartupSource::RegistryPolicyRun {
                delete_registry_value(&key, &payload.path, &payload.value_name)?;
                let disabled_record = DisabledStartupRecord {
                    item_id: item.id.clone(),
                    source: item.source,
                    scope: item.scope,
                    snapshot_json: serde_json::to_string(snapshot).map_err(|error| {
                        StartupError::new(
                            StartupErrorCode::IoError,
                            format!("序列化禁用快照失败: {error}"),
                        )
                    })?,
                    disabled_at: chrono::Utc::now().to_rfc3339(),
                };
                rollback::save_disabled_entry(&disabled_record)?;
                return Ok(vec![format!("禁用策略启动项 {}", payload.value_name)]);
            }

            Err(StartupError::new(
                StartupErrorCode::Unsupported,
                "当前注册表启动项不支持禁用",
            ))
        }
        super::models::StartupAction::Delete => {
            if item.source == StartupSource::RegistryPolicyRun && item.state == StartupState::Disabled {
                rollback::remove_disabled_entry(&item.id)?;
                return Ok(vec![format!("删除禁用状态的策略启动项 {}", payload.value_name)]);
            }

            delete_registry_value(&key, &payload.path, &payload.value_name)?;
            if let Some(bucket) = payload.approved_bucket.as_deref() {
                startup_approved::restore_state_hex(item.scope, bucket, &payload.value_name, None)?;
            }
            Ok(vec![format!("删除注册表启动项 {}", payload.value_name)])
        }
        _ => Err(StartupError::new(
            StartupErrorCode::Unsupported,
            "当前注册表启动项不支持该操作",
        )),
    }
}

pub fn restore_snapshot(snapshot: &StartupSnapshot) -> Result<Vec<String>, StartupError> {
    let payload: RegistrySnapshotPayload =
        serde_json::from_value(snapshot.source_payload.clone()).map_err(|error| {
            StartupError::new(
                StartupErrorCode::IoError,
                format!("解析注册表来源快照失败: {error}"),
            )
        })?;
    let hive = parse_hive(&payload.hive)?;
    let key = RegKey::predef(hive);
    let bytes = hex_decode(&payload.value_hex)?;
    let (subkey, _) = key.create_subkey(&payload.path).map_err(|error| {
        StartupError::new(
            StartupErrorCode::IoError,
            format!("创建注册表键失败: {error}"),
        )
    })?;
    subkey
        .set_raw_value(
            &payload.value_name,
            &RegValue {
                bytes,
                vtype: reg_type_from_u32(payload.value_type),
            },
        )
        .map_err(|error| {
            StartupError::new(
                StartupErrorCode::IoError,
                format!("恢复注册表启动项失败: {error}"),
            )
        })?;

    if let Some(bucket) = payload.approved_bucket.as_deref() {
        startup_approved::restore_state_hex(
            snapshot.item.scope,
            bucket,
            payload
                .approved_value_name
                .as_deref()
                .unwrap_or(&payload.value_name),
            payload.approved_state_hex.as_deref(),
        )?;
    }

    Ok(vec![format!("恢复注册表启动项 {}", payload.value_name)])
}

fn read_config_items(
    config: &RegistrySourceConfig,
    include_raw: bool,
) -> Result<Vec<StartupItem>, StartupError> {
    let hive = RegKey::predef(config.hive);
    let Ok(key) = hive.open_subkey_with_flags(&config.path, KEY_READ) else {
        return Ok(Vec::new());
    };

    let mut items = Vec::new();
    for value in key.enum_values().flatten() {
        let (value_name, reg_value) = value;
        if !matches!(reg_value.vtype, REG_SZ | REG_EXPAND_SZ) {
            continue;
        }
        let command = decode_reg_string(&reg_value)?;
        let mut item = StartupItem::new(
            &value_name,
            config.source,
            config.scope,
            StartupLocator {
                location: format!(r"{}\{}\{}", hive_name(config.hive), config.path, value_name),
                bucket: Some(config.locator_bucket.clone()),
            },
        );
        populate_command_fields(&mut item, &command);
        item.command = Some(command.clone());
        item.requires_admin = matches!(config.scope, StartupScope::Machine);

        if config.source == StartupSource::RegistryRun {
            let approved_value_name = approved_value_name(config, &value_name);
            if let Some(bucket) = config.approved_bucket.as_deref() {
                if let Some(enabled) =
                    startup_approved::read_state(config.scope, bucket, &approved_value_name)?
                {
                    item.state = if enabled {
                        StartupState::Enabled
                    } else {
                        StartupState::Disabled
                    };
                }
            }
        }

        if item.state != StartupState::Disabled && item.target_exists == Some(false) {
            item.state = StartupState::Broken;
            item.warnings.push("目标程序不存在".to_string());
        }

        if include_raw {
            item.raw = Some(serde_json::json!({
                "registry_path": config.path,
                "value_name": value_name,
                "registry_type": format!("{:?}", reg_value.vtype),
            }));
        }

        items.push(item);
    }

    Ok(items)
}

fn read_disabled_policy_items(include_raw: bool) -> Result<Vec<StartupItem>, StartupError> {
    let records = rollback::list_disabled_entries(Some(StartupSource::RegistryPolicyRun), None)?;
    let mut items = Vec::new();

    for record in records {
        let snapshot: StartupSnapshot = serde_json::from_str(&record.snapshot_json).map_err(|error| {
            StartupError::new(
                StartupErrorCode::IoError,
                format!("解析策略启动项禁用快照失败: {error}"),
            )
        })?;
        let mut item = snapshot.item;
        item.state = StartupState::Disabled;
        if !include_raw {
            item.raw = None;
        }
        items.push(item);
    }

    Ok(items)
}

fn snapshot_payload_from_item(item: &StartupItem) -> Result<RegistrySnapshotPayload, StartupError> {
    let config = resolve_config_for_item(item)?;
    let hive = RegKey::predef(config.hive);
    let key = hive.open_subkey_with_flags(&config.path, KEY_READ).map_err(|error| {
        StartupError::new(
            StartupErrorCode::NotFound,
            format!("打开注册表键失败: {error}"),
        )
    })?;
    let approved_value_name = approved_value_name(&config, &item.name);

    let raw_value = key.get_raw_value(&item.name).map_err(|error| {
        StartupError::new(
            StartupErrorCode::NotFound,
            format!("读取注册表值失败: {error}"),
        )
    })?;

    let approved_state_hex = if let Some(bucket) = config.approved_bucket.as_deref() {
        startup_approved::read_state_hex(item.scope, bucket, &approved_value_name)?
    } else {
        None
    };

    Ok(RegistrySnapshotPayload {
        hive: hive_name(config.hive).to_string(),
        path: config.path,
        value_name: item.name.clone(),
        value_hex: hex_encode(&raw_value.bytes),
        value_type: reg_type_to_u32(raw_value.vtype),
        approved_bucket: config.approved_bucket,
        approved_value_name: Some(approved_value_name),
        approved_state_hex,
    })
}

fn resolve_config_for_item(item: &StartupItem) -> Result<RegistrySourceConfig, StartupError> {
    build_configs()
        .into_iter()
        .find(|config| {
            config.source == item.source
                && config.scope == item.scope
                && item.locator.location.starts_with(&format!(r"{}\{}", hive_name(config.hive), config.path))
        })
        .ok_or_else(|| StartupError::new(StartupErrorCode::InvalidSelector, "无法解析注册表来源定位信息"))
}

fn delete_registry_value(key: &RegKey, path: &str, value_name: &str) -> Result<(), StartupError> {
    let subkey = key.open_subkey_with_flags(path, KEY_WRITE).map_err(|error| {
        StartupError::new(
            StartupErrorCode::IoError,
            format!("打开注册表键失败: {error}"),
        )
    })?;
    subkey.delete_value(value_name).map_err(|error| {
        StartupError::new(
            StartupErrorCode::IoError,
            format!("删除注册表值失败: {error}"),
        )
    })?;
    Ok(())
}

fn validate_create_user_run_item(
    config: &RegistrySourceConfig,
    name: &str,
    command: &str,
) -> Result<(), StartupError> {
    if name.trim().is_empty() {
        return Err(StartupError::new(
            StartupErrorCode::InvalidSelector,
            "启动项名称不能为空",
        ));
    }

    if command.trim().is_empty() {
        return Err(StartupError::new(
            StartupErrorCode::InvalidSelector,
            "启动项命令不能为空",
        ));
    }

    let hive = RegKey::predef(config.hive);
    if let Ok(key) = hive.open_subkey_with_flags(&config.path, KEY_READ) {
        if key.get_raw_value(name).is_ok() {
            return Err(StartupError::new(
                StartupErrorCode::Conflict,
                format!("启动项已存在: {name}"),
            ));
        }
    }

    Ok(())
}

fn build_user_run_item(
    config: &RegistrySourceConfig,
    name: &str,
    command: &str,
    include_raw: bool,
) -> StartupItem {
    let mut item = StartupItem::new(
        name,
        config.source,
        config.scope,
        StartupLocator {
            location: format!(r"{}\{}\{}", hive_name(config.hive), config.path, name),
            bucket: Some(config.locator_bucket.clone()),
        },
    );
    populate_command_fields(&mut item, command);
    item.command = Some(command.to_string());
    item.requires_admin = false;

    if item.target_exists == Some(false) {
        item.state = StartupState::Broken;
        item.warnings.push("目标程序不存在".to_string());
    }

    if include_raw {
        item.raw = Some(serde_json::json!({
            "registry_path": config.path,
            "value_name": name,
            "registry_type": "REG_SZ",
        }));
    }

    item
}

fn decode_reg_string(value: &RegValue) -> Result<String, StartupError> {
    if !matches!(value.vtype, REG_SZ | REG_EXPAND_SZ) {
        return Err(StartupError::new(
            StartupErrorCode::Unsupported,
            "当前注册表类型不是字符串",
        ));
    }

    let utf16: Vec<u16> = value
        .bytes
        .chunks(2)
        .filter_map(|chunk| {
            if chunk.len() == 2 {
                Some(u16::from_le_bytes([chunk[0], chunk[1]]))
            } else {
                None
            }
        })
        .take_while(|value| *value != 0)
        .collect();

    String::from_utf16(&utf16).map_err(|error| {
        StartupError::new(
            StartupErrorCode::IoError,
            format!("解析注册表字符串失败: {error}"),
        )
    })
}

fn populate_command_fields(item: &mut StartupItem, command: &str) {
    if let Ok((executable, arguments)) = split_command_for_spawn(command) {
        item.executable_path = Some(executable.clone());
        item.arguments = arguments;
        item.target_exists = if executable.contains('\\') || executable.contains('/') {
            Some(std::path::Path::new(&executable).exists())
        } else {
            None
        };
        item.working_dir = std::path::Path::new(&executable)
            .parent()
            .map(|path| path.to_string_lossy().to_string());
    }
}

fn build_configs() -> Vec<RegistrySourceConfig> {
    vec![
        RegistrySourceConfig {
            source: StartupSource::RegistryRun,
            scope: StartupScope::User,
            hive: HKEY_CURRENT_USER,
            path: std::env::var("RUST_YU_STARTUP_HKCU_RUN_PATH")
                .unwrap_or_else(|_| DEFAULT_RUN_PATH.to_string()),
            locator_bucket: "run".to_string(),
            approved_bucket: Some("Run".to_string()),
        },
        RegistrySourceConfig {
            source: StartupSource::RegistryRunOnce,
            scope: StartupScope::User,
            hive: HKEY_CURRENT_USER,
            path: std::env::var("RUST_YU_STARTUP_HKCU_RUNONCE_PATH")
                .unwrap_or_else(|_| DEFAULT_RUN_ONCE_PATH.to_string()),
            locator_bucket: "run_once".to_string(),
            approved_bucket: None,
        },
        RegistrySourceConfig {
            source: StartupSource::RegistryPolicyRun,
            scope: StartupScope::User,
            hive: HKEY_CURRENT_USER,
            path: std::env::var("RUST_YU_STARTUP_HKCU_POLICY_RUN_PATH")
                .unwrap_or_else(|_| DEFAULT_POLICY_RUN_PATH.to_string()),
            locator_bucket: "policy_run".to_string(),
            approved_bucket: None,
        },
        RegistrySourceConfig {
            source: StartupSource::RegistryRun,
            scope: StartupScope::Machine,
            hive: HKEY_LOCAL_MACHINE,
            path: std::env::var("RUST_YU_STARTUP_HKLM_RUN_PATH")
                .unwrap_or_else(|_| DEFAULT_RUN_PATH.to_string()),
            locator_bucket: "run_machine".to_string(),
            approved_bucket: Some("Run".to_string()),
        },
        RegistrySourceConfig {
            source: StartupSource::RegistryRun,
            scope: StartupScope::Machine,
            hive: HKEY_LOCAL_MACHINE,
            path: std::env::var("RUST_YU_STARTUP_HKLM_RUN32_PATH")
                .unwrap_or_else(|_| DEFAULT_MACHINE_WOW64_RUN_PATH.to_string()),
            locator_bucket: "run_machine32".to_string(),
            approved_bucket: Some("Run32".to_string()),
        },
        RegistrySourceConfig {
            source: StartupSource::RegistryRunOnce,
            scope: StartupScope::Machine,
            hive: HKEY_LOCAL_MACHINE,
            path: std::env::var("RUST_YU_STARTUP_HKLM_RUNONCE_PATH")
                .unwrap_or_else(|_| DEFAULT_RUN_ONCE_PATH.to_string()),
            locator_bucket: "run_once_machine".to_string(),
            approved_bucket: None,
        },
        RegistrySourceConfig {
            source: StartupSource::RegistryRunOnce,
            scope: StartupScope::Machine,
            hive: HKEY_LOCAL_MACHINE,
            path: std::env::var("RUST_YU_STARTUP_HKLM_RUNONCE32_PATH")
                .unwrap_or_else(|_| DEFAULT_MACHINE_WOW64_RUN_ONCE_PATH.to_string()),
            locator_bucket: "run_once_machine32".to_string(),
            approved_bucket: None,
        },
        RegistrySourceConfig {
            source: StartupSource::RegistryPolicyRun,
            scope: StartupScope::Machine,
            hive: HKEY_LOCAL_MACHINE,
            path: std::env::var("RUST_YU_STARTUP_HKLM_POLICY_RUN_PATH")
                .unwrap_or_else(|_| DEFAULT_POLICY_RUN_PATH.to_string()),
            locator_bucket: "policy_run_machine".to_string(),
            approved_bucket: None,
        },
        RegistrySourceConfig {
            source: StartupSource::RegistryPolicyRun,
            scope: StartupScope::Machine,
            hive: HKEY_LOCAL_MACHINE,
            path: std::env::var("RUST_YU_STARTUP_HKLM_POLICY_RUN32_PATH")
                .unwrap_or_else(|_| DEFAULT_MACHINE_WOW64_POLICY_RUN_PATH.to_string()),
            locator_bucket: "policy_run_machine32".to_string(),
            approved_bucket: None,
        },
    ]
}

fn approved_value_name(config: &RegistrySourceConfig, value_name: &str) -> String {
    if config.approved_bucket.as_deref() == Some("Run32") {
        value_name.to_string()
    } else {
        value_name.to_string()
    }
}

fn hive_name(hive: HKEY) -> &'static str {
    if hive == HKEY_CURRENT_USER {
        "HKCU\\"
    } else {
        "HKLM\\"
    }
}

fn parse_hive(value: &str) -> Result<HKEY, StartupError> {
    match value {
        "HKCU\\" => Ok(HKEY_CURRENT_USER),
        "HKLM\\" => Ok(HKEY_LOCAL_MACHINE),
        _ => Err(StartupError::new(
            StartupErrorCode::InvalidSelector,
            format!("未知注册表根键: {value}"),
        )),
    }
}

fn reg_type_to_u32(value: RegType) -> u32 {
    match value {
        REG_SZ => 1,
        REG_EXPAND_SZ => 2,
        _ => 1,
    }
}

fn reg_type_from_u32(value: u32) -> RegType {
    match value {
        2 => REG_EXPAND_SZ,
        _ => REG_SZ,
    }
}

#[cfg(test)]
mod tests {
    use winreg::enums::{HKEY_CURRENT_USER, KEY_READ};
    use winreg::RegKey;

    use super::{collect_items, create_user_run_item};
    use crate::modules::startup::models::{StartupScope, StartupSource, StartupState};
    use crate::modules::startup::TEST_STARTUP_ENV_LOCK;

    #[test]
    fn registry_run_collects_hkcu_items_and_disabled_policy_entries() {
        let _guard = TEST_STARTUP_ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let suffix = uuid::Uuid::new_v4().to_string();
        let run_path = format!(r"Software\rust-yu-test\{}\Run", suffix);
        let policy_path = format!(r"Software\rust-yu-test\{}\PolicyRun", suffix);
        let approved_base = format!(r"Software\rust-yu-test\{}\StartupApproved", suffix);
        std::env::set_var("RUST_YU_STARTUP_HKCU_RUN_PATH", &run_path);
        std::env::set_var("RUST_YU_STARTUP_HKCU_POLICY_RUN_PATH", &policy_path);
        std::env::set_var("RUST_YU_STARTUP_APPROVED_HKCU_BASE", &approved_base);
        std::env::set_var(
            "RUST_YU_STORAGE_DIR",
            std::env::temp_dir().join(format!("rust-yu-registry-test-{suffix}")),
        );

        let hive = RegKey::predef(HKEY_CURRENT_USER);
        let (run_key, _) = hive.create_subkey(&run_path).expect("应能创建测试 run 键");
        run_key
            .set_value("DemoRun", &r#""C:\Windows\System32\notepad.exe""#.to_string())
            .expect("应能写入测试 run 值");
        super::startup_approved::write_state(StartupScope::User, "Run", "DemoRun", false)
            .expect("应能写入禁用状态");

        let (policy_key, _) = hive
            .create_subkey(&policy_path)
            .expect("应能创建测试策略键");
        policy_key
            .set_value("DemoPolicy", &r#""C:\Windows\System32\calc.exe""#.to_string())
            .expect("应能写入策略值");

        let items = collect_items(false).expect("应能枚举注册表启动项");
        let run_item = items
            .iter()
            .find(|item| item.name == "DemoRun")
            .cloned()
            .unwrap_or_else(|| panic!("expected run item"));
        let policy_item = items
            .iter()
            .find(|item| item.name == "DemoPolicy")
            .cloned()
            .unwrap_or_else(|| panic!("expected policy item"));

        assert_eq!(run_item.source, StartupSource::RegistryRun);
        assert_eq!(run_item.state, StartupState::Disabled);
        assert_eq!(policy_item.source, StartupSource::RegistryPolicyRun);

        hive.delete_subkey_all(&format!(r"Software\rust-yu-test\{}", suffix))
            .ok();
        std::env::remove_var("RUST_YU_STARTUP_HKCU_RUN_PATH");
        std::env::remove_var("RUST_YU_STARTUP_HKCU_POLICY_RUN_PATH");
        std::env::remove_var("RUST_YU_STARTUP_APPROVED_HKCU_BASE");
        std::env::remove_var("RUST_YU_STORAGE_DIR");
    }

    #[test]
    fn create_user_run_item_writes_hkcu_run_value() {
        let _guard = TEST_STARTUP_ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let suffix = uuid::Uuid::new_v4().to_string();
        let run_path = format!(r"Software\rust-yu-test\{}\Run", suffix);
        let approved_base = format!(r"Software\rust-yu-test\{}\StartupApproved", suffix);
        std::env::set_var("RUST_YU_STARTUP_HKCU_RUN_PATH", &run_path);
        std::env::set_var("RUST_YU_STARTUP_APPROVED_HKCU_BASE", &approved_base);

        let item = create_user_run_item("DemoAdd", r#""C:\Windows\System32\notepad.exe""#)
            .expect("应能创建 HKCU Run 启动项");

        assert_eq!(item.name, "DemoAdd");
        assert_eq!(item.source, StartupSource::RegistryRun);
        assert_eq!(item.scope, StartupScope::User);

        let hive = RegKey::predef(HKEY_CURRENT_USER);
        let key = hive
            .open_subkey_with_flags(&run_path, KEY_READ)
            .expect("应能打开测试 run 键");
        let value: String = key.get_value("DemoAdd").expect("应能读取新增值");
        assert_eq!(value, r#""C:\Windows\System32\notepad.exe""#);

        hive.delete_subkey_all(&format!(r"Software\rust-yu-test\{}", suffix))
            .ok();
        std::env::remove_var("RUST_YU_STARTUP_HKCU_RUN_PATH");
        std::env::remove_var("RUST_YU_STARTUP_APPROVED_HKCU_BASE");
    }
}
