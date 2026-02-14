use crate::modules::common::error::UninstallerError;
use super::models::{InstalledProgram, InstallSource};

/// 列出 MSI 产品
pub fn list_msi_products() -> Result<Vec<InstalledProgram>, UninstallerError> {
    #[cfg(windows)]
    {
        list_msi_products_impl()
    }

    #[cfg(not(windows))]
    {
        Ok(Vec::new())
    }
}

#[cfg(windows)]
fn list_msi_products_impl() -> Result<Vec<InstalledProgram>, UninstallerError> {
    // 使用 PowerShell 枚举 MSI 产品
    use std::process::Command;

    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            r#"
            $products = Get-WmiObject -Class Win32_Product | ForEach-Object {
                [PSCustomObject]@{
                    Name = $_.Name
                    Vendor = $_.Vendor
                    Version = $_.Version
                    InstallLocation = $_.InstallLocation
                    IdentifyingNumber = $_.IdentifyingNumber
                }
            }
            $products | ConvertTo-Json -Depth 2
            "#,
        ])
        .output();

    match output {
        Ok(output) => {
            if output.status.success() {
                let json_str = String::from_utf8_lossy(&output.stdout);
                parse_msi_products(&json_str)
            } else {
                tracing::warn!("获取MSI产品失败: {}", String::from_utf8_lossy(&output.stderr));
                Ok(Vec::new())
            }
        }
        Err(e) => {
            tracing::warn!("执行 PowerShell 失败: {}", e);
            Ok(Vec::new())
        }
    }
}

#[cfg(windows)]
fn parse_msi_products(json_str: &str) -> Result<Vec<InstalledProgram>, UninstallerError> {
    if json_str.trim().is_empty() || json_str.trim() == "null" {
        return Ok(Vec::new());
    }

    let products: Vec<MsiProductJson> = serde_json::from_str(json_str)
        .unwrap_or_else(|_| {
            serde_json::from_str::<MsiProductJson>(json_str)
                .map(|p| vec![p])
                .unwrap_or_default()
        });

    let mut programs = Vec::new();

    for product in products {
        let name = product.name.unwrap_or_default();
        if name.is_empty() {
            continue;
        }

        let mut program = InstalledProgram::new(name, InstallSource::Msi);
        program.publisher = product.vendor;
        program.version = product.version;
        program.install_location = product.install_location;

        // 使用 IdentifyingNumber 构建卸载命令
        if let Some(id) = product.identifying_number {
            program.uninstall_string = Some(format!("msiexec /x{{{}}}", id));
            program.id = format!("msi-{}", id.to_lowercase());
        }

        programs.push(program);
    }

    Ok(programs)
}

#[derive(serde::Deserialize, Debug)]
struct MsiProductJson {
    #[serde(rename = "Name")]
    name: Option<String>,

    #[serde(rename = "Vendor")]
    vendor: Option<String>,

    #[serde(rename = "Version")]
    version: Option<String>,

    #[serde(rename = "InstallLocation")]
    install_location: Option<String>,

    #[serde(rename = "IdentifyingNumber")]
    identifying_number: Option<String>,
}
