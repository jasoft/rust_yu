use rust_yu_lib::application::force_uninstall::{
    clean_force_uninstall as run_clean_force_uninstall,
    plan_force_uninstall as run_plan_force_uninstall, ForceCleanupSelection, ForceUninstallPlan,
    ForceUninstallResult,
};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

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

#[derive(Debug, Clone, Serialize)]
pub struct ContextMenuStatus {
    pub enabled: bool,
    pub command: Option<String>,
}

#[tauri::command]
pub fn get_force_uninstall_startup_target() -> Result<Option<String>, CommandError> {
    let args = std::env::args().collect::<Vec<_>>();
    let target = match crate::bootstrap::parse_internal_args(&args) {
        Ok(crate::bootstrap::InternalArgs::ForceUninstall { path, .. }) => Some(path),
        Ok(crate::bootstrap::InternalArgs::ElevatedEntry { force_target, .. }) => force_target,
        _ => None,
    };
    Ok(target)
}

#[tauri::command]
pub fn get_force_uninstall_context_menu() -> Result<ContextMenuStatus, CommandError> {
    context_menu_status().map_err(CommandError::from)
}

#[tauri::command]
pub fn set_force_uninstall_context_menu(enabled: bool) -> Result<ContextMenuStatus, CommandError> {
    set_context_menu(enabled).map_err(CommandError::from)
}

#[tauri::command]
pub async fn capture_hunter_target(timeout_secs: Option<u64>) -> Result<String, CommandError> {
    let timeout_secs = timeout_secs.unwrap_or(15).clamp(3, 30);
    tokio::task::spawn_blocking(move || capture_foreground_process(timeout_secs))
        .await
        .map_err(|error| CommandError::new(format!("猎手模式任务失败: {error}")))?
        .map_err(CommandError::from)
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

fn other_error(
    message: impl Into<String>,
) -> rust_yu_lib::modules::common::error::UninstallerError {
    rust_yu_lib::modules::common::error::UninstallerError::Other(message.into())
}

fn context_menu_status(
) -> Result<ContextMenuStatus, rust_yu_lib::modules::common::error::UninstallerError> {
    #[cfg(windows)]
    {
        use winreg::enums::HKEY_CURRENT_USER;
        use winreg::RegKey;

        let classes = RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey("Software\\Classes")
            .map_err(|error| other_error(error.to_string()))?;
        for key_path in CONTEXT_MENU_KEY_PATHS {
            let command_path = format!("{key_path}\\command");
            if let Some(command) = classes
                .open_subkey(&command_path)
                .ok()
                .and_then(|key: winreg::RegKey| key.get_value::<String, _>("").ok())
            {
                return Ok(ContextMenuStatus {
                    enabled: true,
                    command: Some(command),
                });
            }
        }
        return Ok(ContextMenuStatus {
            enabled: false,
            command: None,
        });
    }
    #[cfg(not(windows))]
    {
        Ok(ContextMenuStatus {
            enabled: false,
            command: None,
        })
    }
}

fn set_context_menu(
    enabled: bool,
) -> Result<ContextMenuStatus, rust_yu_lib::modules::common::error::UninstallerError> {
    #[cfg(windows)]
    {
        use winreg::enums::HKEY_CURRENT_USER;
        use winreg::RegKey;

        let classes = RegKey::predef(HKEY_CURRENT_USER)
            .open_subkey_with_flags(
                "Software\\Classes",
                winreg::enums::KEY_WRITE | winreg::enums::KEY_READ,
            )
            .map_err(|error| other_error(error.to_string()))?;
        if !enabled {
            for key_path in CONTEXT_MENU_KEY_PATHS {
                let _ = classes.delete_subkey_all(key_path);
            }
            return Ok(ContextMenuStatus {
                enabled: false,
                command: None,
            });
        }
        let executable = std::env::current_exe()
            .map_err(|error| other_error(format!("无法定位 Rust Yu 可执行文件: {error}")))?;
        let executable = executable
            .canonicalize()
            .unwrap_or(executable)
            .to_string_lossy()
            .replace('"', "\\\"");
        let command = format!("\"{executable}\" --force-uninstall \"%1\"");
        for key_path in CONTEXT_MENU_KEY_PATHS {
            let (key, _) = classes
                .create_subkey(key_path)
                .map_err(|error| other_error(error.to_string()))?;
            key.set_value("", &"使用 Rust Yu 强制卸载")
                .map_err(|error| other_error(error.to_string()))?;
            key.set_value("Icon", &executable)
                .map_err(|error| other_error(error.to_string()))?;
            let (command_key, _) = key
                .create_subkey("command")
                .map_err(|error| other_error(error.to_string()))?;
            command_key
                .set_value("", &command)
                .map_err(|error| other_error(error.to_string()))?;
        }
        return Ok(ContextMenuStatus {
            enabled: true,
            command: Some(command),
        });
    }
    #[cfg(not(windows))]
    {
        let _ = enabled;
        Err(other_error("当前平台不支持 Windows 右键菜单"))
    }
}

#[cfg(windows)]
const CONTEXT_MENU_KEY_PATHS: [&str; 2] = [
    "*\\shell\\RustYuForceUninstall",
    "Directory\\shell\\RustYuForceUninstall",
];

#[cfg(windows)]
fn capture_foreground_process(
    timeout_secs: u64,
) -> Result<String, rust_yu_lib::modules::common::error::UninstallerError> {
    use std::os::windows::ffi::OsStringExt;
    use windows::core::PWSTR;
    use windows::Win32::Foundation::CloseHandle;
    use windows::Win32::System::Threading::{
        OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        PROCESS_QUERY_LIMITED_INFORMATION,
    };
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

    let own_pid = std::process::id();
    let deadline = Instant::now() + Duration::from_secs(timeout_secs);
    while Instant::now() < deadline {
        let window = unsafe { GetForegroundWindow() };
        if !window.is_invalid() {
            let mut process_id = 0_u32;
            unsafe { GetWindowThreadProcessId(window, Some(&mut process_id as *mut u32)) };
            if process_id != 0 && process_id != own_pid {
                let process =
                    unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id) };
                if let Ok(process) = process {
                    let mut buffer = vec![0_u16; 32_768];
                    let mut length = buffer.len() as u32;
                    let result = unsafe {
                        QueryFullProcessImageNameW(
                            process,
                            PROCESS_NAME_WIN32,
                            PWSTR(buffer.as_mut_ptr()),
                            &mut length,
                        )
                    };
                    unsafe {
                        let _ = CloseHandle(process);
                    }
                    if result.is_ok() && length > 0 {
                        return Ok(std::ffi::OsString::from_wide(&buffer[..length as usize])
                            .to_string_lossy()
                            .to_string());
                    }
                }
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Err(other_error(
        "猎手模式在限定时间内没有捕获到其他窗口，请重试并先点击目标程序窗口",
    ))
}

#[cfg(not(windows))]
fn capture_foreground_process(
    _timeout_secs: u64,
) -> Result<String, rust_yu_lib::modules::common::error::UninstallerError> {
    Err(other_error("当前平台不支持窗口猎手模式"))
}
