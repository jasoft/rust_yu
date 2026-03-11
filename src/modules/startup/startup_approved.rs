use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
use winreg::{RegKey, RegValue};

use super::models::{hex_decode, hex_encode, StartupError, StartupErrorCode, StartupScope};

const DEFAULT_APPROVED_BASE_PATH: &str =
    r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved";

fn base_path_for_scope(scope: StartupScope) -> String {
    let env_name = match scope {
        StartupScope::User => "RUST_YU_STARTUP_APPROVED_HKCU_BASE",
        StartupScope::Machine => "RUST_YU_STARTUP_APPROVED_HKLM_BASE",
    };

    std::env::var(env_name).unwrap_or_else(|_| DEFAULT_APPROVED_BASE_PATH.to_string())
}

fn open_hive(scope: StartupScope) -> RegKey {
    let hive = match scope {
        StartupScope::User => HKEY_CURRENT_USER,
        StartupScope::Machine => HKEY_LOCAL_MACHINE,
    };
    RegKey::predef(hive)
}

pub fn read_state(scope: StartupScope, bucket: &str, value_name: &str) -> Result<Option<bool>, StartupError> {
    let hive = open_hive(scope);
    let path = format!(r"{}\{}", base_path_for_scope(scope), bucket);
    let Ok(key) = hive.open_subkey(&path) else {
        return Ok(None);
    };
    let Ok(value) = key.get_raw_value(value_name) else {
        return Ok(None);
    };

    if value.bytes.is_empty() {
        return Ok(None);
    }

    Ok(Some(value.bytes[0] != 3))
}

pub fn read_state_hex(
    scope: StartupScope,
    bucket: &str,
    value_name: &str,
) -> Result<Option<String>, StartupError> {
    let hive = open_hive(scope);
    let path = format!(r"{}\{}", base_path_for_scope(scope), bucket);
    let Ok(key) = hive.open_subkey(&path) else {
        return Ok(None);
    };
    let Ok(value) = key.get_raw_value(value_name) else {
        return Ok(None);
    };
    Ok(Some(hex_encode(&value.bytes)))
}

pub fn write_state(
    scope: StartupScope,
    bucket: &str,
    value_name: &str,
    enabled: bool,
) -> Result<(), StartupError> {
    let hive = open_hive(scope);
    let path = format!(r"{}\{}", base_path_for_scope(scope), bucket);
    let (key, _) = hive.create_subkey(&path).map_err(|error| {
        StartupError::new(
            StartupErrorCode::IoError,
            format!("创建 StartupApproved 键失败: {error}"),
        )
    })?;

    let mut bytes = key
        .get_raw_value(value_name)
        .map(|value| value.bytes)
        .unwrap_or_else(|_| vec![0; 12]);
    if bytes.is_empty() {
        bytes = vec![0; 12];
    }
    bytes[0] = if enabled { 2 } else { 3 };

    key.set_raw_value(
        value_name,
        &RegValue {
            bytes,
            vtype: winreg::enums::RegType::REG_BINARY,
        },
    )
    .map_err(|error| {
        StartupError::new(
            StartupErrorCode::IoError,
            format!("写入 StartupApproved 状态失败: {error}"),
        )
    })?;

    Ok(())
}

pub fn restore_state_hex(
    scope: StartupScope,
    bucket: &str,
    value_name: &str,
    state_hex: Option<&str>,
) -> Result<(), StartupError> {
    let hive = open_hive(scope);
    let path = format!(r"{}\{}", base_path_for_scope(scope), bucket);
    let (key, _) = hive.create_subkey(&path).map_err(|error| {
        StartupError::new(
            StartupErrorCode::IoError,
            format!("创建 StartupApproved 键失败: {error}"),
        )
    })?;

    if let Some(hex) = state_hex {
        let bytes = hex_decode(hex)?;
        key.set_raw_value(
            value_name,
            &RegValue {
                bytes,
                vtype: winreg::enums::RegType::REG_BINARY,
            },
        )
        .map_err(|error| {
            StartupError::new(
                StartupErrorCode::IoError,
                format!("恢复 StartupApproved 状态失败: {error}"),
            )
        })?;
    } else {
        key.delete_value(value_name).ok();
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{read_state, write_state};
    use crate::modules::startup::models::StartupScope;
    use crate::modules::startup::TEST_STARTUP_ENV_LOCK;

    #[test]
    fn startup_approved_roundtrip_updates_enabled_flag() {
        let _guard = TEST_STARTUP_ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let base_path = format!(r"Software\rust-yu-test\StartupApproved\{}", uuid::Uuid::new_v4());
        std::env::set_var("RUST_YU_STARTUP_APPROVED_HKCU_BASE", &base_path);

        write_state(StartupScope::User, "Run", "Demo", false).ok();
        let disabled = read_state(StartupScope::User, "Run", "Demo").unwrap_or(None);
        write_state(StartupScope::User, "Run", "Demo", true).ok();
        let enabled = read_state(StartupScope::User, "Run", "Demo").unwrap_or(None);

        assert_eq!(disabled, Some(false));
        assert_eq!(enabled, Some(true));

        let hive = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER);
        hive.delete_subkey_all(&base_path).ok();
        std::env::remove_var("RUST_YU_STARTUP_APPROVED_HKCU_BASE");
    }
}
