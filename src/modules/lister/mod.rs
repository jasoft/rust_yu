pub mod models;
pub mod registry;
pub mod msi;
pub mod store;
pub mod storage;

use crate::modules::common::error::UninstallerError;
use crate::modules::common::utils;
use models::InstalledProgram;
use models::InstallSource;

/// 列出所有已安装程序
pub fn list_all_programs(
    source: Option<InstallSource>,
    search: Option<&str>,
) -> Result<Vec<InstalledProgram>, UninstallerError> {
    let mut all_programs = Vec::new();

    // 根据 source 参数决定来源
    // None 默认返回 standard 列表 (Registry only，不包括 Store 和 MSI)
    // MSI (Win32_Product) 非常慢，会触发 MSI 验证
    let sources = match source {
        Some(s) => vec![s],
        None => vec![InstallSource::Registry], // 只读取注册表，最快
    };

    for src in &sources {
        match src {
            InstallSource::Registry => {
                match registry::list_registry_programs() {
                    Ok(programs) => all_programs.extend(programs),
                    Err(e) => tracing::warn!("读取注册表程序失败: {}", e),
                }
            }
            InstallSource::Msi => {
                match msi::list_msi_products() {
                    Ok(programs) => all_programs.extend(programs),
                    Err(e) => tracing::warn!("读取MSI程序失败: {}", e),
                }
            }
            InstallSource::Store => {
                match store::list_store_apps() {
                    Ok(programs) => all_programs.extend(programs),
                    Err(e) => tracing::warn!("读取商店应用失败: {}", e),
                }
            }
            InstallSource::Unknown => {}
        }
    }

    // 搜索过滤
    if let Some(query) = search {
        let query_lower = query.to_lowercase();
        all_programs.retain(|p| {
            utils::fuzzy_match(&p.name.to_lowercase(), &query_lower)
                || p.publisher
                    .as_ref()
                    .map(|s| utils::fuzzy_match(&s.to_lowercase(), &query_lower))
                    .unwrap_or(false)
        });
    }

    // 去重 (按名称去重，保留第一个)
    let mut seen = std::collections::HashSet::new();
    all_programs.retain(|p| seen.insert(p.name.to_lowercase()));

    // 按名称排序
    all_programs.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    Ok(all_programs)
}
