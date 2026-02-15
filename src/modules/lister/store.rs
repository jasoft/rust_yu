use super::models::{InstallSource, InstalledProgram};
use crate::modules::common::error::UninstallerError;
use std::process::Command;

/// 列出微软商店应用
pub fn list_store_apps() -> Result<Vec<InstalledProgram>, UninstallerError> {
    // 使用 PowerShell 获取商店应用
    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            r#"
            Get-AppxPackage | Where-Object { $_.IsFramework -eq $false -and $_.SignatureKind -ne 'System' } | ForEach-Object {
                [PSCustomObject]@{
                    Name = $_.Name
                    Publisher = $_.Publisher
                    Version = $_.Version
                    InstallLocation = $_.InstallLocation
                    PackageFullName = $_.PackageFullName
                }
            } | ConvertTo-Json -Depth 2
            "#,
        ])
        .output();

    match output {
        Ok(output) => {
            if output.status.success() {
                let json_str = String::from_utf8_lossy(&output.stdout);
                parse_store_apps(&json_str)
            } else {
                // 如果 PowerShell 失败，返回空列表
                tracing::warn!(
                    "获取商店应用失败: {}",
                    String::from_utf8_lossy(&output.stderr)
                );
                Ok(Vec::new())
            }
        }
        Err(e) => {
            tracing::warn!("执行 PowerShell 失败: {}", e);
            Ok(Vec::new())
        }
    }
}

fn parse_store_apps(json_str: &str) -> Result<Vec<InstalledProgram>, UninstallerError> {
    if json_str.trim().is_empty() {
        return Ok(Vec::new());
    }

    // 尝试解析 JSON
    let apps: Vec<StoreAppJson> = serde_json::from_str(json_str).unwrap_or_else(|_| {
        // 可能是单个对象
        serde_json::from_str::<StoreAppJson>(json_str)
            .map(|a| vec![a])
            .unwrap_or_default()
    });

    let mut programs = Vec::new();

    for app in apps {
        let name = app.name.unwrap_or_default();
        if name.is_empty() {
            continue;
        }

        let mut program = InstalledProgram::new(name, InstallSource::Store);
        program.publisher = app.publisher;
        program.version = app.version;
        program.install_location = app.install_location;

        // 商店应用的卸载命令
        if let Some(ref pkg) = app.package_full_name {
            program.id = pkg.clone();
            program.uninstall_string = Some(format!(
                "powershell -Command \"Remove-AppxPackage -Package '{}'\"",
                pkg
            ));
        } else {
            program.id = uuid::Uuid::new_v4().to_string();
        }

        programs.push(program);
    }

    Ok(programs)
}

#[derive(serde::Deserialize, Debug)]
struct StoreAppJson {
    #[serde(rename = "Name")]
    name: Option<String>,

    #[serde(rename = "Publisher")]
    publisher: Option<String>,

    #[serde(rename = "Version")]
    version: Option<String>,

    #[serde(rename = "InstallLocation")]
    install_location: Option<String>,

    #[serde(rename = "PackageFullName")]
    package_full_name: Option<String>,
}
