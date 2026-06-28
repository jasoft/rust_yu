use rust_yu_lib::modules::common::process;
use rust_yu_lib::modules::common::utils;
use rust_yu_lib::modules::lister::models::InstalledProgram;
use rust_yu_lib::modules::lister::storage;
use rust_yu_lib::modules::scanner::models::Trace;
use rust_yu_lib::modules::{cleaner, scanner, uninstall};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

use super::CommandError;

/// 卸载选项
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UninstallOptions {
    /// 目标程序名称
    pub program_name: String,
    /// 是否仅扫描残留（不实际卸载）
    #[serde(default)]
    pub scan_only: bool,
    /// 卸载后是否自动清理残留
    #[serde(default)]
    pub clean_after: bool,
    /// 等待超时（秒）
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
    /// 确认执行（false 则仅预览）
    #[serde(default)]
    pub confirm: bool,
}

fn default_timeout() -> u64 {
    120
}

/// 卸载进度事件
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "stage", rename_all = "snake_case")]
pub enum UninstallProgress {
    /// 已定位目标程序
    TargetResolved {
        program: InstalledProgram,
        uninstall_command: Option<String>,
        route: String,
    },
    /// 卸载进程已启动
    UninstallStarted { command: String },
    /// 卸载进程已完成
    UninstallCompleted {
        exit_code: Option<u32>,
        reboot_required: bool,
    },
    /// 残留扫描完成
    ScanCompleted { traces: Vec<Trace> },
    /// 残留清理结果
    CleanCompleted {
        success_count: usize,
        failed_count: usize,
        bytes_freed: u64,
    },
    /// 整体完成
    Finished {
        success: bool,
        message: String,
    },
}

const UNINSTALL_PROGRESS_EVENT: &str = "uninstall-progress";

/// 卸载结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UninstallResult {
    pub success: bool,
    pub message: String,
    pub exit_code: Option<u32>,
    pub reboot_required: bool,
    pub traces_found: usize,
    pub traces_cleaned: usize,
    pub bytes_freed: u64,
}

