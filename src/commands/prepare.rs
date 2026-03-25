use std::io::{self, Write};

use anyhow::{bail, Result};
use clap::{Parser, ValueEnum};

use crate::commands::list::parse_install_source_selector;
use crate::modules::lister;
use crate::modules::lister::models::{
    MetadataWarmupProgress, MetadataWarmupQuery, MetadataWarmupSummary,
};

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
pub enum PrepareOutputFormat {
    /// 输出结构化 JSON 汇总，适合 agent 或脚本解析。
    Json,
    /// 输出适合终端人工查看的文本汇总。
    Text,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
pub enum PrepareProgressFormat {
    /// 输出人类可读的进度文本。
    Text,
    /// 逐行输出 JSON 进度事件，适合 agent 流式消费。
    Jsonl,
    /// 关闭进度输出，仅保留最终汇总。
    None,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
pub enum PrepareProgressStream {
    /// 将进度写到标准错误，保持标准输出留给最终结果。
    Stderr,
    /// 将进度写到标准输出，适合只消费单一流的调用方。
    Stdout,
}

#[derive(Parser, Debug)]
pub struct PrepareMetadataCommand {
    #[arg(
        long,
        help = "预热图标缓存文件。",
        long_help = "预热图标缓存文件。\n仅处理没有现成图标缓存的新程序，或图标缓存文件已经缺失的程序。"
    )]
    pub icons: bool,

    #[arg(
        long,
        help = "每次重新扫描程序大小。",
        long_help = "每次重新扫描程序大小。\n会统计安装目录，以及保守匹配到的 AppData 数据目录，例如 Roaming、Local、LocalLow 下的应用数据。"
    )]
    pub sizes: bool,

    /// 数据来源过滤器，推荐使用 `standard` 或 `registry`。
    #[arg(long, default_value = "standard")]
    pub source: String,

    /// 可选搜索关键词，仅对匹配到的程序执行预热。
    #[arg(short, long)]
    pub search: Option<String>,

    #[arg(
        long,
        help = "预热前强制重建基础列表缓存。",
        long_help = "预热前强制重建基础列表缓存。\n当新安装程序尚未进入缓存时，建议与 --icons 搭配使用，以便发现新增程序。"
    )]
    pub refresh: bool,

    /// 最终汇总输出格式。
    #[arg(long, value_enum, default_value_t = PrepareOutputFormat::Json)]
    pub format: PrepareOutputFormat,

    /// 实时进度输出格式。
    #[arg(long, value_enum, default_value_t = PrepareProgressFormat::Text)]
    pub progress_format: PrepareProgressFormat,

    /// 实时进度输出目标流。
    #[arg(long, value_enum, default_value_t = PrepareProgressStream::Stderr)]
    pub progress_stream: PrepareProgressStream,
}

pub async fn execute(cmd: PrepareMetadataCommand) -> Result<()> {
    if !cmd.icons && !cmd.sizes {
        bail!("至少需要指定 --icons 或 --sizes");
    }

    let progress_format = cmd.progress_format;
    let progress_stream = cmd.progress_stream;
    let query = MetadataWarmupQuery {
        source: parse_install_source_selector(&cmd.source),
        search: cmd.search.clone(),
        refresh: cmd.refresh,
        cache_ttl_seconds: lister::storage::DEFAULT_CACHE_TTL_SECONDS,
        icons: cmd.icons,
        sizes: cmd.sizes,
    };

    let summary = lister::warmup_program_metadata(query, |event| {
        emit_progress(progress_format, progress_stream, &event);
    })?;

    match cmd.format {
        PrepareOutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&summary)?);
        }
        PrepareOutputFormat::Text => {
            print_summary(&summary);
        }
    }

    Ok(())
}

