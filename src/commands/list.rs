use crate::modules::common::text::write_csv_stdout;
use crate::modules::lister::{self, models::InstalledProgram};
use anyhow::Result;
use clap::{Parser, ValueEnum};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

const TABLE_SEPARATOR: &str = "  ";
const TABLE_ELLIPSIS: &str = "...";

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
pub enum ListOutputFormat {
    #[value(help = "适合终端人工查看的对齐表格输出")]
    Table,
    #[value(help = "适合自动化处理的 JSON 输出")]
    Json,
    #[value(
        alias = "powershell",
        help = "CSV 输出；`powershell` 是它的别名，适合在 PowerShell 中接 ConvertFrom-Csv"
    )]
    Csv,
}

#[derive(Parser, Debug)]
pub struct ListCommand {
    /// 列表输出格式。`powershell` 是 `csv` 的别名。
    #[arg(long, value_enum, default_value_t = ListOutputFormat::Table)]
    pub format: ListOutputFormat,

    /// 数据来源过滤器。
    /// `all` 默认合并 registry/msi/store，`standard` 仅扫描注册表。
    #[arg(long, default_value = "all")]
    pub source: String,

    /// 可选搜索关键词，会对名称和发布者做忽略大小写、忽略空白的包含匹配。
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
    tracing::debug!(
        "列出已安装程序, source: {}, search: {:?}",
        cmd.source,
        cmd.search
    );

    let query = build_query(&cmd)?;
    let mut programs = lister::list_programs_with_cache(query)?.programs;

    sort_programs(&mut programs, &cmd);

