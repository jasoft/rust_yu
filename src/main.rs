use anyhow::Result;
use clap::Parser;
use std::process;

mod commands;
mod modules;

#[derive(Parser, Debug)]
#[command(name = "rust-yu")]
#[command(about = "Windows 卸载程序命令行工具", long_about = None)]
#[command(version = "0.1.0")]
struct Cli {
    #[command(subcommand)]
    command: commands::Command,

    /// 详细输出模式
    #[arg(short, long, global = true)]
    verbose: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    // 初始化日志
    modules::common::logging::init_logging(false);

    // 解析命令行参数
    let cli = Cli::parse();

    // 根据 verbose 重新初始化日志级别
    if cli.verbose {
        modules::common::logging::init_logging(true);
    }

    // 执行命令
    let result = match cli.command {
        commands::Command::List(cmd) => commands::list::execute(cmd).await,
        commands::Command::Search(cmd) => commands::search::execute(cmd).await,
        commands::Command::Clean(cmd) => commands::clean::execute(cmd).await,
        commands::Command::Report(cmd) => commands::report::execute(cmd).await,
        commands::Command::Uninstall(cmd) => commands::uninstall::execute(cmd).await,
    };

    match result {
        Ok(_) => {}
        Err(e) => {
            if cli.verbose {
                tracing::error!("错误: {}", e);
            } else {
                eprintln!("错误: {}", e);
            }
            process::exit(1);
        }
    }

    Ok(())
}
