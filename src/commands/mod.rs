pub mod list;
pub mod search;
pub mod clean;
pub mod report;

use clap::Subcommand;

#[derive(Subcommand, Debug)]
pub enum Command {
    /// 列出所有已安装的程序
    List(list::ListCommand),

    /// 搜索程序残留痕迹
    Search(search::SearchCommand),

    /// 清理程序残留痕迹
    Clean(clean::CleanCommand),

    /// 查看卸载报告
    Report(report::ReportCommand),
}
