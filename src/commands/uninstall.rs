//! uninstall 命令 - 卸载程序并清理残留

use crate::modules::common::utils;
use crate::modules::lister::storage;
use crate::modules::{cleaner, lister, scanner};
use anyhow::Result;
use clap::Parser;

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
    println!("=== 卸载程序: {} ===\n", cmd.target);

    // 1. 查找程序并保存注册表信息
    println!("[1/4] 搜索程序并保存注册表信息...");
    let program = find_and_save_program(&cmd.target, cmd.uninstall_string.as_deref())?;

    if let Some(prog) = &program {
        println!("  - 找到程序: {}", prog.name);
        if let Some(publisher) = &prog.publisher {
            println!("  - 发布者: {}", publisher);
        }
        if let Some(version) = &prog.version {
            println!("  - 版本: {}", version);
        }
        if let Some(location) = &prog.install_location {
            println!("  - 安装位置: {}", location);
        }
    } else {
        println!("  - 未在已安装程序中找到，将尝试直接执行卸载命令");
    }

    // 2. 执行卸载命令并等待
    println!("\n[2/4] 执行卸载命令并等待进程结束...");

    let uninstall_str = program
        .as_ref()
        .and_then(|p| p.preferred_uninstall_string().map(ToOwned::to_owned))
        .or(cmd.uninstall_string);

    if let Some(uninstall_str) = uninstall_str {
        println!("  - 卸载命令: {}", uninstall_str);

        utils::ensure_running_as_administrator()?;

        // 执行卸载并等待进程组结束
        match run_uninstall_with_wait(&uninstall_str, cmd.timeout).await {
            Ok(_) => {
                println!("  - 卸载进程已结束");
            }
            Err(e) => {
                println!("  - 警告: 卸载进程等待超时或出错: {}", e);
            }
        }

        if let Some(installed_program) = &program {
            wait_for_program_removal(installed_program, cmd.timeout).await?;
        }
    } else {
        anyhow::bail!("未找到程序对应的卸载命令，请确认程序名称或手动传入 --uninstall-string");
    }

    // 3. 如果需要清理残留
    if cmd.clean {
        println!("\n[3/4] 搜索残留痕迹...");

        // 搜索残留
        let traces = scanner::scan_all_traces(&cmd.target, None).await?;
        let existing_traces: Vec<_> = traces.into_iter().filter(|t| t.exists).collect();

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
                    let size = trace
                        .size
                        .map(|s| utils::format_size(s))
                        .unwrap_or_default();
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
        storage::delete_saved_program(&cmd.target)?;
        println!("  - 已清理");
    } else {
        println!("\n[4/4] 保留程序信息缓存 (可使用 --preserve=false 清理)");
    }

    // 卸载流程会改变已安装程序列表，保守起见直接失效列表缓存
    storage::invalidate_scan_cache_for_program(&cmd.target)?;
    println!("  - 已失效安装列表缓存");

    println!("\n=== 卸载完成 ===");
    Ok(())
}

/// 查找程序并保存注册表信息
fn find_and_save_program(
    target: &str,
    uninstall_string: Option<&str>,
) -> Result<Option<lister::models::InstalledProgram>> {
    // 如果提供了 uninstall_string，直接创建程序信息
    if let Some(uninstall_str) = uninstall_string {
        let program = lister::models::InstalledProgram::new(
            target.to_string(),
            lister::models::InstallSource::Registry,
        );
        let mut prog = program;
        prog.uninstall_string = Some(uninstall_str.to_string());
        storage::save_program_snapshot(&[prog.clone()])?;
        return Ok(Some(prog));
    }

    // 搜索已安装的程序
    let programs = lister::list_all_programs(None, None)?;

    let matched = select_matching_program(programs, target)?;

    if let Some(program) = matched {
        // 保存到存储
        storage::save_program_snapshot(&[program.clone()])?;
        Ok(Some(program))
    } else {
        Ok(None)
    }
}

