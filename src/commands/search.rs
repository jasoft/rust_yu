use anyhow::Result;
use clap::Parser;
use crate::modules::scanner;

#[derive(Parser, Debug)]
pub struct SearchCommand {
    /// 程序名称 (必需)
    pub program_name: String,

    /// 搜索类型 (all|registry|files|shortcuts|appdata)
    #[arg(long, default_value = "all")]
    pub trace_type: String,

    /// 输出文件路径
    #[arg(short, long)]
    pub output: Option<String>,

    /// 详细输出
    #[arg(short, long)]
    pub verbose: bool,
}

pub async fn execute(cmd: SearchCommand) -> Result<()> {
    println!("正在搜索 \"{}\" 的残留痕迹...\n", cmd.program_name);

    let trace_types = match cmd.trace_type.as_str() {
        "registry" => vec![scanner::models::TraceType::RegistryKey],
        "files" => vec![scanner::models::TraceType::File, scanner::models::TraceType::AppData],
        "shortcuts" => vec![scanner::models::TraceType::Shortcut],
        "appdata" => vec![scanner::models::TraceType::AppData],
        _ => vec![
            scanner::models::TraceType::RegistryKey,
            scanner::models::TraceType::File,
            scanner::models::TraceType::AppData,
            scanner::models::TraceType::Shortcut,
        ],
    };

    let traces = scanner::scan_all_traces(&cmd.program_name, Some(trace_types)).await?;

    // 过滤只显示存在的
    let existing_traces: Vec<_> = traces.into_iter().filter(|t| t.exists).collect();

    println!("找到 {} 个痕迹:\n", existing_traces.len());

    // 按类型分组输出
    let mut registry_count = 0;
    let mut file_count = 0;
    let mut appdata_count = 0;
    let mut shortcut_count = 0;

    for trace in &existing_traces {
        match trace.trace_type {
            scanner::models::TraceType::RegistryKey => registry_count += 1,
            scanner::models::TraceType::File => file_count += 1,
            scanner::models::TraceType::AppData => appdata_count += 1,
            scanner::models::TraceType::Shortcut => shortcut_count += 1,
            _ => {}
        }

        if cmd.verbose {
            let confidence = match trace.confidence {
                scanner::models::Confidence::High => "高",
                scanner::models::Confidence::Medium => "中",
                scanner::models::Confidence::Low => "低",
            };

            println!(
                "  [{:12}] {} (置信度: {})",
                format!("{:?}", trace.trace_type),
                trace.path,
                confidence
            );
        } else {
            println!(
                "  [{:12}] {}",
                format!("{:?}", trace.trace_type),
                trace.path
            );
        }
    }

    println!("\n--- 统计 ---");
    println!("  注册表: {}", registry_count);
    println!("  文件: {}", file_count);
    println!("  AppData: {}", appdata_count);
    println!("  快捷方式: {}", shortcut_count);

    // 保存到文件
    if let Some(output) = &cmd.output {
        let json = serde_json::to_string_pretty(&existing_traces)?;
        std::fs::write(output, json)?;
        println!("\n结果已保存到: {}", output);
    }

    Ok(())
}
