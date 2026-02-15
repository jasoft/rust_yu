use rust_yu_lib::lister;
use rust_yu_lib::lister::models::{InstalledProgram, InstallSource};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct ListOptions {
    pub source: Option<String>,
    pub search: Option<String>,
}

#[tauri::command]
pub async fn list_programs(options: Option<ListOptions>) -> Result<Vec<InstalledProgram>, String> {
    let source = options
        .as_ref()
        .and_then(|o| o.source.as_deref())
        .map(|s| match s.to_lowercase().as_str() {
            "registry" => InstallSource::Registry,
            "msi" => InstallSource::Msi,
            "store" => InstallSource::Store,
            _ => InstallSource::Registry,
        });

    let search = options.and_then(|o| o.search);

    let search_ref = search.as_deref();

    // 由于 list_all_programs 是同步函数，我们可以直接调用
    // 如果需要异步，可以在主项目中添加 async 版本
    let programs = lister::list_all_programs(source, search_ref)
        .map_err(|e| e.to_string())?;

    Ok(programs)
}
