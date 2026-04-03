//! uninstall 命令 - 卸载程序并清理残留

use crate::commands::target;
use crate::modules::common::{process, utils};
use crate::modules::lister::{models::InstallSource, storage};
use crate::modules::scanner::models::{Confidence, Trace, TraceType};
use crate::modules::{cleaner, lister, scanner, uninstall};
use anyhow::Result;
use clap::Parser;
use std::collections::HashSet;
use winreg::RegKey;

#[derive(Parser, Debug)]
pub struct UninstallCommand {
    /// 程序名称 (必需)
    pub target: String,

    /// 自动确认删除 (不指定则预览)
    #[arg(long)]
    pub confirm: bool,

    /// 卸载后搜索并清理残留
    #[arg(long)]
    pub clean: bool,

    /// 保留搜索缓存供 uninstall 后使用 (默认启用)
    /// 设为 false 可在卸载后删除缓存
    #[arg(long, action = clap::ArgAction::Set, default_value_t = true)]
    pub preserve: bool,

    /// 等待超时时间 (秒)
    #[arg(long, default_value = "120")]
    pub timeout: u64,

    /// 指定卸载命令 (如果 target 不是已安装的程序)
    #[arg(long)]
    pub uninstall_string: Option<String>,

    /// 输出格式 (table/json)
    #[arg(long, default_value = "table")]
    pub format: String,
}

