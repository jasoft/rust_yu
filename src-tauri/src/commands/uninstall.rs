use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct UninstallOptions {
    pub program_name: String,
    pub scan_only: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UninstallResult {
    pub success: bool,
    pub message: String,
    pub traces_found: u32,
    pub traces_cleaned: u32,
}

/// 卸载程序命令
/// 注意：Windows 程序卸载通常需要通过系统 API 或 MSI 安装程序
/// 这里主要演示如何调用扫描和清理功能
#[tauri::command]
pub async fn uninstall_program(
    program_name: String,
    scan_only: bool,
) -> Result<UninstallResult, String> {
    // TODO: 实现真正的程序卸载功能
    // 这需要调用 Windows API 如 MsiEnumProducts 或通过注册表查找卸载命令

    Ok(UninstallResult {
        success: true,
        message: if scan_only {
            format!("已扫描 {} 的残留痕迹，请使用 clean 命令清理", program_name)
        } else {
            format!("程序卸载功能正在开发中")
        },
        traces_found: 0,
        traces_cleaned: 0,
    })
}
