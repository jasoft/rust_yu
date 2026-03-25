use crate::modules::lister::{self, models::InstalledProgram};
use anyhow::Result;
use clap::Parser;

#[derive(Parser, Debug)]
pub struct ListCommand {
    /// 列表输出格式，`table` 适合终端人工查看，`json` 适合 agent 解析。
    #[arg(long, default_value = "table")]
    pub format: String,

    /// 数据来源过滤器。
    /// `all` 默认合并 registry/msi/store，`standard` 仅扫描注册表。
    #[arg(long, default_value = "all")]
    pub source: String,

    /// 可选搜索关键词，会对名称和发布者做模糊匹配。
    #[arg(short, long)]
    pub search: Option<String>,

    /// 排序字段，可选 `name`、`date`、`size`。
    #[arg(long, default_value = "name")]
    pub sort_by: String,

    /// 按升序排序。
    #[arg(long, conflicts_with = "descending")]
    pub ascending: bool,

    /// 按降序排序。
    #[arg(long, conflicts_with = "ascending")]
    pub descending: bool,

    /// 跳过读取缓存并立即重建基础列表缓存。
    #[arg(long)]
    pub refresh: bool,
}

pub async fn execute(cmd: ListCommand) -> Result<()> {
    tracing::info!(
        "列出已安装程序, source: {}, search: {:?}",
        cmd.source,
        cmd.search
    );

    let query = build_query(&cmd)?;
    let mut programs = lister::list_programs_with_cache(query)?.programs;

    sort_programs(&mut programs, &cmd);

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

fn sort_programs(programs: &mut [InstalledProgram], cmd: &ListCommand) {
    match cmd.sort_by.as_str() {
        "name" => programs.sort_by(|a, b| a.name.cmp(&b.name)),
        "date" => programs.sort_by(|a, b| a.install_date.cmp(&b.install_date)),
        "size" => programs.sort_by(|a, b| a.size.cmp(&b.size)),
        _ => {}
    }

    if cmd.descending {
        programs.reverse();
    }
}

fn build_query(cmd: &ListCommand) -> Result<lister::models::ListProgramsQuery> {
    let source = parse_install_source_selector(&cmd.source)?;

    Ok(lister::models::ListProgramsQuery {
        source,
        search: cmd.search.clone(),
        refresh: cmd.refresh,
        cache_ttl_seconds: lister::storage::DEFAULT_CACHE_TTL_SECONDS,
    })
}

pub(crate) fn parse_install_source_selector(
    source: &str,
) -> Result<lister::models::InstallSourceSelector> {
    lister::models::InstallSourceSelector::parse(source)
        .ok_or_else(|| anyhow::anyhow!("未知来源: {source}"))
}

fn print_table(programs: &[InstalledProgram]) {
    println!("\n{}", "=".repeat(100));
    println!(
        "{:<45} {:<25} {:<15} {:<12}",
        "名称", "发布者", "版本", "来源"
    );
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

#[cfg(test)]
mod tests {
    use super::{build_query, parse_install_source_selector, sort_programs, ListCommand};
    use crate::modules::lister::models::{InstallSource, InstallSourceSelector, InstalledProgram};
    use clap::Parser;

    #[test]
    fn list_command_accepts_refresh_flag() {
        let parsed = ListCommand::try_parse_from(["rust-yu", "--refresh"]);

        assert!(parsed.is_ok(), "expected --refresh to parse successfully");
    }

    #[test]
    fn list_command_defaults_to_all_sources() {
        let parsed = ListCommand::try_parse_from(["rust-yu"])
            .expect("expected default list command to parse");

        assert_eq!(parsed.source, "all");
    }

    #[test]
    fn build_query_enables_refresh_when_flag_is_set() {
        let cmd = ListCommand {
            format: "table".to_string(),
            source: "standard".to_string(),
            search: Some("7zip".to_string()),
            sort_by: "name".to_string(),
            ascending: false,
            descending: false,
            refresh: true,
        };

        let query = build_query(&cmd).expect("build query should succeed");

        assert!(query.refresh);
        assert_eq!(query.search.as_deref(), Some("7zip"));
        assert_eq!(query.source, InstallSourceSelector::Standard);
    }

    #[test]
    fn build_query_treats_all_as_distinct_selector() {
        let cmd = ListCommand {
            format: "table".to_string(),
            source: "all".to_string(),
            search: None,
            sort_by: "name".to_string(),
            ascending: false,
            descending: false,
            refresh: false,
        };

        let query = build_query(&cmd).expect("build query should succeed");

        assert_eq!(query.source, InstallSourceSelector::All);
    }

    #[test]
    fn default_name_sort_is_ascending() {
        let mut programs = vec![
            InstalledProgram::new("Zeta".to_string(), InstallSource::Registry),
            InstalledProgram::new("Alpha".to_string(), InstallSource::Registry),
        ];
        let cmd = ListCommand {
            format: "table".to_string(),
            source: "standard".to_string(),
            search: None,
            sort_by: "name".to_string(),
            ascending: false,
            descending: false,
            refresh: false,
        };

        sort_programs(&mut programs, &cmd);

        assert_eq!(programs[0].name, "Alpha");
        assert_eq!(programs[1].name, "Zeta");
    }

    #[test]
    fn list_command_accepts_descending_flag() {
        let parsed = ListCommand::try_parse_from(["yu", "--descending"]);

        assert!(
            parsed.is_ok(),
            "expected --descending to parse successfully"
        );
    }

    #[test]
    fn list_command_rejects_conflicting_direction_flags() {
        let parsed = ListCommand::try_parse_from(["yu", "--ascending", "--descending"]);

        assert!(
            parsed.is_err(),
            "expected conflicting direction flags to be rejected"
        );
    }

    #[test]
    fn parse_install_source_selector_rejects_unknown_values() {
        let result = parse_install_source_selector("winget");

        assert!(result.is_err(), "expected unknown selector to be rejected");
    }
}
