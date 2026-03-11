pub mod clean;
pub mod list;
pub mod report;
pub mod search;
pub mod startup;
pub mod uninstall;

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

    /// 管理 Windows 自启动项
    Startup(startup::StartupCommand),

    /// 卸载程序并清理残留
    Uninstall(uninstall::UninstallCommand),
}
