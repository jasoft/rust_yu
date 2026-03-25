use std::path::{Path, PathBuf};

use super::models::{
    hex_decode, hex_encode, StartupError, StartupErrorCode, StartupItem, StartupLocator,
    StartupScope, StartupSnapshot, StartupSource, StartupState,
};
use super::startup_approved;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct StartupFolderSnapshotPayload {
    file_path: String,
    file_name: String,
    file_hex: String,
    approved_state_hex: Option<String>,
}

pub fn collect_items(include_raw: bool) -> Result<Vec<StartupItem>, StartupError> {
    let mut items = Vec::new();
    for (scope, dir) in startup_directories()? {
        if !dir.exists() {
            continue;
        }

        for entry in std::fs::read_dir(&dir).map_err(|error| {
            StartupError::new(
                StartupErrorCode::IoError,
                format!("读取启动目录失败: {error}"),
            )
        })? {
            let entry = match entry {
                Ok(value) => value,
                Err(_) => continue,
            };
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Some(file_name) = path
                .file_name()
                .map(|value| value.to_string_lossy().to_string())
            else {
                continue;
            };
            let mut item = StartupItem::new(
                path.file_stem()
                    .map(|value| value.to_string_lossy().to_string())
                    .unwrap_or_else(|| file_name.clone()),
                StartupSource::StartupFolder,
                scope,
                StartupLocator {
                    location: path.to_string_lossy().to_string(),
                    bucket: Some("startup_folder".to_string()),
                },
            );
            item.command = Some(path.to_string_lossy().to_string());
            item.executable_path = Some(path.to_string_lossy().to_string());
            item.target_exists = Some(path.exists());
            item.working_dir = path
                .parent()
                .map(|value| value.to_string_lossy().to_string());
            item.requires_admin = matches!(scope, StartupScope::Machine);

            if let Some(enabled) = startup_approved::read_state(scope, "StartupFolder", &file_name)?
            {
                item.state = if enabled {
                    StartupState::Enabled
                } else {
                    StartupState::Disabled
                };
            }

            if item.state != StartupState::Disabled && !path.exists() {
                item.state = StartupState::Broken;
            }

            if include_raw {
                item.raw = Some(serde_json::json!({
                    "file_name": file_name,
                    "directory": dir.to_string_lossy(),
                }));
            }

            items.push(item);
        }
    }

    Ok(items)
}

pub fn capture_snapshot(item: &StartupItem) -> Result<StartupSnapshot, StartupError> {
    let path = PathBuf::from(&item.locator.location);
    let file_name = path
        .file_name()
        .map(|value| value.to_string_lossy().to_string())
        .ok_or_else(|| {
            StartupError::new(StartupErrorCode::InvalidSelector, "缺少启动目录文件名")
        })?;

    let bytes = std::fs::read(&path).map_err(|error| {
        StartupError::new(
            StartupErrorCode::IoError,
            format!("读取启动目录文件失败: {error}"),
        )
    })?;
    let payload = StartupFolderSnapshotPayload {
        file_path: path.to_string_lossy().to_string(),
        file_name: file_name.clone(),
        file_hex: hex_encode(&bytes),
        approved_state_hex: startup_approved::read_state_hex(
            item.scope,
            "StartupFolder",
            &file_name,
        )?,
    };

    Ok(StartupSnapshot {
        item: item.clone(),
        source_payload: serde_json::to_value(payload).map_err(|error| {
            StartupError::new(
                StartupErrorCode::IoError,
                format!("序列化启动目录快照失败: {error}"),
            )
        })?,
    })
}

pub fn apply_action(
    item: &StartupItem,
    action: super::models::StartupAction,
    snapshot: &StartupSnapshot,
) -> Result<Vec<String>, StartupError> {
    let payload: StartupFolderSnapshotPayload =
        serde_json::from_value(snapshot.source_payload.clone()).map_err(|error| {
            StartupError::new(
                StartupErrorCode::IoError,
                format!("解析启动目录快照失败: {error}"),
            )
        })?;
    let path = PathBuf::from(&payload.file_path);

    match action {
        super::models::StartupAction::Enable => {
            startup_approved::write_state(item.scope, "StartupFolder", &payload.file_name, true)?;
            Ok(vec![format!("启用启动目录项 {}", payload.file_name)])
        }
        super::models::StartupAction::Disable => {
            startup_approved::write_state(item.scope, "StartupFolder", &payload.file_name, false)?;
            Ok(vec![format!("禁用启动目录项 {}", payload.file_name)])
        }
        super::models::StartupAction::Delete => {
            std::fs::remove_file(&path).map_err(|error| {
                StartupError::new(
                    StartupErrorCode::IoError,
                    format!("删除启动目录文件失败: {error}"),
                )
            })?;
            startup_approved::restore_state_hex(
                item.scope,
                "StartupFolder",
                &payload.file_name,
                None,
            )?;
            Ok(vec![format!("删除启动目录项 {}", payload.file_name)])
        }
        _ => Err(StartupError::new(
            StartupErrorCode::Unsupported,
            "启动目录项不支持该操作",
        )),
    }
}