    match cmd.format {
        ListOutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&programs)?);
        }
        ListOutputFormat::Csv => {
            write_csv_stdout(&render_csv(&programs))?;
        }
        ListOutputFormat::Table => {
            print!("{}", render_table(&programs));
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

fn render_table(programs: &[InstalledProgram]) -> String {
    let headers = ["Name", "Publisher", "Version", "Source"];
    let max_widths = [48, 28, 18, 12];
    let rows: Vec<[String; 4]> = programs
        .iter()
        .map(|program| {
            [
                sanitize_inline_text(&program.name),
                sanitize_inline_text(program.publisher.as_deref().unwrap_or_default()),
                sanitize_inline_text(program.version.as_deref().unwrap_or_default()),
                render_install_source(program),
            ]
        })
        .collect();

    let widths = compute_column_widths(&headers, &rows, &max_widths);
    let mut output = String::new();
    output.push_str(&render_table_row(&headers, &widths));
    output.push('\n');
    output.push_str(&render_table_rule(&widths));

    for row in &rows {
        output.push('\n');
        output.push_str(&render_table_row(
            &[
                row[0].as_str(),
                row[1].as_str(),
                row[2].as_str(),
                row[3].as_str(),
            ],
            &widths,
        ));
    }

    output.push_str("\n\n");
    output.push_str(&format!("Count: {}\n", programs.len()));
    output
}

fn render_csv(programs: &[InstalledProgram]) -> String {
    let mut output = String::new();
    output.push_str(
        "Name,Publisher,Version,Source,InstallDate,InstallLocation,UninstallString,QuietUninstallString,Size,EstimatedSize,Id\n",
    );

    for program in programs {
        let row = [
            csv_field(&program.name),
            csv_field(program.publisher.as_deref().unwrap_or_default()),
            csv_field(program.version.as_deref().unwrap_or_default()),
            csv_field(&render_install_source(program)),
            csv_field(program.install_date.as_deref().unwrap_or_default()),
            csv_field(program.install_location.as_deref().unwrap_or_default()),
            csv_field(program.uninstall_string.as_deref().unwrap_or_default()),
            csv_field(
                program
                    .quiet_uninstall_string
                    .as_deref()
                    .unwrap_or_default(),
            ),
            csv_number_field(program.size),
            csv_number_field(program.estimated_size),
            csv_field(&program.id),
        ];
        output.push_str(&row.join(","));
        output.push('\n');
    }

    output
}

fn render_install_source(program: &InstalledProgram) -> String {
    match program.install_source {
        lister::models::InstallSource::Registry => "Registry".to_string(),
        lister::models::InstallSource::Msi => "MSI".to_string(),
        lister::models::InstallSource::Store => "Store".to_string(),
        lister::models::InstallSource::Unknown => "Unknown".to_string(),
    }
}

fn compute_column_widths<const N: usize>(
    headers: &[&str; N],
    rows: &[[String; N]],
    max_widths: &[usize; N],
) -> [usize; N] {
    std::array::from_fn(|index| {
        let header_width = display_width(headers[index]);
        let widest_value = rows
            .iter()
            .map(|row| display_width(&row[index]))
            .max()
            .unwrap_or(0);

        header_width.max(widest_value).min(max_widths[index])
    })
}

fn render_table_row(values: &[&str], widths: &[usize]) -> String {
    values
        .iter()
        .zip(widths.iter())
        .map(|(value, width)| pad_display_width(value, *width))
        .collect::<Vec<_>>()
        .join(TABLE_SEPARATOR)
}

fn render_table_rule(widths: &[usize]) -> String {
    widths
        .iter()
        .map(|width| "-".repeat(*width))
        .collect::<Vec<_>>()
        .join(TABLE_SEPARATOR)
}

fn pad_display_width(value: &str, width: usize) -> String {
    let fitted = truncate_display_width(value, width);
    let padding = width.saturating_sub(display_width(&fitted));

    format!("{fitted}{}", " ".repeat(padding))
}

fn truncate_display_width(value: &str, max_width: usize) -> String {
    if display_width(value) <= max_width {
        return value.to_string();
    }

    if max_width <= TABLE_ELLIPSIS.len() {
        return ".".repeat(max_width);
    }

    let ellipsis_width = TABLE_ELLIPSIS.len();
    let mut current_width = 0;
    let mut truncated = String::new();

    for ch in value.chars() {
        let char_width = UnicodeWidthChar::width(ch).unwrap_or(0);
        if current_width + char_width + ellipsis_width > max_width {
            break;
        }

        truncated.push(ch);
        current_width += char_width;
    }

    truncated.push_str(TABLE_ELLIPSIS);
    truncated
}

fn display_width(value: &str) -> usize {
    UnicodeWidthStr::width(value)
}

fn sanitize_inline_text(value: &str) -> String {
    value.replace(['\r', '\n', '\t'], " ")
}

fn csv_field(value: &str) -> String {
    let sanitized = sanitize_inline_text(value);
    if sanitized.contains([',', '"']) {
        return format!("\"{}\"", sanitized.replace('"', "\"\""));
    }

    sanitized
}

fn csv_number_field(value: Option<u64>) -> String {
    value.map(|number| number.to_string()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::{
        build_query, parse_install_source_selector, render_csv, render_table, sort_programs,
        ListCommand, ListOutputFormat,
    };
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
            format: ListOutputFormat::Table,
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
            format: ListOutputFormat::Table,
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
            format: ListOutputFormat::Table,
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

    #[test]
    fn list_command_accepts_powershell_format() {
        let parsed = ListCommand::try_parse_from(["yu", "--format", "powershell"]);

        assert!(parsed.is_ok(), "expected powershell format to parse");
        assert_eq!(
            parsed.expect("parse should succeed").format,
            ListOutputFormat::Csv
        );
    }

    #[test]
    fn render_table_aligns_wide_characters_with_ascii_headers() {
        let mut programs = vec![
            InstalledProgram::new("中文应用".to_string(), InstallSource::Registry),
            InstalledProgram::new(
                "VeryLongApplicationNameThatShouldBeTrimmedBecauseItExceedsTheVisibleColumnWidth"
                    .to_string(),
                InstallSource::Msi,
            ),
        ];
        programs[0].publisher = Some("测试厂商".to_string());
        programs[1].publisher = Some("Publisher".to_string());

        let rendered = render_table(&programs);

        assert!(rendered.contains("Name"));
        assert!(rendered.contains("Publisher"));
        assert!(rendered.contains("Registry"));
        assert!(rendered.contains("MSI"));
        assert!(rendered.contains("..."));
    }

    #[test]
    fn render_csv_outputs_powershell_friendly_headers() {
        let mut programs = vec![InstalledProgram::new(
            "App,Name".to_string(),
            InstallSource::Store,
        )];
        programs[0].publisher = Some("Vendor \"Quoted\"".to_string());
        programs[0].id = "app-id".to_string();

        let rendered = render_csv(&programs);

        assert!(rendered.starts_with("Name,Publisher,Version,Source"));
        assert!(rendered.contains("\"App,Name\""));
        assert!(rendered.contains("\"Vendor \"\"Quoted\"\"\""));
        assert!(rendered.contains("Store"));
        assert!(rendered.contains("app-id"));
    }
}
