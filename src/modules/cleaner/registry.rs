use super::models::CleanResult;
use crate::modules::common::error::UninstallerError;
use crate::modules::common::utils;
use crate::modules::scanner::models::{Trace, TraceType};
use winreg::enums::*;
use winreg::RegKey;

/// 删除注册表痕迹
pub async fn delete_registry_trace(trace: &Trace) -> Result<CleanResult, UninstallerError> {
    let path = &trace.path;

    // 解析路径
    let (hkey, subkey_path) = match utils::parse_registry_path(path) {
        Some((h, p)) => (h, p.to_string()),
        None => {
            return Ok(CleanResult {
                trace_id: trace.id.clone(),
                path: path.clone(),
                success: false,
                error: Some("无效的注册表路径格式".to_string()),
                bytes_freed: 0,
            });
        }
    };

    // 删除操作
    let result = match trace.trace_type {
        TraceType::RegistryKey => delete_registry_key(hkey, &subkey_path),
        TraceType::RegistryValue => delete_registry_value(hkey, &subkey_path),
        _ => {
            return Ok(CleanResult {
                trace_id: trace.id.clone(),
                path: path.clone(),
                success: false,
                error: Some("不支持的注册表痕迹类型".to_string()),
                bytes_freed: 0,
            })
        }
    };

    match result {
        Ok(_) => {
            tracing::info!("已删除注册表项: {}", path);

            Ok(CleanResult {
                trace_id: trace.id.clone(),
                path: path.clone(),
                success: true,
                error: None,
                bytes_freed: 0,
            })
        }
        Err(e) => {
            tracing::error!("删除注册表失败 {}: {}", path, e);

            Ok(CleanResult {
                trace_id: trace.id.clone(),
                path: path.clone(),
                success: false,
                error: Some(e.to_string()),
                bytes_freed: 0,
            })
        }
    }
}

/// 删除注册表键
fn delete_registry_key(hkey: winreg::HKEY, path: &str) -> Result<(), UninstallerError> {
    // 尝试删除键（可能需要先删除子键）
    let key = RegKey::predef(hkey);

    // 尝试直接删除
    match key.delete_subkey_all(path) {
        Ok(_) => Ok(()),
        Err(e) => {
            // 如果是键不存在，返回成功
            if e.kind() == std::io::ErrorKind::NotFound {
                Ok(())
            } else {
                Err(UninstallerError::Registry(e.to_string()))
            }
        }
    }
}

/// 删除注册表值
fn delete_registry_value(hkey: winreg::HKEY, path: &str) -> Result<(), UninstallerError> {
    // 解析键路径和值名
    let parts: Vec<&str> = path.rsplitn(2, '\\').collect();
    if parts.len() != 2 {
        return Err(UninstallerError::Registry("无效的注册表值路径".to_string()));
    }

    let (value_name, key_path) = (parts[0], parts[1]);

    let key = RegKey::predef(hkey).open_subkey_with_flags(key_path, KEY_WRITE)?;

    match key.delete_value(value_name) {
        Ok(_) => Ok(()),
        Err(e) => {
            if e.kind() == std::io::ErrorKind::NotFound {
                Ok(())
            } else {
                Err(UninstallerError::Registry(e.to_string()))
            }
        }
    }
}
