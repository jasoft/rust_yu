//! uninstall 命令 - 卸载程序并清理残留

use anyhow::Result;
use clap::Parser;
use crate::modules::{lister, scanner, cleaner};
use crate::modules::lister::storage;
use crate::modules::common::utils;

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
    #[arg(long, default_value = "true")]
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
        .and_then(|p| p.uninstall_string.clone())
        .or(cmd.uninstall_string);

    if let Some(uninstall_str) = uninstall_str {
        println!("  - 卸载命令: {}", uninstall_str);

        // 执行卸载并等待进程组结束
        match run_uninstall_with_wait(&uninstall_str, cmd.timeout).await {
            Ok(_) => {
                println!("  - 卸载进程已结束");
            }
            Err(e) => {
                println!("  - 警告: 卸载进程等待超时或出错: {}", e);
            }
        }
    } else {
        println!("  - 未找到卸载命令");
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
                    let size = trace.size.map(|s| utils::format_size(s)).unwrap_or_default();
                    println!(
                        "  [{}] {:12} {} {}",
                        i + 1,
                        format!("{:?}", trace.trace_type),
                        trace.path,
                        if !size.is_empty() { format!("({})", size) } else { String::new() }
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
    let programs = lister::list_all_programs(None, Some(target))?;

    // 查找最匹配的程序
    let target_lower = target.to_lowercase();
    let matched = programs
        .into_iter()
        .find(|p| p.name.to_lowercase().contains(&target_lower));

    if let Some(program) = matched {
        // 保存到存储
        storage::save_program_snapshot(&[program.clone()])?;
        Ok(Some(program))
    } else {
        Ok(None)
    }
}

/// 执行卸载命令并等待进程组结束
async fn run_uninstall_with_wait(uninstall_string: &str, timeout_secs: u64) -> Result<()> {
    use std::process::{Command, Stdio};
    use std::time::Duration;

    // 处理常见的卸载命令格式
    let cmd_str = if uninstall_string.to_lowercase().starts_with("msiexec") {
        format!("{} /quiet /norestart", uninstall_string)
    } else {
        uninstall_string.to_string()
    };

    tracing::info!("执行卸载命令: {}", cmd_str);

    // 使用 spawn 而不是 output，这样我们可以获取 PID
    #[cfg(windows)]
    {
        let child = Command::new("cmd")
            .args(["/C", &cmd_str])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        let pid = child.id();
        println!("  - 进程 PID: {}", pid);

        // 等待进程组结束
        utils::wait_for_process_group(pid, timeout_secs).await?;

        // 额外等待一段时间，确保清理完成
        tokio::time::sleep(Duration::from_secs(2)).await;
    }

    #[cfg(not(windows))]
    {
        let output = Command::new("cmd")
            .args(["/C", &cmd_str])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("卸载命令执行失败: {}", stderr);
        }
    }

    Ok(())
}
