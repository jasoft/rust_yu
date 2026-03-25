use crate::modules::common::error::UninstallerError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UninstallExitClassification {
    pub successful: bool,
    pub user_cancelled: bool,
    pub reboot_required: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UninstallCompletionStatus {
    Completed,
    InterruptedByUser,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UninstallRunResult {
    pub completion_status: UninstallCompletionStatus,
    pub exit_code: Option<u32>,
    pub classification: UninstallExitClassification,
    pub likely_interactive: bool,
    pub used_job_object: bool,
}

pub fn is_likely_interactive_uninstall(command: &str) -> bool {
    let lower = command.trim().to_lowercase();
    let tokens = lower.split_whitespace().map(str::trim).collect::<Vec<_>>();

    if lower.starts_with("msiexec") {
        return !tokens
            .iter()
            .any(|token| matches!(*token, "/quiet" | "/qn" | "/qb!" | "/qb-"));
    }

    !tokens.iter().any(|token| {
        matches!(
            *token,
            "/s" | "/silent"
                | "/verysilent"
                | "/quiet"
                | "/qn"
                | "/qs"
                | "/quietuninstall"
                | "-s"
                | "-silent"
        )
    })
}

pub fn classify_uninstall_exit_code(
    command: &str,
    exit_code: Option<u32>,
) -> UninstallExitClassification {
    let is_msi = command.trim().to_lowercase().starts_with("msiexec");

    match exit_code {
        None => UninstallExitClassification {
            successful: false,
            user_cancelled: false,
            reboot_required: false,
        },
        Some(0) => UninstallExitClassification {
            successful: true,
            user_cancelled: false,
            reboot_required: false,
        },
        Some(1641 | 3010) => UninstallExitClassification {
            successful: true,
            user_cancelled: false,
            reboot_required: true,
        },
        Some(1602 | 1223) if is_msi || exit_code == Some(1223) => UninstallExitClassification {
            successful: false,
            user_cancelled: true,
            reboot_required: false,
        },
        Some(_) => UninstallExitClassification {
            successful: false,
            user_cancelled: false,
            reboot_required: false,
        },
    }
}

pub fn build_interactive_uninstall_message(program_name: &str) -> String {
    format!(
        "  - 检测到 {} 可能会弹出卸载窗口，请在图形界面完成确认；命令行将等待卸载结束并继续核验实际移除结果",
        program_name
    )
}

pub fn build_unsuccessful_uninstall_message(
    program_name: &str,
    still_registered: bool,
    install_dir_exists: bool,
    exit_code: Option<u32>,
    user_cancelled: bool,
    interrupted: bool,
) -> String {
    let status = if interrupted {
        "用户中断了命令行等待，程序状态未最终确认"
    } else if user_cancelled {
        "卸载已取消"
    } else {
        "卸载失败"
    };

    format!(
        "{}: name={}, registered={}, install_dir_exists={}, exit_code={}",
        status,
        program_name,
        still_registered,
        install_dir_exists,
        exit_code
            .map(|code| code.to_string())
            .unwrap_or_else(|| "unknown".to_string())
    )
}

pub async fn run_uninstall_command(
    command: &str,
    timeout_secs: u64,
) -> Result<UninstallRunResult, UninstallerError> {
    let likely_interactive = is_likely_interactive_uninstall(command);

    #[cfg(windows)]
    {
        use std::sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        };

        let command_owned = command.trim().to_string();
        let cancel_requested = Arc::new(AtomicBool::new(false));
        let cancel_for_worker = Arc::clone(&cancel_requested);

        let task = tokio::task::spawn_blocking(move || {
            run_uninstall_command_windows(&command_owned, timeout_secs, cancel_for_worker.as_ref())
        });
        tokio::pin!(task);

        let mut result = tokio::select! {
            join_result = &mut task => join_result
                .map_err(|error| UninstallerError::Other(format!("等待卸载线程失败: {error}")))??,
            signal = tokio::signal::ctrl_c() => {
                signal.map_err(|error| UninstallerError::Other(format!("监听 Ctrl+C 失败: {error}")))?;
                cancel_requested.store(true, Ordering::SeqCst);
                task.await
                    .map_err(|error| UninstallerError::Other(format!("等待卸载线程失败: {error}")))??
            }
        };

        result.likely_interactive = likely_interactive;
        return Ok(result);
    }

    #[cfg(not(windows))]
    {
        use std::process::Command;

        let output = Command::new("cmd")
            .args(["/C", command])
            .output()
            .map_err(UninstallerError::FileSystem)?;
        let exit_code = output.status.code().map(|code| code as u32);

        return Ok(UninstallRunResult {
            completion_status: UninstallCompletionStatus::Completed,
            exit_code,
            classification: classify_uninstall_exit_code(command, exit_code),
            likely_interactive,
            used_job_object: false,
        });
    }
}

#[cfg(windows)]
fn run_uninstall_command_windows(
    command: &str,
    timeout_secs: u64,
    cancel_requested: &std::sync::atomic::AtomicBool,
) -> Result<UninstallRunResult, UninstallerError> {
    use std::{
        ffi::OsStr,
        mem::size_of,
        os::windows::ffi::OsStrExt,
        path::PathBuf,
        time::{Duration, Instant},
    };
    use windows::{
        core::{PCWSTR, PWSTR},
        Win32::{
            Foundation::{CloseHandle, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT},
            System::{
                JobObjects::{
                    AssignProcessToJobObject, CreateJobObjectW,
                    JobObjectBasicAccountingInformation, QueryInformationJobObject,
                    JOBOBJECT_BASIC_ACCOUNTING_INFORMATION,
                },
                Threading::{
                    CreateProcessW, GetExitCodeProcess, ResumeThread, WaitForSingleObject,
                    CREATE_BREAKAWAY_FROM_JOB, CREATE_SUSPENDED, PROCESS_INFORMATION, STARTUPINFOW,
                },
            },
        },
    };

    struct HandleGuard(HANDLE);

    impl HandleGuard {
        fn new(handle: HANDLE) -> Self {
            Self(handle)
        }

        fn raw(&self) -> HANDLE {
            self.0
        }
    }

    impl Drop for HandleGuard {
        fn drop(&mut self) {
            if !self.0.is_invalid() {
                unsafe {
                    let _ = CloseHandle(self.0);
                }
            }
        }
    }

    struct ProcessLaunch {
        process: HandleGuard,
        thread: HandleGuard,
    }

    fn to_wide(value: &str) -> Vec<u16> {
        OsStr::new(value)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    fn infer_current_directory(command: &str) -> Option<Vec<u16>> {
        let (executable, _) =
            crate::modules::common::utils::split_command_for_spawn(command).ok()?;
        let executable_path = PathBuf::from(executable);
        let working_dir = executable_path.parent().filter(|path| path.is_dir())?;
        Some(to_wide(&working_dir.to_string_lossy()))
    }

    fn try_create_process(command: &str) -> Result<ProcessLaunch, windows::core::Error> {
        let mut command_line = to_wide(command);
        let current_directory = infer_current_directory(command);
        let current_directory_ptr = current_directory
            .as_ref()
            .map(|dir| PCWSTR(dir.as_ptr()))
            .unwrap_or(PCWSTR::null());
        let mut startup_info = STARTUPINFOW::default();
        startup_info.cb = size_of::<STARTUPINFOW>() as u32;
        let mut process_info = PROCESS_INFORMATION::default();

        unsafe {
            CreateProcessW(
                PCWSTR::null(),
                Some(PWSTR(command_line.as_mut_ptr())),
                None,
                None,
                false,
                CREATE_SUSPENDED | CREATE_BREAKAWAY_FROM_JOB,
                None,
                current_directory_ptr,
                &startup_info,
                &mut process_info,
            )?;
        }

        Ok(ProcessLaunch {
            process: HandleGuard::new(process_info.hProcess),
            thread: HandleGuard::new(process_info.hThread),
        })
    }

    fn create_process_with_fallback(command: &str) -> Result<ProcessLaunch, UninstallerError> {
        let mut attempts = vec![command.trim().to_string()];
        if !command.trim().to_lowercase().starts_with("cmd /c") {
            attempts.push(format!("cmd /C {}", command.trim()));
        }

        let mut last_error = None;
        for candidate in attempts {
            match try_create_process(&candidate) {
                Ok(launch) => return Ok(launch),
                Err(error) => last_error = Some(error),
            }
        }

        Err(UninstallerError::Other(format!(
            "无法启动卸载进程: {}",
            last_error
                .map(|error| error.to_string())
                .unwrap_or_else(|| "unknown".to_string())
        )))
    }

    let launch = create_process_with_fallback(command)?;
    let process_handle = launch.process;
    let thread_handle = launch.thread;

    let mut used_job_object = false;
    let mut job_handle = None;

    // 先挂起创建进程，再尝试加入 Job，避免父进程过早退出后子进程逃逸。
    match unsafe { CreateJobObjectW(None, None) } {
        Ok(handle) => {
            let guard = HandleGuard::new(handle);
            let assigned =
                unsafe { AssignProcessToJobObject(guard.raw(), process_handle.raw()) }.is_ok();
            if assigned {
                used_job_object = true;
                job_handle = Some(guard);
            } else {
                tracing::warn!("AssignProcessToJobObject 失败，回退为等待直接进程");
            }
        }
        Err(error) => {
            tracing::warn!("CreateJobObjectW 失败，回退为等待直接进程: {}", error);
        }
    }

    let resume_result = unsafe { ResumeThread(thread_handle.raw()) };
    if resume_result == u32::MAX {
        return Err(UninstallerError::Other(
            "无法恢复卸载进程主线程".to_string(),
        ));
    }

    let started_at = Instant::now();
    loop {
        if cancel_requested.load(std::sync::atomic::Ordering::SeqCst) {
            return Ok(UninstallRunResult {
                completion_status: UninstallCompletionStatus::InterruptedByUser,
                exit_code: None,
                classification: classify_uninstall_exit_code(command, None),
                likely_interactive: false,
                used_job_object,
            });
        }

        if started_at.elapsed() >= Duration::from_secs(timeout_secs) {
            return Err(UninstallerError::Timeout(format!(
                "等待卸载进程链结束超时（{} 秒）",
                timeout_secs
            )));
        }

        if let Some(job) = &job_handle {
            let mut info = JOBOBJECT_BASIC_ACCOUNTING_INFORMATION::default();
            unsafe {
                QueryInformationJobObject(
                    Some(job.raw()),
                    JobObjectBasicAccountingInformation,
                    (&mut info as *mut JOBOBJECT_BASIC_ACCOUNTING_INFORMATION).cast(),
                    size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>() as u32,
                    None,
                )
                .map_err(|error| UninstallerError::Other(format!("读取 Job 状态失败: {error}")))?;
            }

            if info.ActiveProcesses == 0 {
                break;
            }

            std::thread::sleep(Duration::from_millis(250));
            continue;
        }

        let wait_result = unsafe { WaitForSingleObject(process_handle.raw(), 250) };
        if wait_result == WAIT_OBJECT_0 {
            break;
        }
        if wait_result != WAIT_TIMEOUT {
            return Err(UninstallerError::Other(format!(
                "等待卸载进程失败: {:?}",
                wait_result
            )));
        }
    }

    let mut exit_code = 0u32;
    unsafe {
        GetExitCodeProcess(process_handle.raw(), &mut exit_code)
            .map_err(|error| UninstallerError::Other(format!("读取卸载进程退出码失败: {error}")))?;
    }

    Ok(UninstallRunResult {
        completion_status: UninstallCompletionStatus::Completed,
        exit_code: Some(exit_code),
        classification: classify_uninstall_exit_code(command, Some(exit_code)),
        likely_interactive: false,
        used_job_object,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        build_unsuccessful_uninstall_message, classify_uninstall_exit_code,
        is_likely_interactive_uninstall,
    };

    #[test]
    fn detects_interactive_exe_uninstall_without_silent_flags() {
        assert!(is_likely_interactive_uninstall(
            r#""C:\Program Files\App\uninstall.exe""#
        ));
    }

    #[test]
    fn detects_silent_exe_uninstall_switches() {
        assert!(!is_likely_interactive_uninstall(
            r#""C:\Program Files\App\uninstall.exe" /S"#
        ));
    }

    #[test]
    fn detects_cancelled_msi_exit_code() {
        let result = classify_uninstall_exit_code("msiexec /x {GUID}", Some(1602));

        assert!(result.user_cancelled);
        assert!(!result.successful);
    }

    #[test]
    fn treats_reboot_required_exit_code_as_success() {
        let result = classify_uninstall_exit_code("msiexec /x {GUID}", Some(3010));

        assert!(result.successful);
        assert!(result.reboot_required);
    }

    #[test]
    fn builds_cancelled_message_when_program_still_present() {
        let message =
            build_unsuccessful_uninstall_message("Demo App", true, true, Some(1602), true, false);

        assert!(message.contains("卸载已取消"));
        assert!(message.contains("registered=true"));
    }

    #[test]
    fn builds_interrupted_message_when_user_stops_waiting() {
        let message =
            build_unsuccessful_uninstall_message("Demo App", true, false, None, false, true);

        assert!(message.contains("用户中断了命令行等待"));
    }
}
