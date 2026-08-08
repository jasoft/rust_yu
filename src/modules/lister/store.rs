use super::models::{InstallSource, InstalledProgram, MetadataConfidence, MetadataSource};
use crate::modules::common::error::UninstallerError;
use crate::modules::common::text::{build_powershell_script, decode_windows_output};
use std::path::{Component, Path, PathBuf};
use std::process::Command;

/// 列出微软商店应用。
///
/// `Get-AppxPackage.Name` 是包标识，不是用户在开始菜单中看到的名称。这里同时读取
/// AppxManifest.xml 的展示字段，随后在 Rust 侧解析 `ms-resource:` 和图标资源。
pub fn list_store_apps() -> Result<Vec<InstalledProgram>, UninstallerError> {
    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            &build_powershell_script(
                r#"
            Get-AppxPackage |
                Where-Object {
                    $_.IsFramework -eq $false -and
                    $_.IsResourcePackage -eq $false -and
                    $_.SignatureKind -ne 'System'
                } |
                ForEach-Object {
                    $package = $_
                    $manifestDisplayName = $null
                    $publisherDisplayName = $null
                    $logo = $null

                    try {
                        $manifest = Get-AppxPackageManifest -Package $package.PackageFullName -ErrorAction Stop
                        $manifestDisplayName = "$($manifest.Package.Properties.DisplayName)"
                        $publisherDisplayName = "$($manifest.Package.Properties.PublisherDisplayName)"

                        # 现代应用通常把更合适的列表图标放在 VisualElements 中；
                        # 老包没有该字段时再回退到 Properties.Logo。
                        $visualElements = $manifest.Package.Applications.Application |
                            ForEach-Object { $_.VisualElements } |
                            Where-Object { $_ -ne $null } |
                            Select-Object -First 1
                        if ($null -ne $visualElements -and $visualElements.Square44x44Logo) {
                            $logo = "$($visualElements.Square44x44Logo)"
                        } else {
                            $logo = "$($manifest.Package.Properties.Logo)"
                        }
                    } catch {
                        # 个别受保护包不允许读取清单；保留包 API 返回值作为安全降级。
                    }

                    [PSCustomObject]@{
                        Name = $package.Name
                        DisplayName = $manifestDisplayName
                        Publisher = $package.Publisher
                        PublisherDisplayName = $publisherDisplayName
                        Version = "$($package.Version)"
                        InstallLocation = $package.InstallLocation
                        PackageFullName = $package.PackageFullName
                        Logo = $logo
                    }
                } | ConvertTo-Json -Compress -Depth 3
            "#,
            ),
        ])
        .output();

    match output {
        Ok(output) if output.status.success() => {
            let json_str = decode_windows_output(&output.stdout);
            parse_store_apps(&json_str)
        }
        Ok(output) => {
            tracing::warn!(
                "获取商店应用失败: {}",
                decode_windows_output(&output.stderr)
            );
            Ok(Vec::new())
        }
        Err(error) => {
            tracing::warn!("执行 PowerShell 失败: {}", error);
            Ok(Vec::new())
        }
    }
}

fn parse_store_apps(json_str: &str) -> Result<Vec<InstalledProgram>, UninstallerError> {
    if json_str.trim().is_empty() {
        return Ok(Vec::new());
    }

    let apps = serde_json::from_str::<StoreAppPayload>(json_str)
        .map(StoreAppPayload::into_vec)
        .map_err(|error| UninstallerError::Other(format!("解析商店应用清单失败: {error}")))?;

    Ok(apps.into_iter().filter_map(store_app_to_program).collect())
}

fn store_app_to_program(app: StoreAppJson) -> Option<InstalledProgram> {
    let package_name = app.name.and_then(non_empty)?;
    let install_location = app.install_location.and_then(non_empty);
    let display_name = install_location
        .as_deref()
        .and_then(|root| resolve_manifest_text(root, &package_name, app.display_name.as_deref()))
        .or_else(|| valid_plain_text(app.display_name.as_deref()))
        .unwrap_or_else(|| package_name.clone());

    let publisher = install_location
        .as_deref()
        .and_then(|root| {
            resolve_manifest_text(root, &package_name, app.publisher_display_name.as_deref())
        })
        .or_else(|| valid_plain_text(app.publisher_display_name.as_deref()))
        .or_else(|| app.publisher.and_then(non_empty));

    let icon_path = install_location
        .as_deref()
        .and_then(|root| resolve_manifest_logo(root, app.logo.as_deref()));

    let mut program = InstalledProgram::new(display_name, InstallSource::Store);
    program.publisher = publisher;
    program.version = app.version.and_then(non_empty);
    program.install_location = install_location;
    program.icon_path = icon_path;
    if program.icon_path.is_some() {
        program.icon_source = MetadataSource::Filesystem;
        program.icon_confidence = MetadataConfidence::High;
    }

    if let Some(package_full_name) = app.package_full_name.and_then(non_empty) {
        program.id = package_full_name.clone();
        program.uninstall_string = Some(format!(
            "powershell -Command \"Remove-AppxPackage -Package '{}'\"",
            package_full_name.replace('\'', "''")
        ));
    }

    Some(program)
}