/// 卸载程序命令：查找已安装程序、执行卸载命令、可选扫描清理残留。
/// 通过事件系统向前端推送实时进度。
#[tauri::command]
pub async fn uninstall_program(
    app: AppHandle,
    options: UninstallOptions,
) -> Result<UninstallResult, CommandError> {
    let scan_target_name;
    let program: Option<InstalledProgram>;
    let uninstall_command: Option<String>;
    let route_name: String;

    // 1. 查找目标程序
    let resolved = rust_yu_lib::commands::target::resolve_installed_target(&options.program_name)
        .map_err(|e| CommandError::new(e.to_string()))?;

    program = Some(resolved.program.clone());
    scan_target_name = resolved.program.name.clone();
    uninstall_command = resolved
        .program
        .preferred_uninstall_string()
        .map(str::to_string);
    route_name = uninstall::route_name(resolved.program.uninstall_kind).to_string();

    // 保存程序快照（卸载后用于残留扫描）
    storage::save_program_snapshot(std::slice::from_ref(&resolved.program))
        .map_err(|e| CommandError::new(e.to_string()))?;

    let _ = app.emit(
        UNINSTALL_PROGRESS_EVENT,
        UninstallProgress::TargetResolved {
            program: resolved.program.clone(),
            uninstall_command: uninstall_command.clone(),
            route: route_name.clone(),
        },
    );

    if options.scan_only {
        // 仅扫描模式
        let traces = scanner::scan_all_traces(&scan_target_name, None)
            .await
            .map_err(|e| CommandError::new(e.to_string()))?;
        let existing: Vec<_> = traces.into_iter().filter(|t| t.exists).collect();
        let _ = app.emit(
            UNINSTALL_PROGRESS_EVENT,
            UninstallProgress::ScanCompleted {
                traces: existing.clone(),
            },
        );

        return Ok(UninstallResult {
            success: true,
            message: format!("已扫描 {} 的残留痕迹，共 {} 项", scan_target_name, existing.len()),
            exit_code: None,
            reboot_required: false,
            traces_found: existing.len(),
            traces_cleaned: 0,
            bytes_freed: 0,
        });
    }

    // 2. 检查管理员权限
    utils::ensure_running_as_administrator().map_err(|e| CommandError::new(e.to_string()))?;

    // 3. 执行卸载命令
    let cmd_str = uninstall_command
        .as_deref()
        .ok_or_else(|| CommandError::new(format!("未找到 {} 的卸载命令", scan_target_name)))?;

    let normalized_cmd = utils::normalize_uninstall_command(cmd_str);
    let _ = app.emit(
        UNINSTALL_PROGRESS_EVENT,
        UninstallProgress::UninstallStarted {
            command: normalized_cmd.clone(),
        },
    );

    let run_result = process::run_uninstall_command(&normalized_cmd, options.timeout_secs)
        .await
        .map_err(|e| CommandError::new(e.to_string()))?;

    let reboot_required = run_result.classification.reboot_required;

    match run_result.completion_status {
        process::UninstallCompletionStatus::InterruptedByUser => {
            let msg = format!("卸载已被用户中断: {}", scan_target_name);
            let _ = app.emit(
                UNINSTALL_PROGRESS_EVENT,
                UninstallProgress::Finished {
                    success: false,
                    message: msg.clone(),
                },
            );
            return Err(CommandError::new(msg));
        }
        process::UninstallCompletionStatus::Completed => {}
    }

    let _ = app.emit(
        UNINSTALL_PROGRESS_EVENT,
        UninstallProgress::UninstallCompleted {
            exit_code: run_result.exit_code,
            reboot_required,
        },
    );

    // 4. 等待程序实际移除
    if let Some(installed_program) = &program {
        let removal_status =
            uninstall::wait_for_program_removal(installed_program, options.timeout_secs)
                .await
                .map_err(|e| CommandError::new(e.to_string()))?;

        if !removal_status.removed {
            let msg = format!(
                "卸载命令已执行，但程序仍存在: name={}, registered={}, install_dir_exists={}",
                scan_target_name,
                removal_status.still_registered,
                removal_status.install_dir_exists
            );
            let _ = app.emit(
                UNINSTALL_PROGRESS_EVENT,
                UninstallProgress::Finished {
                    success: false,
                    message: msg.clone(),
                },
            );
            return Err(CommandError::new(msg));
        }
    }

    // 5. 残留扫描
    let mut traces = Vec::new();
    if options.clean_after {
        let scan_result = scanner::scan_all_traces(&scan_target_name, None)
            .await
            .map_err(|e| CommandError::new(e.to_string()))?;

        // 补充卸载前快照中的路径
        if let Some(installed_program) = &program {
            traces.extend(build_snapshot_residue_traces(installed_program)?);
        }
        traces.extend(scan_result);

        // 去重并只保留存在的
        let mut seen = std::collections::HashSet::new();
        traces.retain(|t| {
            if !t.exists {
                return false;
            }
            let key = (t.trace_type.to_string(), t.path.to_lowercase());
            seen.insert(key)
        });

        let _ = app.emit(
            UNINSTALL_PROGRESS_EVENT,
            UninstallProgress::ScanCompleted {
                traces: traces.clone(),
            },
        );

        if options.confirm && !traces.is_empty() {
            let results = cleaner::clean_traces(traces.clone(), true)
                .await
                .map_err(|e| CommandError::new(e.to_string()))?;
            let success_count = results.iter().filter(|r| r.success).count();
            let failed_count = results.len() - success_count;
            let bytes_freed: u64 = results.iter().map(|r| r.bytes_freed).sum();

            let _ = app.emit(
                UNINSTALL_PROGRESS_EVENT,
                UninstallProgress::CleanCompleted {
                    success_count,
                    failed_count,
                    bytes_freed,
                },
            );

            storage::invalidate_scan_cache_for_program(&scan_target_name)
                .map_err(|e| CommandError::new(e.to_string()))?;

            let _ = app.emit(
                UNINSTALL_PROGRESS_EVENT,
                UninstallProgress::Finished {
                    success: true,
                    message: format!(
                        "卸载完成，已清理 {} 项残留，释放 {}",
                        success_count,
                        utils::format_size(bytes_freed)
                    ),
                },
            );

            return Ok(UninstallResult {
                success: true,
                message: format!(
                    "卸载完成，已清理 {} 项残留，释放 {}",
                    success_count,
                    utils::format_size(bytes_freed)
                ),
                exit_code: run_result.exit_code,
                reboot_required,
                traces_found: traces.len(),
                traces_cleaned: success_count,
                bytes_freed,
            });
        }
    }

    // 失效缓存
    storage::invalidate_scan_cache_for_program(&scan_target_name)
        .map_err(|e| CommandError::new(e.to_string()))?;

    let trace_count = traces.len();
    let _ = app.emit(
        UNINSTALL_PROGRESS_EVENT,
        UninstallProgress::Finished {
            success: true,
            message: format!("卸载完成: {}", scan_target_name),
        },
    );

    Ok(UninstallResult {
        success: true,
        message: format!("卸载完成: {}", scan_target_name),
        exit_code: run_result.exit_code,
        reboot_required,
        traces_found: trace_count,
        traces_cleaned: 0,
        bytes_freed: 0,
    })
}

/// 基于卸载前快照构造残余候选痕迹
fn build_snapshot_residue_traces(
    program: &InstalledProgram,
) -> Result<Vec<Trace>, CommandError> {
    let mut traces = Vec::new();

    if let Some(install_location) = program.install_location.as_deref() {
        let path = std::path::Path::new(install_location);
        if path.exists() {
            let mut trace = Trace::new(
                program.name.clone(),
                rust_yu_lib::modules::scanner::models::TraceType::File,
                install_location.to_string(),
            )
            .with_description("卸载前快照记录的安装目录".to_string())
            .with_confidence(rust_yu_lib::modules::scanner::models::Confidence::High);

            if let Ok(metadata) = path.metadata() {
                if metadata.is_file() {
                    trace.size = Some(metadata.len());
                }
            }

            traces.push(trace);
        }
    }

    if let Some(registry_path) = program.uninstall_registry_key_path.as_deref() {
        if utils::parse_registry_path(registry_path)
            .and_then(|(hive, subpath)| {
                winreg::RegKey::predef(hive)
                    .open_subkey(subpath)
                    .ok()
            })
            .is_some()
        {
            traces.push(
                Trace::new(
                    program.name.clone(),
                    rust_yu_lib::modules::scanner::models::TraceType::RegistryKey,
                    registry_path.to_string(),
                )
                .with_description("卸载前快照记录的 Uninstall 注册表项".to_string())
                .with_confidence(rust_yu_lib::modules::scanner::models::Confidence::High),
            );
        }
    }

    Ok(traces)
}
