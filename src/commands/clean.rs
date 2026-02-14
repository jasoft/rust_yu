use anyhow::Result;
use clap::Parser;
use chrono::Utc;
use crate::modules::{lister, scanner, cleaner, reporter};

#[derive(Parser, Debug)]
pub struct CleanCommand {
    /// 程序名称、ID 或卸载命令
    pub target: String,

    /// 确认删除 (不指定则预览)
    #[arg(long)]
    pub confirm: bool,

    /// 要删除的痕迹类型
    #[arg(long, default_value = "all")]
    pub trace_type: String,

    /// 排除的痕迹 ID (可多次指定)
    #[arg(long)]
    pub exclude: Vec<String>,

    /// 生成报告
    #[arg(long)]
    pub report: bool,

    /// 报告输出路径
    #[arg(long)]
    pub report_path: Option<String>,

    /// 先执行程序的卸载命令
    #[arg(long)]
    pub uninstall: bool,

    /// 卸载命令 (如果 target 不是已安装的程序)
    #[arg(long)]
    pub uninstall_string: Option<String>,
}

pub async fn execute(cmd: CleanCommand) -> Result<()> {
    // 1. 如果指定了 --uninstall，先尝试卸载程序
    if cmd.uninstall {
        println!("正在尝试卸载程序: {}\n", cmd.target);

        let uninstall_result = if let Some(uninstall_str) = &cmd.uninstall_string {
            // 使用指定的卸载命令
            run_uninstall_command(uninstall_str).await
        } else {
            // 搜索已安装的程序并获取卸载命令
            let programs = lister::list_all_programs(None, Some(&cmd.target))?;
            if let Some(program) = programs.iter().find(|p| p.name.to_lowercase().contains(&cmd.target.to_lowercase())) {
                if let Some(uninstall_str) = &program.uninstall_string {
                    run_uninstall_command(uninstall_str).await
                } else {
                    anyhow::bail!("程序没有卸载命令")
                }
            } else {
                anyhow::bail!("未找到程序: {}", cmd.target)
            }
        };

        match uninstall_result {
            Ok(_) => println!("程序卸载命令已执行\n"),
            Err(e) => println!("卸载命令执行失败: {}\n", e),
        }
    }

    // 2. 搜索残留痕迹
    println!("正在搜索残留痕迹...");
    let trace_types = match cmd.trace_type.as_str() {
        "registry" => vec![scanner::models::TraceType::RegistryKey],
        "files" => vec![scanner::models::TraceType::File],
        "appdata" => vec![scanner::models::TraceType::AppData],
        "shortcuts" => vec![scanner::models::TraceType::Shortcut],
        _ => vec![
            scanner::models::TraceType::RegistryKey,
            scanner::models::TraceType::File,
            scanner::models::TraceType::AppData,
            scanner::models::TraceType::Shortcut,
        ],
    };

    let all_traces = scanner::scan_all_traces(&cmd.target, Some(trace_types)).await?;

    // 过滤存在的和排除的
    let traces_to_clean: Vec<_> = all_traces
        .into_iter()
        .filter(|t| t.exists && !cmd.exclude.contains(&t.id))
        .collect();

    println!("找到 {} 个残留痕迹\n", traces_to_clean.len());

    // 3. 预览模式 (不确认)
    if !cmd.confirm {
        println!("=== 预览模式 ===");
        println!("使用 --confirm 确认删除\n");

        for trace in &traces_to_clean {
            let size = trace.size.map(|s| format_size(s)).unwrap_or_default();
            println!(
                "  [{:12}] {} {}",
                format!("{:?}", trace.trace_type),
                trace.path,
                if !size.is_empty() { format!("({})", size) } else { String::new() }
            );
        }

        println!("\n共 {} 项", traces_to_clean.len());
        return Ok(());
    }

    // 4. 执行删除
    println!("=== 开始清理 ===\n");

    let clean_results = cleaner::clean_traces(traces_to_clean, true).await?;

    // 5. 统计结果
    let success_count = clean_results.iter().filter(|r| r.success).count();
    let failed_count = clean_results.len() - success_count;
    let total_freed: u64 = clean_results.iter().map(|r| r.bytes_freed).sum();

    println!("\n--- 清理完成 ---");
    println!("  成功: {}", success_count);
    println!("  失败: {}", failed_count);
    println!("  释放空间: {}", format_size(total_freed));

    // 6. 生成报告
    if cmd.report {
        let report = reporter::models::UninstallerReport {
            id: uuid::Uuid::new_v4().to_string(),
            program_name: cmd.target.clone(),
            generated_at: Utc::now(),
            traces_found: vec![],
            traces_removed: clean_results,
            total_size_freed: total_freed,
            success: failed_count == 0,
            warnings: vec![],
        };

        let report_path = cmd.report_path.unwrap_or_else(|| format!("uninstall_report_{}.html", cmd.target));
        let html = reporter::html::generate_html_report(&report)?;
        std::fs::write(&report_path, html)?;
        println!("\n报告已生成: {}", report_path);
    }

    Ok(())
}

async fn run_uninstall_command(uninstall_string: &str) -> Result<()> {
    // 处理常见的卸载命令格式
    let cmd = if uninstall_string.to_lowercase().starts_with("msiexec") {
        // MSI 卸载
        format!("{} /quiet /norestart", uninstall_string)
    } else {
        // 普通卸载命令
        uninstall_string.to_string()
    };

    tracing::info!("执行卸载命令: {}", cmd);

    #[cfg(windows)]
    {
        use std::process::Command as StdCommand;

        let output = if cmd.contains("msiexec") {
            StdCommand::new("cmd")
                .args(["/C", &cmd])
                .output()?
        } else {
            StdCommand::new("cmd")
                .args(["/C", &cmd])
                .output()?
        };

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("卸载命令执行失败: {}", stderr);
        }
    }

    Ok(())
}

fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}