fn emit_progress(
    format: PrepareProgressFormat,
    stream: PrepareProgressStream,
    event: &MetadataWarmupProgress,
) {
    match format {
        PrepareProgressFormat::None => {}
        PrepareProgressFormat::Text => {
            let line = format_progress_line(event);
            write_progress_line(stream, &line);
        }
        PrepareProgressFormat::Jsonl => {
            if let Ok(line) = serde_json::to_string(event) {
                write_progress_line(stream, &line);
            }
        }
    }
}

fn format_progress_line(event: &MetadataWarmupProgress) -> String {
    match event.stage {
        lister::models::MetadataWarmupStage::Started => {
            format!("[{}] started total={}", event.kind.as_str(), event.total)
        }
        lister::models::MetadataWarmupStage::ItemStarted => format!(
            "[{}] {}/{} {}",
            event.kind.as_str(),
            event.current,
            event.total,
            event.program_name.as_deref().unwrap_or("<unknown>")
        ),
        lister::models::MetadataWarmupStage::ItemFinished => format!(
            "[{}] {}/{} {} {}{}",
            event.kind.as_str(),
            event.current,
            event.total,
            event
                .status
                .map(|status| format!("{status:?}").to_lowercase())
                .unwrap_or_else(|| "finished".to_string()),
            event.program_name.as_deref().unwrap_or("<unknown>"),
            event
                .message
                .as_deref()
                .map(|value| format!(" ({value})"))
                .unwrap_or_default()
        ),
        lister::models::MetadataWarmupStage::Completed => {
            format!("[{}] completed total={}", event.kind.as_str(), event.total)
        }
    }
}

fn write_progress_line(stream: PrepareProgressStream, line: &str) {
    match stream {
        PrepareProgressStream::Stderr => {
            let mut stderr = io::stderr().lock();
            let _ = writeln!(stderr, "{line}");
            let _ = stderr.flush();
        }
        PrepareProgressStream::Stdout => {
            let mut stdout = io::stdout().lock();
            let _ = writeln!(stdout, "{line}");
            let _ = stdout.flush();
        }
    }
}

fn print_summary(summary: &MetadataWarmupSummary) {
    println!("总程序数: {}", summary.total_programs);
    println!("匹配程序数: {}", summary.matched_programs);

    if let Some(stats) = &summary.icons {
        println!(
            "图标预热: total={} eligible={} processed={} updated={} skipped={} failed={}",
            stats.total,
            stats.eligible,
            stats.processed,
            stats.updated,
            stats.skipped,
            stats.failed
        );
    }

    if let Some(stats) = &summary.sizes {
        println!(
            "大小预热: total={} eligible={} processed={} updated={} skipped={} failed={}",
            stats.total,
            stats.eligible,
            stats.processed,
            stats.updated,
            stats.skipped,
            stats.failed
        );
    }

    println!(
        "缓存状态: hit={} valid={} refreshed={} reason={}",
        summary.cache.cache_hit,
        summary.cache.cache_valid,
        summary.cache.refreshed,
        summary.cache.reason.as_deref().unwrap_or("none")
    );
}

#[cfg(test)]
mod tests {
    use super::{PrepareMetadataCommand, PrepareOutputFormat, PrepareProgressFormat};
    use clap::{Parser, ValueEnum};

    #[test]
    fn prepare_command_accepts_streaming_flags() {
        let parsed = PrepareMetadataCommand::try_parse_from([
            "yu",
            "--icons",
            "--sizes",
            "--search",
            "git",
            "--format",
            "json",
            "--progress-format",
            "jsonl",
            "--progress-stream",
            "stderr",
        ]);

        assert!(parsed.is_ok(), "expected metadata prepare flags to parse");
    }

    #[test]
    fn prepare_output_format_defaults_are_stable() {
        assert_eq!(
            PrepareOutputFormat::Json
                .to_possible_value()
                .map(|value| value.get_name().to_string()),
            Some("json".to_string())
        );
        assert_eq!(
            PrepareProgressFormat::Jsonl
                .to_possible_value()
                .map(|value| value.get_name().to_string()),
            Some("jsonl".to_string())
        );
    }
}