pub async fn execute(cmd: UninstallCommand) -> Result<()> {
    let explicit_uninstall_string = cmd.uninstall_string.clone();
    println!("=== 卸载程序: {} ===\n", cmd.target);

    // 1. 查找程序并保存注册表信息
    println!("[1/4] 搜索程序并保存注册表信息...");
    let program = find_and_save_program(&cmd.target, explicit_uninstall_string.as_deref())?;
    let scan_target_name = program
        .as_ref()
        .map(|installed_program| installed_program.name.as_str())
        .unwrap_or(cmd.target.as_str());

    if explicit_uninstall_string.is_some() {
        println!("  - 名称: {}", scan_target_name);
        println!("  - 说明: 已显式传入 --uninstall-string，跳过已安装 App 搜索");
    } else if let Some(prog) = &program {
        println!("{}", target::format_selected_target(prog));
    } else {
        println!("  - 未在已安装程序中找到，将尝试直接执行卸载命令");
    }

    // 2. 执行卸载命令并等待
    println!("\n[2/4] 执行卸载命令并等待完成...");

    let uninstall_str = if let Some(command) = explicit_uninstall_string.as_deref() {
        Some(utils::normalize_uninstall_command(command))
    } else if let Some(installed_program) = program.as_ref() {
        Some(uninstall::resolve_uninstall_command(installed_program)?)
    } else {
        None
    };

    if let Some(uninstall_str) = uninstall_str {
        println!("  - 卸载命令: {}", uninstall_str);
        if explicit_uninstall_string.is_none() {
            if let Some(installed_program) = &program {
                println!(
                    "  - 卸载类型: {}",
                    uninstall::route_name(installed_program.uninstall_kind)
                );
            }
        }

        utils::ensure_running_as_administrator()?;

        if process::is_likely_interactive_uninstall(&uninstall_str) {
            println!(
                "{}",
                process::build_interactive_uninstall_message(scan_target_name)
            );
        }

        let run_result = process::run_uninstall_command(&uninstall_str, cmd.timeout).await?;
        if run_result.used_job_object {
            println!("  - 已使用 Job Object 等待卸载进程链结束");
        } else {
            println!("  - 已等待直接卸载进程结束");
        }

        match run_result.completion_status {
            process::UninstallCompletionStatus::InterruptedByUser => {
                let message = process::build_unsuccessful_uninstall_message(
                    scan_target_name,
                    true,
                    false,
                    run_result.exit_code,
                    false,
                    true,
                );
                anyhow::bail!(message);
            }
            process::UninstallCompletionStatus::Completed => {
                println!(
                    "  - 卸载进程链已结束，exit_code={}",
                    run_result
                        .exit_code
                        .map(|code| code.to_string())
                        .unwrap_or_else(|| "unknown".to_string())
                );
            }
        }

        if let Some(installed_program) = &program {
            let removal_status =
                uninstall::wait_for_program_removal(installed_program, cmd.timeout).await?;
            if removal_status.removed {
                match installed_program.uninstall_kind {
                    lister::models::UninstallKind::Store => {
                        println!("  - 已确认 Store 包条目已移除");
                    }
                    lister::models::UninstallKind::Msi => {
                        println!("  - 已确认 MSI 产品条目已移除");
                    }
                    lister::models::UninstallKind::Legacy => {
                        println!("  - 已确认程序条目与安装目录均已移除");
                    }
                }
            } else {
                anyhow::bail!(process::build_unsuccessful_uninstall_message(
                    &installed_program.name,
                    removal_status.still_registered,
                    removal_status.install_dir_exists,
                    run_result.exit_code,
                    run_result.classification.user_cancelled,
                    false,
                ));
            }
        } else if run_result.classification.user_cancelled {
            anyhow::bail!("卸载已取消，未执行后续残留清理");
        } else if !run_result.classification.successful {
            anyhow::bail!(
                "卸载失败，退出码异常: {}",
                run_result
                    .exit_code
                    .map(|code| code.to_string())
                    .unwrap_or_else(|| "unknown".to_string())
            );
        }

        if run_result.classification.reboot_required {
            println!("  - 卸载器提示需要重启后才能完全生效");
        }
    } else {
        anyhow::bail!("未找到程序对应的卸载命令，请确认程序名称或手动传入 --uninstall-string");
    }

    // 3. 如果需要清理残留
    if cmd.clean {
        println!("\n[3/4] 搜索残留痕迹...");

        // 搜索残留
        let mut traces = scanner::scan_all_traces(scan_target_name, None).await?;
        if let Some(installed_program) = &program {
            // 卸载完成后，部分安装目录或 Uninstall 项可能已经被删除。
            // 这里把卸载前快照里的关键路径重新参与候选集合，避免后续清理只依赖名称扫描。
            traces.extend(build_snapshot_residue_traces(installed_program)?);
        }
        let existing_traces = dedupe_traces(traces.into_iter().filter(|t| t.exists).collect());

        println!("  - 找到 {} 个残留痕迹\n", existing_traces.len());

        if existing_traces.is_empty() {
            println!("  未发现残留痕迹");
        } else {
            // 预览或确认删除
            if cmd.confirm {
                // 直接执行清理
                println!("  执行清理中...\n");
                let results = cleaner::clean_traces(existing_traces.clone(), true).await?;

                // 统计结果
                let success_count = results.iter().filter(|r| r.success).count();
                let failed_count = results.len() - success_count;
                let total_freed: u64 = results.iter().map(|r| r.bytes_freed).sum();

                println!("  --- 清理完成 ---");
                println!("    成功: {}", success_count);
                println!("    失败: {}", failed_count);
                println!("    释放空间: {}", utils::format_size(total_freed));
            } else {
                // 预览模式，让用户选择
                println!("=== 预览模式 ===\n");
                for (i, trace) in existing_traces.iter().enumerate() {
                    let size = trace.size.map(utils::format_size).unwrap_or_default();
                    println!(
                        "  [{}] {:12} {} {}",
                        i + 1,
                        format!("{:?}", trace.trace_type),
                        trace.path,
                        if !size.is_empty() {
                            format!("({})", size)
                        } else {
                            String::new()
                        }
                    );
                }

                println!("\n  共 {} 项", existing_traces.len());
                println!("\n  使用 --confirm 参数确认删除");

                // 让用户选择
                println!("\n  请输入要删除的项 (如 1,3,5 或 all):");
                print!("  > ");
                use std::io::Write;
                std::io::stdout().flush()?;

                let mut input = String::new();
                std::io::stdin().read_line(&mut input)?;
                let input = input.trim();

                let to_delete: Vec<usize> = if input == "all" {
                    (1..=existing_traces.len()).collect()
                } else {
                    input
                        .split(',')
                        .filter_map(|s| s.trim().parse::<usize>().ok())
                        .filter(|&i| i > 0 && i <= existing_traces.len())
                        .collect()
                };

                if to_delete.is_empty() {
                    println!("  未选择任何项，取消删除");
                } else {
                    let traces_to_delete: Vec<_> = to_delete
                        .iter()
                        .filter_map(|&i| existing_traces.get(i - 1).cloned())
                        .collect();

                    println!("\n  删除 {} 项...\n", traces_to_delete.len());
                    let results = cleaner::clean_traces(traces_to_delete, true).await?;

                    let success_count = results.iter().filter(|r| r.success).count();
                    println!("  成功删除: {}", success_count);
                }
            }
        }
    } else {
        println!("\n[3/4] 跳过清理 (未指定 --clean)");
    }

    // 4. 清理保存的程序信息
    if !cmd.preserve {
        println!("\n[4/4] 清理保存的程序信息...");
        storage::delete_saved_program(scan_target_name)?;
        println!("  - 已清理");
    } else {
        println!("\n[4/4] 保留程序信息缓存 (可使用 --preserve=false 清理)");
    }

    // 卸载流程会改变已安装程序列表，保守起见直接失效列表缓存
    storage::invalidate_scan_cache_for_program(scan_target_name)?;
    println!("  - 已失效安装列表缓存");

    println!("\n=== 卸载完成 ===");
    Ok(())
}

fn build_snapshot_residue_traces(program: &lister::models::InstalledProgram) -> Result<Vec<Trace>> {
    let mut traces = Vec::new();

    if let Some(install_location) = program.install_location.as_deref() {
        let path = std::path::Path::new(install_location);
        if path.exists() {
            let mut trace = Trace::new(
                program.name.clone(),
                TraceType::File,
                install_location.to_string(),
            )
            .with_description("卸载前快照记录的安装目录".to_string())
            .with_confidence(Confidence::High);

            if let Ok(metadata) = path.metadata() {
                if metadata.is_file() {
                    trace.size = Some(metadata.len());
                }
            }

            traces.push(trace);
        }
    }

    if let Some(registry_path) = program.uninstall_registry_key_path.as_deref() {
        if registry_key_exists(registry_path) {
            traces.push(
                Trace::new(
                    program.name.clone(),
                    TraceType::RegistryKey,
                    registry_path.to_string(),
                )
                .with_description("卸载前快照记录的 Uninstall 注册表项".to_string())
                .with_confidence(Confidence::High),
            );
        }
    }

    Ok(traces)
}