fn select_matching_program(
    programs: Vec<lister::models::InstalledProgram>,
    target: &str,
) -> Result<Option<lister::models::InstalledProgram>> {
    let target_lower = target.to_lowercase();

    let mut exact_matches = programs
        .iter()
        .filter(|program| program.name.to_lowercase() == target_lower)
        .cloned()
        .collect::<Vec<_>>();
    if let Some(program) = exact_matches.pop() {
        return Ok(Some(program));
    }

    let partial_matches = programs
        .into_iter()
        .filter(|program| program.name.to_lowercase().contains(&target_lower))
        .collect::<Vec<_>>();

    match partial_matches.as_slice() {
        [] => Ok(None),
        [program] => Ok(Some(program.clone())),
        _ => {
            let candidates = partial_matches
                .iter()
                .map(|program| program.name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            anyhow::bail!(
                "找到多个匹配项，请使用更精确的名称: {}",
                candidates
            );
        }
    }
}

async fn wait_for_program_removal(
    program: &lister::models::InstalledProgram,
    timeout_secs: u64,
) -> Result<()> {
    use std::time::{Duration, Instant};

    let started_at = Instant::now();
    let expected_name = program.name.to_lowercase();
    let install_location = program
        .install_location
        .as_deref()
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(std::path::PathBuf::from);

    loop {
        let still_registered = match program.install_source {
            lister::models::InstallSource::Registry | lister::models::InstallSource::Unknown => {
                lister::registry::registry_program_exists(&program.name)?
            }
            _ => lister::list_all_programs(Some(program.install_source), None)?
                .into_iter()
                .any(|candidate| candidate.name.to_lowercase() == expected_name),
        };
        let install_dir_exists = install_location
            .as_ref()
            .map(|path| path.exists())
            .unwrap_or(false);

        tracing::info!(
            "等待卸载完成, name={}, still_registered={}, install_dir_exists={}",
            program.name,
            still_registered,
            install_dir_exists
        );

        if !still_registered && !install_dir_exists {
            println!("  - 已确认程序条目与安装目录均已移除");
            return Ok(());
        }

        if started_at.elapsed() >= Duration::from_secs(timeout_secs) {
            anyhow::bail!(
                "卸载命令已退出，但程序仍残留: registered={}, install_dir_exists={}, name={}",
                still_registered,
                install_dir_exists,
                program.name
            );
        }

        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

/// 执行卸载命令并等待进程组结束
async fn run_uninstall_with_wait(uninstall_string: &str, _timeout_secs: u64) -> Result<()> {
    use std::process::{Command, Stdio};
    use std::time::Duration;

    // 处理常见的卸载命令格式
    let cmd_str = utils::normalize_uninstall_command(uninstall_string);

    tracing::info!("执行卸载命令: {}", cmd_str);

    // 使用 spawn 而不是 output，这样我们可以获取 PID
    #[cfg(windows)]
    {
        let mut wrapper_script_path = None;
        let mut child = if cmd_str.to_lowercase().starts_with("msiexec") {
            let (executable, arguments) = utils::split_command_for_spawn(&cmd_str)?;
            Command::new(&executable)
                .args(&arguments)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?
        } else {
            let wrapper_script = utils::create_command_wrapper_script(&cmd_str)?;
            let script_arg = wrapper_script.to_string_lossy().to_string();
            wrapper_script_path = Some(wrapper_script);
            Command::new("cmd")
                .args(["/C", &script_arg])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?
        };

        let pid = child.id();
        println!("  - 进程 PID: {}", pid);

        let status = tokio::task::spawn_blocking(move || child.wait()).await??;
        if let Some(script_path) = wrapper_script_path {
            let _ = std::fs::remove_file(script_path);
        }
        if !status.success() {
            anyhow::bail!("卸载进程退出码异常: {:?}", status.code());
        }

        // 额外等待一段时间，确保清理完成
        tokio::time::sleep(Duration::from_secs(2)).await;
    }

    #[cfg(not(windows))]
    {
        let output = Command::new("cmd").args(["/C", &cmd_str]).output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("卸载命令执行失败: {}", stderr);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{select_matching_program, UninstallCommand};
    use crate::modules::lister::models::{InstallSource, InstalledProgram};
    use clap::Parser;

    #[test]
    fn preserve_defaults_to_true() {
        let command =
            UninstallCommand::try_parse_from(["rust-yu", "example-app"]).expect("应能解析默认参数");

        assert!(command.preserve);
    }

    #[test]
    fn preserve_accepts_explicit_false() {
        let command = UninstallCommand::try_parse_from([
            "rust-yu",
            "example-app",
            "--preserve=false",
        ])
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
    fn select_matching_program_prefers_exact_name_match() {
        let exact =
            InstalledProgram::new("7-Zip 24.09 (x64)".to_string(), InstallSource::Registry);
        let partial =
            InstalledProgram::new("7-Zip 24.01 (x64 edition)".to_string(), InstallSource::Registry);

        let selected = select_matching_program(vec![partial, exact.clone()], "7-Zip 24.09 (x64)")
            .expect("精确匹配不应报错");

        assert_eq!(selected.map(|program| program.name), Some(exact.name));
    }

    #[test]
    fn select_matching_program_rejects_ambiguous_partial_matches() {
        let first =
            InstalledProgram::new("7-Zip 24.09 (x64)".to_string(), InstallSource::Registry);
        let second =
            InstalledProgram::new("7-Zip 24.01 (x64 edition)".to_string(), InstallSource::Registry);

        let error = select_matching_program(vec![first, second], "7-Zip")
            .expect_err("模糊匹配多个程序时应拒绝继续");

        assert!(error.to_string().contains("找到多个匹配项"));
    }
}
