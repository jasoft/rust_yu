//! 程序信息持久化存储模块
//!
//! 用于在卸载程序前保存注册表信息，以便卸载后仍能搜索残留

use crate::modules::common::error::UninstallerError;
use super::models::InstalledProgram;
use std::path::PathBuf;

/// 获取存储目录
fn get_storage_dir() -> Result<PathBuf, UninstallerError> {
    let app_data = dirs::data_dir()
        .ok_or_else(|| UninstallerError::Other("无法获取 AppData 目录".to_string()))?;
    let dir = app_data.join("awake-windows");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

/// 获取程序信息存储文件路径
fn get_storage_file() -> Result<PathBuf, UninstallerError> {
    Ok(get_storage_dir()?.join("programs.json"))
}

/// 保存程序快照
pub fn save_program_snapshot(programs: &[InstalledProgram]) -> Result<(), UninstallerError> {
    let path = get_storage_file()?;

    // 读取现有数据
    let mut all_programs: Vec<InstalledProgram> = if path.exists() {
        let content = std::fs::read_to_string(&path)?;
        serde_json::from_str(&content).unwrap_or_default()
    } else {
        Vec::new()
    };

    // 添加或更新程序
    for program in programs {
        // 按名称更新（如果已存在则替换）
        all_programs.retain(|p| p.name.to_lowercase() != program.name.to_lowercase());
        all_programs.push(program.clone());
    }

    // 写入文件
    let content = serde_json::to_string_pretty(&all_programs)
        .map_err(|e| UninstallerError::Serde(e.to_string()))?;
    std::fs::write(&path, content)?;

    tracing::info!("已保存 {} 个程序信息到存储", programs.len());
    Ok(())
}

/// 获取所有保存的程序
pub fn get_saved_programs() -> Result<Vec<InstalledProgram>, UninstallerError> {
    let path = get_storage_file()?;

    if !path.exists() {
        return Ok(Vec::new());
    }

    let content = std::fs::read_to_string(&path)?;
    let programs: Vec<InstalledProgram> = serde_json::from_str(&content)
        .unwrap_or_default();

    Ok(programs)
}

/// 根据名称获取保存的程序
pub fn get_saved_program(name: &str) -> Result<Option<InstalledProgram>, UninstallerError> {
    let programs = get_saved_programs()?;
    let name_lower = name.to_lowercase();

    Ok(programs
        .into_iter()
        .find(|p| p.name.to_lowercase().contains(&name_lower)))
}

/// 删除保存的程序信息
pub fn delete_saved_program(name: &str) -> Result<(), UninstallerError> {
    let path = get_storage_file()?;

    if !path.exists() {
        return Ok(());
    }

    let content = std::fs::read_to_string(&path)?;
    let mut programs: Vec<InstalledProgram> = serde_json::from_str(&content)
        .unwrap_or_default();

    let name_lower = name.to_lowercase();
    programs.retain(|p| !p.name.to_lowercase().contains(&name_lower));

    let content = serde_json::to_string_pretty(&programs)
        .map_err(|e| UninstallerError::Serde(e.to_string()))?;
    std::fs::write(&path, content)?;

    Ok(())
}

/// 搜索时优先查询保存的数据
pub fn search_programs_with_fallback(query: &str) -> Result<Vec<InstalledProgram>, UninstallerError> {
    let saved = get_saved_programs()?;
    let query_lower = query.to_lowercase();

    let matched: Vec<InstalledProgram> = saved
        .into_iter()
        .filter(|p| {
            p.name.to_lowercase().contains(&query_lower)
                || p.publisher
                    .as_ref()
                    .map(|s| s.to_lowercase().contains(&query_lower))
                    .unwrap_or(false)
        })
        .collect();

    Ok(matched)
}