fn registry_key_exists(path: &str) -> bool {
    utils::parse_registry_path(path)
        .and_then(|(hive, subpath)| RegKey::predef(hive).open_subkey(subpath).ok())
        .is_some()
}

fn dedupe_traces(traces: Vec<Trace>) -> Vec<Trace> {
    let mut seen = HashSet::new();
    let mut deduped = Vec::with_capacity(traces.len());

    for trace in traces {
        let key = (trace.trace_type.to_string(), trace.path.to_lowercase());
        if seen.insert(key) {
            deduped.push(trace);
        }
    }

    deduped
}

/// 查找程序并保存注册表信息
fn find_and_save_program(
    target: &str,
    uninstall_string: Option<&str>,
) -> Result<Option<lister::models::InstalledProgram>> {
    // 如果提供了 uninstall_string，直接创建程序信息
    if let Some(uninstall_str) = uninstall_string {
        let program =
            lister::models::InstalledProgram::new(target.to_string(), InstallSource::Unknown);
        let mut prog = program;
        prog.uninstall_string = Some(uninstall_str.to_string());
        storage::save_program_snapshot(&[prog.clone()])?;
        return Ok(Some(prog));
    }

    let program = target::resolve_installed_target(target)?.program;
    storage::save_program_snapshot(std::slice::from_ref(&program))?;
    Ok(Some(program))
}

#[cfg(test)]
mod tests {
    use super::{build_snapshot_residue_traces, UninstallCommand};
    use crate::modules::lister::models::{InstallSource, InstalledProgram, UninstallKind};
    use crate::modules::scanner::models::TraceType;
    use crate::modules::uninstall;
    use clap::Parser;
    use std::path::PathBuf;
    use winreg::enums::HKEY_CURRENT_USER;
    use winreg::RegKey;

    #[test]
    fn preserve_defaults_to_true() {
        let command =
            UninstallCommand::try_parse_from(["rust-yu", "example-app"]).expect("应能解析默认参数");

        assert!(command.preserve);
    }

    #[test]
    fn preserve_accepts_explicit_false() {
        let command =
            UninstallCommand::try_parse_from(["rust-yu", "example-app", "--preserve=false"])
                .expect("应能解析显式 false");

        assert!(!command.preserve);
    }

    #[test]
    fn preferred_uninstall_string_uses_quiet_variant_when_available() {
        let mut program = InstalledProgram::new("7-Zip".to_string(), InstallSource::Registry);
        program.uninstall_string = Some(r#""C:\Program Files\7-Zip\Uninstall.exe""#.to_string());
        program.quiet_uninstall_string =
            Some(r#""C:\Program Files\7-Zip\Uninstall.exe" /S"#.to_string());

        assert_eq!(
            program.preferred_uninstall_string(),
            Some(r#""C:\Program Files\7-Zip\Uninstall.exe" /S"#)
        );
    }

    #[test]
    fn uninstall_route_name_tracks_uninstall_kind() {
        assert_eq!(uninstall::route_name(UninstallKind::Legacy), "legacy");
        assert_eq!(uninstall::route_name(UninstallKind::Msi), "msi");
        assert_eq!(uninstall::route_name(UninstallKind::Store), "store");
    }

    #[test]
    fn build_snapshot_residue_traces_uses_saved_install_location_and_uninstall_key() {
        let suffix = uuid::Uuid::new_v4().to_string();
        let install_root =
            std::env::temp_dir().join(format!("rust-yu-uninstall-snapshot-{suffix}"));
        let registry_path = format!(r"Software\rust-yu-test\{suffix}");
        let hive = RegKey::predef(HKEY_CURRENT_USER);

        std::fs::create_dir_all(&install_root).expect("应能创建测试安装目录");
        hive.create_subkey(&registry_path)
            .expect("应能创建测试注册表项");

        let mut program = InstalledProgram::new("Demo App".to_string(), InstallSource::Registry);
        program.install_location = Some(install_root.to_string_lossy().to_string());
        program.uninstall_registry_key_path = Some(format!(r"HKCU\{registry_path}"));

        let traces =
            build_snapshot_residue_traces(&program).expect("应能基于卸载前快照生成残余候选");

        assert!(traces.iter().any(|trace| {
            trace.trace_type == TraceType::File && trace.path == install_root.to_string_lossy()
        }));
        assert!(traces.iter().any(|trace| {
            trace.trace_type == TraceType::RegistryKey
                && trace.path == format!(r"HKCU\{registry_path}")
        }));

        let _ = hive.delete_subkey_all(&registry_path);
        let _ = std::fs::remove_dir_all(PathBuf::from(&install_root));
    }
}