pub fn restore_snapshot(snapshot: &StartupSnapshot) -> Result<Vec<String>, StartupError> {
    let payload: StartupFolderSnapshotPayload =
        serde_json::from_value(snapshot.source_payload.clone()).map_err(|error| {
            StartupError::new(
                StartupErrorCode::IoError,
                format!("解析启动目录快照失败: {error}"),
            )
        })?;
    let path = PathBuf::from(&payload.file_path);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| {
            StartupError::new(
                StartupErrorCode::IoError,
                format!("创建启动目录失败: {error}"),
            )
        })?;
    }
    let bytes = hex_decode(&payload.file_hex)?;
    std::fs::write(&path, bytes).map_err(|error| {
        StartupError::new(
            StartupErrorCode::IoError,
            format!("恢复启动目录文件失败: {error}"),
        )
    })?;
    startup_approved::restore_state_hex(
        snapshot.item.scope,
        "StartupFolder",
        &payload.file_name,
        payload.approved_state_hex.as_deref(),
    )?;

    Ok(vec![format!("恢复启动目录项 {}", payload.file_name)])
}

fn startup_directories() -> Result<Vec<(StartupScope, PathBuf)>, StartupError> {
    Ok(vec![
        (
            StartupScope::User,
            startup_dir_for_scope(StartupScope::User)?,
        ),
        (
            StartupScope::Machine,
            startup_dir_for_scope(StartupScope::Machine)?,
        ),
    ])
}

fn startup_dir_for_scope(scope: StartupScope) -> Result<PathBuf, StartupError> {
    let override_var = match scope {
        StartupScope::User => "RUST_YU_STARTUP_FOLDER_USER_DIR",
        StartupScope::Machine => "RUST_YU_STARTUP_FOLDER_COMMON_DIR",
    };
    if let Ok(path) = std::env::var(override_var) {
        return Ok(PathBuf::from(path));
    }

    match scope {
        StartupScope::User => {
            let app_data = std::env::var("APPDATA").map_err(|error| {
                StartupError::new(
                    StartupErrorCode::IoError,
                    format!("读取 APPDATA 失败: {error}"),
                )
            })?;
            Ok(Path::new(&app_data)
                .join("Microsoft")
                .join("Windows")
                .join("Start Menu")
                .join("Programs")
                .join("Startup"))
        }
        StartupScope::Machine => {
            let program_data = std::env::var("ProgramData").map_err(|error| {
                StartupError::new(
                    StartupErrorCode::IoError,
                    format!("读取 ProgramData 失败: {error}"),
                )
            })?;
            Ok(Path::new(&program_data)
                .join("Microsoft")
                .join("Windows")
                .join("Start Menu")
                .join("Programs")
                .join("Startup"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::collect_items;
    use crate::modules::startup::models::{StartupScope, StartupState};
    use crate::modules::startup::TEST_STARTUP_ENV_LOCK;

    #[test]
    fn startup_folder_collects_items_and_respects_disabled_state() {
        let _guard = TEST_STARTUP_ENV_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let suffix = uuid::Uuid::new_v4().to_string();
        let root = std::env::temp_dir().join(format!("rust-yu-startup-folder-{suffix}"));
        std::fs::create_dir_all(&root).expect("应能创建测试启动目录");
        let entry_path = root.join("DemoStartup.cmd");
        std::fs::write(&entry_path, "@echo off\r\n").expect("应能写入测试启动文件");
        std::env::set_var("RUST_YU_STARTUP_FOLDER_USER_DIR", &root);
        std::env::set_var(
            "RUST_YU_STARTUP_APPROVED_HKCU_BASE",
            format!(r"Software\rust-yu-test\{}\StartupApproved", suffix),
        );
        super::startup_approved::write_state(
            StartupScope::User,
            "StartupFolder",
            "DemoStartup.cmd",
            false,
        )
        .expect("应能写入禁用状态");

        let items = collect_items(false).expect("应能枚举启动目录");
        let item = items
            .iter()
            .find(|value| value.name == "DemoStartup")
            .cloned()
            .unwrap_or_else(|| panic!("expected startup folder item"));
        assert_eq!(item.state, StartupState::Disabled);

        std::fs::remove_dir_all(&root).ok();
        std::env::remove_var("RUST_YU_STARTUP_FOLDER_USER_DIR");
        std::env::remove_var("RUST_YU_STARTUP_APPROVED_HKCU_BASE");
    }
}
