use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
pub struct ReportCommand {
    /// 报告文件路径或程序名
    pub identifier: String,

    /// 查看所有报告列表
    #[arg(long)]
    pub list: bool,

    /// 输出 HTML 文件
    #[arg(short, long)]
    pub html: Option<String>,
}

pub async fn execute(cmd: ReportCommand) -> Result<()> {
    let reports_dir = get_reports_dir()?;

    if cmd.list {
        // 列出所有报告
        println!("卸载报告目录: {}\n", reports_dir.display());

        if !reports_dir.exists() {
            println!("暂无报告文件");
            return Ok(());
        }

        let entries = std::fs::read_dir(&reports_dir)?;
        let mut count = 0;

        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().map(|e| e == "html").unwrap_or(false) {
                count += 1;
                let metadata = std::fs::metadata(&path)?;
                let modified = metadata.modified()?
                    .duration_since(std::time::UNIX_EPOCH)
                    .ok()
                    .and_then(|d| chrono::DateTime::<chrono::Utc>::from_timestamp(d.as_secs() as i64, 0))
                    .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
                    .unwrap_or_default();

                println!("  {}  (修改时间: {})", path.file_name().unwrap().to_string_lossy(), modified);
            }
        }

        println!("\n共 {} 个报告", count);
        return Ok(());
    }

    // 尝试作为文件路径或程序名加载报告
    let report_path = PathBuf::from(&cmd.identifier);

    if report_path.exists() {
        // 作为文件路径
        let content = std::fs::read_to_string(&report_path)?;
        print_html_content(&content, &cmd.html)?;
    } else {
        // 作为程序名，搜索报告
        let search_pattern = format!("uninstall_report_{}.html", cmd.identifier);
        let found = reports_dir.join(&search_pattern);

        if found.exists() {
            let content = std::fs::read_to_string(&found)?;
            print_html_content(&content, &cmd.html)?;
        } else {
            // 搜索所有匹配的报告
            if reports_dir.exists() {
                let entries = std::fs::read_dir(&reports_dir)?;
                let mut found_reports = Vec::new();

                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().map(|e| e == "html").unwrap_or(false) {
                        let filename = path.file_name().unwrap().to_string_lossy().to_lowercase();
                        if filename.contains(&cmd.identifier.to_lowercase()) {
                            found_reports.push(path);
                        }
                    }
                }

                if found_reports.is_empty() {
                    println!("未找到报告: {}", cmd.identifier);
                    println!("使用 --list 查看所有报告");
                } else {
                    println!("找到以下报告:");
                    for report in &found_reports {
                        println!("  {}", report.display());
                    }
                }
            } else {
                println!("报告目录不存在");
            }
        }
    }

    Ok(())
}

fn print_html_content(content: &str, output_path: &Option<String>) -> Result<()> {
    if let Some(path) = output_path {
        std::fs::write(path, content)?;
        println!("报告已保存到: {}", path);
    } else {
        // 简单打印 HTML 内容（实际可以打开浏览器）
        println!("\n{}", content);
    }

    Ok(())
}

fn get_reports_dir() -> Result<PathBuf> {
    let dir = dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("rust-yu")
        .join("reports");

    Ok(dir)
}
