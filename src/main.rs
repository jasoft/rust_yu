use anyhow::Result;
use clap::Parser;
use std::process;

mod commands;
mod modules;

#[derive(Parser, Debug)]
#[command(name = "yu")]
#[command(about = "Windows 卸载程序命令行工具", long_about = None)]
#[command(version = "0.1.3")]
struct Cli {
    #[command(subcommand)]
    command: commands::Command,

    /// 详细输出模式
    #[arg(short, long, global = true)]
    verbose: bool,
}

#[tokio::main]
async fn main() -> Result<()> {
    modules::common::text::init_console_utf8();

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
        commands::Command::Prepare(cmd) => commands::prepare::execute(cmd).await,
        commands::Command::Search(cmd) => commands::search::execute(cmd).await,
        commands::Command::Clean(cmd) => commands::clean::execute(cmd).await,
        commands::Command::Report(cmd) => commands::report::execute(cmd).await,
        commands::Command::Startup(cmd) => commands::startup::execute(cmd).await,
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

#[cfg(test)]
mod tests {
    use super::Cli;
    use clap::{error::ErrorKind, CommandFactory, Parser};

    #[test]
    fn cli_command_name_is_yu() {
        let command = Cli::command();

        assert_eq!(command.get_name(), "yu");
    }

    #[test]
    fn all_top_level_commands_support_help_flag() {
        let help_invocations = [
            ["yu", "--help"].as_slice(),
            ["yu", "list", "--help"].as_slice(),
            ["yu", "prepare", "--help"].as_slice(),
            ["yu", "search", "--help"].as_slice(),
            ["yu", "clean", "--help"].as_slice(),
            ["yu", "report", "--help"].as_slice(),
            ["yu", "startup", "--help"].as_slice(),
            ["yu", "uninstall", "--help"].as_slice(),
        ];

        for invocation in help_invocations {
            let error =
                Cli::try_parse_from(invocation).expect_err("`--help` 应触发帮助输出而不是正常解析");
            assert_eq!(
                error.kind(),
                ErrorKind::DisplayHelp,
                "help invocation should render help: {:?}",
                invocation
            );
        }
    }

    #[test]
    fn startup_subcommands_support_help_flag() {
        let help_invocations = [
            ["yu", "startup", "list", "--help"].as_slice(),
            ["yu", "startup", "add", "--help"].as_slice(),
            ["yu", "startup", "show", "--help"].as_slice(),
            ["yu", "startup", "sources", "--help"].as_slice(),
            ["yu", "startup", "enable", "--help"].as_slice(),
            ["yu", "startup", "disable", "--help"].as_slice(),
            ["yu", "startup", "delete", "--help"].as_slice(),
            ["yu", "startup", "rollback", "--help"].as_slice(),
        ];

        for invocation in help_invocations {
            let error =
                Cli::try_parse_from(invocation).expect_err("`--help` 应触发帮助输出而不是正常解析");
            assert_eq!(
                error.kind(),
                ErrorKind::DisplayHelp,
                "startup help invocation should render help: {:?}",
                invocation
            );
        }
    }
}
