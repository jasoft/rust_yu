use crate::elevation::{
    create_or_repair_current_user_task, current_token_state, inspect_current_user_task,
    run_current_user_task, validate_protected_executable, validate_protected_install_path,
    ElevationError, ElevationErrorCode, TokenState,
};
use crate::single_instance::SingleInstanceGuard;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InternalArgs {
    Normal,
    ElevatedEntry { repair_launch_task: bool },
    RemoveLaunchTasks,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskPresence {
    Missing,
    Valid,
    Invalid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootstrapAction {
    StartDebugGui,
    StartElevatedGui,
    RunExistingTaskAndExit,
    RelaunchWithUacAndExit,
    Reject(ElevationErrorCode),
}

pub fn parse_internal_args(args: &[String]) -> Result<InternalArgs, ElevationError> {
    let mut elevated = false;
    let mut repair = false;
    let mut remove_tasks = false;
    for arg in args.iter().skip(1) {
        match arg.as_str() {
            "--elevated-entry" => elevated = true,
            "--repair-launch-task" if elevated => repair = true,
            "--remove-launch-tasks" => remove_tasks = true,
            _ => {
                return Err(ElevationError::new(
                    ElevationErrorCode::ElevationLaunchFailed,
                    "Rust Yu 只接受固定的内部启动参数",
                ))
            }
        }
    }
    if remove_tasks && elevated {
        return Err(ElevationError::new(
            ElevationErrorCode::ElevationLaunchFailed,
            "维护模式参数不能与管理员入口参数同时使用",
        ));
    }
    Ok(if remove_tasks {
        InternalArgs::RemoveLaunchTasks
    } else if elevated {
        InternalArgs::ElevatedEntry {
            repair_launch_task: repair,
        }
    } else {
        InternalArgs::Normal
    })
}

pub fn decide_bootstrap(
    debug_build: bool,
    args: InternalArgs,
    token: TokenState,
    install_path_safe: bool,
    task: TaskPresence,
) -> BootstrapAction {
    if matches!(args, InternalArgs::ElevatedEntry { .. }) && !token.is_elevated {
        return BootstrapAction::Reject(ElevationErrorCode::UnsupportedStandardUser);
    }
    if debug_build {
        return BootstrapAction::StartDebugGui;
    }
    if !token.is_administrator {
        return BootstrapAction::Reject(ElevationErrorCode::UnsupportedStandardUser);
    }
    if !install_path_safe {
        return BootstrapAction::Reject(ElevationErrorCode::UnsafeInstallLocation);
    }
    if token.is_elevated {
        return BootstrapAction::StartElevatedGui;
    }
    match task {
        TaskPresence::Valid => BootstrapAction::RunExistingTaskAndExit,
        TaskPresence::Missing | TaskPresence::Invalid => BootstrapAction::RelaunchWithUacAndExit,
    }
}

pub fn run_startup_bootstrap() -> bool {
    let args = std::env::args().collect::<Vec<_>>();
    let parsed = match parse_internal_args(&args) {
        Ok(parsed) => parsed,
        Err(error) => return show_startup_error(error),
    };
    let token = match current_token_state() {
        Ok(token) => token,
        Err(error) => {
            return show_startup_error(ElevationError::new(
                ElevationErrorCode::ElevationLaunchFailed,
                error,
            ))
        }
    };
    if matches!(parsed, InternalArgs::RemoveLaunchTasks) {
        if !token.is_elevated {
            return show_startup_error(ElevationError::new(
                ElevationErrorCode::UnsupportedStandardUser,
                "删除 Rust Yu 计划任务需要管理员权限",
            ));
        }
        return match crate::elevation::remove_all_product_tasks() {
            Ok(()) => false,
            Err(error) => show_startup_error(error),
        };
    }
    let debug_build = cfg!(debug_assertions);
    if debug_build && !token.is_elevated {
        return true;
    }

    let executable = match std::env::current_exe() {
        Ok(path) => path,
        Err(error) => {
            return show_startup_error(ElevationError::new(
                ElevationErrorCode::UnsafeInstallLocation,
                error.to_string(),
            ))
        }
    };
    let install_path_safe = executable
        .parent()
        .map(validate_protected_install_path)
        .is_some_and(|result| result.is_ok());
    let task = if install_path_safe {
        match inspect_current_user_task() {
            Ok(Some(_)) => TaskPresence::Valid,
            Ok(None) => TaskPresence::Missing,
            Err(_) => TaskPresence::Invalid,
        }
    } else {
        TaskPresence::Invalid
    };
    let action = decide_bootstrap(debug_build, parsed, token, install_path_safe, task);
    match action {
        BootstrapAction::StartDebugGui => true,
        BootstrapAction::StartElevatedGui => {
            if let Err(error) = validate_protected_executable(&executable)
                .and_then(|_| create_or_repair_current_user_task(&executable).map(|_| ()))
            {
                return show_startup_error(error);
            }
            match SingleInstanceGuard::acquire() {
                Ok(Some(guard)) => {
                    // The named mutex must live for the lifetime of the GUI process.
                    let _ = Box::leak(Box::new(guard));
                    true
                }
                Ok(None) => false,
                Err(error) => show_startup_error(ElevationError::new(
                    ElevationErrorCode::ElevationLaunchFailed,
                    error,
                )),
            }
        }
        BootstrapAction::RunExistingTaskAndExit => match run_current_user_task() {
            Ok(()) => false,
            Err(error) => show_startup_error(error),
        },
        BootstrapAction::RelaunchWithUacAndExit => match launch_with_runas(&executable) {
            Ok(()) => false,
            Err(error) => show_startup_error(error),
        },
        BootstrapAction::Reject(code) => show_startup_error(ElevationError::new(
            code,
            "当前环境不满足管理员 GUI 启动要求",
        )),
    }
}

#[cfg(windows)]
fn launch_with_runas(executable: &PathBuf) -> Result<(), ElevationError> {
    use std::os::windows::ffi::OsStrExt;
    use windows::core::PCWSTR;
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let operation = windows_string("runas");
    let file = executable
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let parameters = windows_string("--elevated-entry --repair-launch-task");
    let result = unsafe {
        ShellExecuteW(
            None,
            PCWSTR(operation.as_ptr()),
            PCWSTR(file.as_ptr()),
            PCWSTR(parameters.as_ptr()),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    if result.0 as usize <= 32 {
        return Err(ElevationError::new(
            ElevationErrorCode::ElevationLaunchFailed,
            "UAC 启动管理员 GUI 失败或被取消",
        ));
    }
    Ok(())
}

#[cfg(not(windows))]
fn launch_with_runas(_executable: &PathBuf) -> Result<(), ElevationError> {
    Err(ElevationError::new(
        ElevationErrorCode::ElevationLaunchFailed,
        "当前平台不支持 UAC 启动",
    ))
}

#[cfg(windows)]
fn windows_string(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

fn show_startup_error(error: ElevationError) -> bool {
    tracing::error!(code = error.code.as_str(), message = %error.message, "GUI bootstrap failed");
    #[cfg(windows)]
    {
        use windows::core::PCWSTR;
        use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};
        let title = windows_string("Rust Yu");
        let message = windows_string(&format!(
            "{}\n错误代码: {}",
            error.message,
            error.code.as_str()
        ));
        unsafe {
            let _ = MessageBoxW(
                None,
                PCWSTR(message.as_ptr()),
                PCWSTR(title.as_ptr()),
                MB_OK | MB_ICONERROR,
            );
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::{
        decide_bootstrap, parse_internal_args, BootstrapAction, InternalArgs, TaskPresence,
    };
    use crate::elevation::{ElevationErrorCode, TokenState};

    fn admin(elevated: bool) -> TokenState {
        TokenState {
            is_elevated: elevated,
            is_administrator: true,
            is_split_token: !elevated,
        }
    }

    #[test]
    fn startup_decision_table_is_stable() {
        assert_eq!(
            decide_bootstrap(
                true,
                InternalArgs::Normal,
                admin(false),
                false,
                TaskPresence::Missing
            ),
            BootstrapAction::StartDebugGui
        );
        assert_eq!(
            decide_bootstrap(
                false,
                InternalArgs::Normal,
                admin(true),
                true,
                TaskPresence::Valid
            ),
            BootstrapAction::StartElevatedGui
        );
        assert_eq!(
            decide_bootstrap(
                false,
                InternalArgs::Normal,
                admin(false),
                true,
                TaskPresence::Valid
            ),
            BootstrapAction::RunExistingTaskAndExit
        );
        assert_eq!(
            decide_bootstrap(
                false,
                InternalArgs::Normal,
                admin(false),
                true,
                TaskPresence::Missing
            ),
            BootstrapAction::RelaunchWithUacAndExit
        );
        assert_eq!(
            decide_bootstrap(
                false,
                InternalArgs::Normal,
                TokenState {
                    is_elevated: false,
                    is_administrator: false,
                    is_split_token: false
                },
                true,
                TaskPresence::Missing
            ),
            BootstrapAction::Reject(ElevationErrorCode::UnsupportedStandardUser)
        );
        assert_eq!(
            decide_bootstrap(
                false,
                InternalArgs::ElevatedEntry {
                    repair_launch_task: false
                },
                admin(false),
                true,
                TaskPresence::Valid
            ),
            BootstrapAction::Reject(ElevationErrorCode::UnsupportedStandardUser)
        );
    }

    #[test]
    fn only_fixed_internal_arguments_are_accepted() {
        let args = vec![
            "RustYu.exe".to_string(),
            "--elevated-entry".to_string(),
            "--repair-launch-task".to_string(),
        ];
        assert_eq!(
            parse_internal_args(&args).expect("fixed args should parse"),
            InternalArgs::ElevatedEntry {
                repair_launch_task: true
            }
        );
        assert!(parse_internal_args(&["RustYu.exe".to_string(), "--clean".to_string()]).is_err());
    }
}
