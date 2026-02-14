use anyhow::Result;
use clap::Parser;
use crate::modules::lister::{self, models::InstalledProgram};

#[derive(Parser, Debug)]
pub struct ListCommand {
    /// 输出格式 (table/json)
    #[arg(long, default_value = "table")]
    pub format: String,

    /// 过滤来源 (registry|msi|store|all)
    #[arg(long, default_value = "all")]
    pub source: String,

    /// 搜索关键词
    #[arg(short, long)]
    pub search: Option<String>,

    /// 排序字段 (name|date|size)
    #[arg(long, default_value = "name")]
    pub sort_by: String,

    /// 按升序排序
    #[arg(long)]
    pub ascending: bool,
}

pub async fn execute(cmd: ListCommand) -> Result<()> {
    tracing::info!("列出已安装程序, source: {}, search: {:?}", cmd.source, cmd.search);

    let source = match cmd.source.as_str() {
        "registry" => Some(lister::models::InstallSource::Registry),
        "msi" => Some(lister::models::InstallSource::Msi),
        "store" => Some(lister::models::InstallSource::Store),
        _ => None,
    };

    let mut programs = lister::list_all_programs(source, cmd.search.as_deref())?;

    // 排序
    match cmd.sort_by.as_str() {
        "name" => programs.sort_by(|a, b| a.name.cmp(&b.name)),
        "date" => programs.sort_by(|a, b| a.install_date.cmp(&b.install_date)),
        "size" => programs.sort_by(|a, b| b.size.cmp(&a.size)),
        _ => {}
    }

    if !cmd.ascending {
        programs.reverse();
    }

    match cmd.format.as_str() {
        "json" => {
            println!("{}", serde_json::to_string_pretty(&programs)?);
        }
        _ => {
            print_table(&programs);
        }
    }

    Ok(())
}

fn print_table(programs: &[InstalledProgram]) {
    println!("\n{}", "=".repeat(100));
    println!("{:<45} {:<25} {:<15} {:<12}", "名称", "发布者", "版本", "来源");
    println!("{}", "=".repeat(100));

    for p in programs {
        let source = match p.install_source {
            lister::models::InstallSource::Registry => "注册表",
            lister::models::InstallSource::Msi => "MSI",
            lister::models::InstallSource::Store => "商店应用",
            lister::models::InstallSource::Unknown => "未知",
        };

        println!(
            "{:<45} {:<25} {:<15} {:<12}",
            truncate_string(&p.name, 44),
            truncate_string(&p.publisher.clone().unwrap_or_default(), 24),
            truncate_string(&p.version.clone().unwrap_or_default(), 14),
            source
        );
    }

    println!("{}", "=".repeat(100));
    println!("总计: {} 个程序\n", programs.len());
}

fn truncate_string(s: &str, max_len: usize) -> String {
    // 使用 char 边界来正确处理 Unicode 字符（包括中文）
    if s.chars().count() > max_len {
        let chars: String = s.chars().take(max_len - 2).collect();
        format!("{}..", chars)
    } else {
        s.to_string()
    }
}