fn non_empty(value: String) -> Option<String> {
    let trimmed = value.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

fn valid_plain_text(value: Option<&str>) -> Option<String> {
    let value = value?.trim();
    (!value.is_empty() && !value.to_ascii_lowercase().starts_with("ms-resource:"))
        .then(|| value.to_string())
}

fn resolve_manifest_text(
    install_location: &str,
    package_name: &str,
    raw_value: Option<&str>,
) -> Option<String> {
    let raw_value = raw_value?.trim();
    if raw_value.is_empty() {
        return None;
    }
    if !raw_value.to_ascii_lowercase().starts_with("ms-resource:") {
        return Some(raw_value.to_string());
    }

    let pri_path = Path::new(install_location).join("resources.pri");
    if !pri_path.is_file() {
        return None;
    }

    let resource_tail = raw_value
        .split_once(':')
        .map(|(_, tail)| tail)
        .unwrap_or_default()
        .trim_start_matches('/')
        .trim();
    if resource_tail.is_empty() {
        return None;
    }

    let last_segment = resource_tail
        .rsplit('/')
        .next()
        .unwrap_or(resource_tail)
        .trim();
    let candidates = [
        format!("ms-resource://{package_name}/resources/{last_segment}"),
        format!("ms-resource://{package_name}/{resource_tail}"),
        raw_value.to_string(),
    ];

    candidates
        .iter()
        .find_map(|resource_key| load_indirect_string(&pri_path, resource_key))
}

#[cfg(windows)]
fn load_indirect_string(pri_path: &Path, resource_key: &str) -> Option<String> {
    use windows::core::PCWSTR;
    use windows::Win32::UI::Shell::SHLoadIndirectString;

    let indirect = format!("@{{{}? {resource_key}}}", pri_path.to_string_lossy());
    let indirect_utf16: Vec<u16> = indirect.encode_utf16().chain(std::iter::once(0)).collect();
    let mut output = vec![0u16; 1024];

    // SHLoadIndirectString 是 Windows 官方的 PRI 间接字符串解析入口；这里只读资源，
    // 输出缓冲区由 Rust 持有且以 NUL 结尾，调用期间指针始终有效。
    if unsafe { SHLoadIndirectString(PCWSTR(indirect_utf16.as_ptr()), &mut output, None) }.is_err()
    {
        return None;
    }

    let length = output
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(output.len());
    let value = String::from_utf16_lossy(&output[..length])
        .trim()
        .to_string();
    (!value.is_empty() && !value.to_ascii_lowercase().starts_with("ms-resource:")).then_some(value)
}

#[cfg(not(windows))]
fn load_indirect_string(_pri_path: &Path, _resource_key: &str) -> Option<String> {
    None
}

fn resolve_manifest_logo(install_location: &str, raw_logo: Option<&str>) -> Option<String> {
    let raw_logo = raw_logo?.trim();
    if raw_logo.is_empty() {
        return None;
    }

    let relative = PathBuf::from(raw_logo.replace('/', "\\"));
    if relative.components().any(|component| {
        matches!(
            component,
            Component::Prefix(_) | Component::RootDir | Component::ParentDir
        )
    }) {
        return None;
    }

    let base_path = Path::new(install_location).join(&relative);
    if base_path.is_file() {
        return Some(base_path.to_string_lossy().to_string());
    }

    let parent = base_path.parent()?;
    let stem = base_path.file_stem()?.to_string_lossy();
    let extension = base_path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("png");

    // 48/44/32 最适合当前列表缓存尺寸；scale 变体按接近 100% 的顺序降级。
    for qualifier in [
        "targetsize-48",
        "targetsize-44",
        "targetsize-32",
        "targetsize-64",
        "scale-100",
        "scale-125",
        "scale-150",
        "scale-200",
        "scale-400",
    ] {
        let candidate = parent.join(format!("{stem}.{qualifier}.{extension}"));
        if candidate.is_file() {
            return Some(candidate.to_string_lossy().to_string());
        }
    }

    // 某些包还带 contrast/theme 等限定符。只接受相同文件名前缀的图片，
    // 避免从包目录中误选其他应用的图标。
    let prefix = format!("{}.", stem.to_ascii_lowercase());
    std::fs::read_dir(parent)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .find(|path| {
            let file_name = path
                .file_name()
                .map(|value| value.to_string_lossy().to_ascii_lowercase())
                .unwrap_or_default();
            let is_supported_image = matches!(
                path.extension()
                    .and_then(|value| value.to_str())
                    .map(str::to_ascii_lowercase)
                    .as_deref(),
                Some("png") | Some("jpg") | Some("jpeg") | Some("ico")
            );
            file_name.starts_with(&prefix) && is_supported_image
        })
        .map(|path| path.to_string_lossy().to_string())
}

#[derive(serde::Deserialize, Debug)]
#[serde(untagged)]
enum StoreAppPayload {
    Many(Vec<StoreAppJson>),
    One(StoreAppJson),
}

impl StoreAppPayload {
    fn into_vec(self) -> Vec<StoreAppJson> {
        match self {
            Self::Many(apps) => apps,
            Self::One(app) => vec![app],
        }
    }
}

#[derive(serde::Deserialize, Debug)]
struct StoreAppJson {
    #[serde(rename = "Name")]
    name: Option<String>,
    #[serde(rename = "DisplayName")]
    display_name: Option<String>,
    #[serde(rename = "Publisher")]
    publisher: Option<String>,
    #[serde(rename = "PublisherDisplayName")]
    publisher_display_name: Option<String>,
    #[serde(rename = "Version")]
    version: Option<String>,
    #[serde(rename = "InstallLocation")]
    install_location: Option<String>,
    #[serde(rename = "PackageFullName")]
    package_full_name: Option<String>,
    #[serde(rename = "Logo")]
    logo: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::{parse_store_apps, resolve_manifest_logo};
    use crate::modules::lister::models::{MetadataConfidence, MetadataSource, UninstallKind};
    use std::fs;

    #[test]
    fn parse_store_apps_prefers_manifest_display_metadata() {
        let json = r#"[{
            "Name": "Microsoft.WindowsCalculator",
            "DisplayName": "Windows Calculator",
            "Publisher": "CN=Microsoft Corporation",
            "PublisherDisplayName": "Microsoft Corporation",
            "Version": "1.0.0",
            "InstallLocation": "C:\\Program Files\\WindowsApps\\Calculator",
            "PackageFullName": "Microsoft.WindowsCalculator_1.0.0_x64__abc",
            "Logo": "Assets\\Square44x44Logo.png"
        }]"#;

        let programs =
            parse_store_apps(json).unwrap_or_else(|error| panic!("unexpected error: {error}"));

        assert_eq!(programs.len(), 1);
        assert_eq!(programs[0].name, "Windows Calculator");
        assert_eq!(
            programs[0].publisher.as_deref(),
            Some("Microsoft Corporation")
        );
        assert_eq!(programs[0].id, "Microsoft.WindowsCalculator_1.0.0_x64__abc");
        assert_eq!(programs[0].uninstall_kind, UninstallKind::Store);
        assert!(programs[0]
            .uninstall_string
            .as_deref()
            .unwrap_or_default()
            .contains("Remove-AppxPackage"));
    }

    #[test]
    fn parse_store_apps_falls_back_to_package_identity_for_resource_name() {
        let json = r#"{
            "Name": "Demo.Store",
            "DisplayName": "ms-resource:AppName",
            "Publisher": "CN=Demo",
            "Version": "2.0.0",
            "InstallLocation": "C:\\missing",
            "PackageFullName": "Demo.Store_2.0.0_x64__xyz"
        }"#;

        let programs =
            parse_store_apps(json).unwrap_or_else(|error| panic!("unexpected error: {error}"));

        assert_eq!(programs.len(), 1);
        assert_eq!(programs[0].name, "Demo.Store");
        assert_eq!(programs[0].version.as_deref(), Some("2.0.0"));
    }

    #[test]
    fn resolve_manifest_logo_prefers_target_size_variant() {
        let root =
            std::env::temp_dir().join(format!("rust-yu-store-logo-test-{}", uuid::Uuid::new_v4()));
        let assets = root.join("Assets");
        assert!(fs::create_dir_all(&assets).is_ok());
        let target = assets.join("Square44x44Logo.targetsize-48.png");
        assert!(fs::write(&target, b"png").is_ok());

        assert_eq!(
            resolve_manifest_logo(&root.to_string_lossy(), Some(r"Assets\Square44x44Logo.png")),
            Some(target.to_string_lossy().to_string())
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn parsed_store_logo_is_high_confidence_filesystem_metadata() {
        let root = std::env::temp_dir().join(format!(
            "rust-yu-store-program-test-{}",
            uuid::Uuid::new_v4()
        ));
        let assets = root.join("Assets");
        assert!(fs::create_dir_all(&assets).is_ok());
        assert!(fs::write(assets.join("Logo.png"), b"png").is_ok());
        let escaped_root = root.to_string_lossy().replace('\\', "\\\\");
        let json = format!(
            r#"{{"Name":"Demo.Store","DisplayName":"Demo","InstallLocation":"{escaped_root}","PackageFullName":"Demo_1","Logo":"Assets\\Logo.png"}}"#
        );

        let programs =
            parse_store_apps(&json).unwrap_or_else(|error| panic!("unexpected error: {error}"));
        assert_eq!(programs[0].icon_source, MetadataSource::Filesystem);
        assert_eq!(programs[0].icon_confidence, MetadataConfidence::High);

        let _ = fs::remove_dir_all(root);
    }
}
