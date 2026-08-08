use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use sysinfo::System;
use walkdir::WalkDir;

const CACHE_DIRECTORIES: &[(&str, &str, &str)] = &[
    ("cache", "网页缓存", "Cache"),
    ("code_cache", "代码缓存", "Code Cache"),
    ("gpu_cache", "GPU 缓存", "GPUCache"),
    (
        "service_worker_cache",
        "Service Worker 缓存",
        "Service Worker\\CacheStorage",
    ),
    ("dawn_cache", "图形缓存", "DawnCache"),
    ("shader_cache", "着色器缓存", "GrShaderCache"),
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserInfo {
    pub id: String,
    pub name: String,
    pub profile_count: usize,
    pub running: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BrowserCleanupKind {
    Cache,
    Extension,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserCleanupItem {
    pub id: String,
    pub browser_id: String,
    pub browser_name: String,
    pub profile: String,
    pub kind: BrowserCleanupKind,
    pub name: String,
    pub description: String,
    pub path: String,
    pub size: u64,
    pub selected_by_default: bool,
    pub confidence: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserScanResult {
    pub browsers: Vec<BrowserInfo>,
    pub items: Vec<BrowserCleanupItem>,
    pub total_size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserCleanupRequest {
    pub item_ids: Vec<String>,
    pub dry_run: bool,
    pub confirm: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserCleanupOutcome {
    pub item_id: String,
    pub name: String,
    pub success: bool,
    pub bytes_freed: u64,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserCleanupResult {
    pub dry_run: bool,
    pub outcomes: Vec<BrowserCleanupOutcome>,
    pub bytes_freed: u64,
}

struct BrowserDefinition {
    id: &'static str,
    name: &'static str,
    relative_user_data: &'static str,
    process_names: &'static [&'static str],
}

const BROWSERS: &[BrowserDefinition] = &[
    BrowserDefinition {
        id: "chrome",
        name: "Google Chrome",
        relative_user_data: "Google\\Chrome\\User Data",
        process_names: &["chrome.exe", "chrome"],
    },
    BrowserDefinition {
        id: "edge",
        name: "Microsoft Edge",
        relative_user_data: "Microsoft\\Edge\\User Data",
        process_names: &["msedge.exe", "msedge"],
    },
    BrowserDefinition {
        id: "brave",
        name: "Brave",
        relative_user_data: "BraveSoftware\\Brave-Browser\\User Data",
        process_names: &["brave.exe", "brave"],
    },
];

pub fn scan_browser_data() -> Result<BrowserScanResult, String> {
    let local_app_data = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .ok_or_else(|| "无法读取 LOCALAPPDATA，不能定位浏览器数据".to_string())?;
    let running_processes = running_process_names();
    let mut browsers = Vec::new();
    let mut items = Vec::new();

    for browser in BROWSERS {
        let user_data = local_app_data.join(browser.relative_user_data);
        if !user_data.is_dir() {
            continue;
        }
        let profiles = discover_profiles(&user_data)?;
        let running = browser
            .process_names
            .iter()
            .any(|name| running_processes.contains(&name.to_ascii_lowercase()));

        for profile in &profiles {
            scan_profile(browser, profile, &mut items);
        }
        browsers.push(BrowserInfo {
            id: browser.id.to_string(),
            name: browser.name.to_string(),
            profile_count: profiles.len(),
            running,
        });
    }

    let total_size = items.iter().map(|item| item.size).sum();
    Ok(BrowserScanResult {
        browsers,
        items,
        total_size,
    })
}

pub fn clean_browser_data(request: &BrowserCleanupRequest) -> Result<BrowserCleanupResult, String> {
    if request.item_ids.is_empty() {
        return Ok(BrowserCleanupResult {
            dry_run: request.dry_run,
            outcomes: Vec::new(),
            bytes_freed: 0,
        });
    }
    if !request.dry_run && !request.confirm {
        return Err("实际清理需要用户明确确认".to_string());
    }

    // 执行前重新扫描，并只接受本次扫描生成的 ID，前端无法构造任意删除路径。
    let scan = scan_browser_data()?;
    let requested: HashSet<&str> = request.item_ids.iter().map(String::as_str).collect();
    let known: HashMap<&str, &BrowserCleanupItem> = scan
        .items
        .iter()
        .map(|item| (item.id.as_str(), item))
        .collect();
    if let Some(unknown) = requested.iter().find(|id| !known.contains_key(**id)) {
        return Err(format!("清理项目已失效或不受支持，请重新扫描：{unknown}"));
    }

    let selected_browser_ids: HashSet<&str> = requested
        .iter()
        .filter_map(|id| known.get(id).map(|item| item.browser_id.as_str()))
        .collect();
    if let Some(browser) = scan
        .browsers
        .iter()
        .find(|browser| browser.running && selected_browser_ids.contains(browser.id.as_str()))
    {
        return Err(format!(
            "{} 正在运行，请完全退出浏览器后再清理",
            browser.name
        ));
    }

    let mut outcomes = Vec::new();
    for item_id in &request.item_ids {
        let Some(item) = known.get(item_id.as_str()) else {
            continue;
        };
        if request.dry_run {
            outcomes.push(BrowserCleanupOutcome {
                item_id: item.id.clone(),
                name: item.name.clone(),
                success: true,
                bytes_freed: item.size,
                error: None,
            });
            continue;
        }

        // 仅删除由扫描器白名单生成的目录；单项失败不会中止其余项目，并完整返回错误。
        let mut bytes_freed = 0;
        let mut errors = Vec::new();
        for path in cleanup_paths(item) {
            if !path.exists() {
                continue;
            }
            let size = directory_size(&path);
            match fs::remove_dir_all(&path) {
                Ok(()) => bytes_freed += size,
                Err(error) => errors.push(format!("{}: {error}", path.display())),
            }
        }
        outcomes.push(BrowserCleanupOutcome {
            item_id: item.id.clone(),
            name: item.name.clone(),
            success: errors.is_empty(),
            bytes_freed,
            error: (!errors.is_empty()).then(|| errors.join("；")),
        });
    }

    let bytes_freed = outcomes.iter().map(|outcome| outcome.bytes_freed).sum();
    Ok(BrowserCleanupResult {
        dry_run: request.dry_run,
        outcomes,
        bytes_freed,
    })
}

fn running_process_names() -> HashSet<String> {
    let mut system = System::new_all();
    system.refresh_all();
    system
        .processes()
        .values()
        .map(|process| process.name().to_string_lossy().to_ascii_lowercase())
        .collect()
}

fn discover_profiles(user_data: &Path) -> Result<Vec<PathBuf>, String> {
    let entries = fs::read_dir(user_data)
        .map_err(|error| format!("无法读取浏览器用户目录 {}: {error}", user_data.display()))?;
    let mut profiles = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if path.is_dir() && (name == "Default" || name.starts_with("Profile ")) {
            profiles.push(path);
        }
    }
    profiles.sort();
    Ok(profiles)
}

fn scan_profile(
    browser: &BrowserDefinition,
    profile_path: &Path,
    items: &mut Vec<BrowserCleanupItem>,
) {
    let profile = profile_path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "Default".to_string());

    for (kind_id, label, relative_path) in CACHE_DIRECTORIES {
        let path = profile_path.join(relative_path);
        if !path.is_dir() || !is_safe_child(profile_path, &path) {
            continue;
        }
        items.push(BrowserCleanupItem {
            id: format!("{}:{}:cache:{}", browser.id, profile, kind_id),
            browser_id: browser.id.to_string(),
            browser_name: browser.name.to_string(),
            profile: profile.clone(),
            kind: BrowserCleanupKind::Cache,
            name: (*label).to_string(),
            description: "可安全重建的临时浏览器数据".to_string(),
            path: path.to_string_lossy().into_owned(),
            size: directory_size(&path),
            selected_by_default: true,
            confidence: "high".to_string(),
        });
    }

    let extensions_root = profile_path.join("Extensions");
    let Ok(extension_entries) = fs::read_dir(&extensions_root) else {
        return;
    };
    for extension_entry in extension_entries.flatten() {
        let extension_path = extension_entry.path();
        if !extension_path.is_dir() || !is_safe_child(profile_path, &extension_path) {
            continue;
        }
        let extension_id = extension_entry.file_name().to_string_lossy().into_owned();
        let (name, version) = extension_metadata(&extension_path, &extension_id);
        let related_paths = extension_cleanup_paths(profile_path, &extension_path);
        let size = related_paths.iter().map(|path| directory_size(path)).sum();
        items.push(BrowserCleanupItem {
            id: format!("{}:{}:extension:{}", browser.id, profile, extension_id),
            browser_id: browser.id.to_string(),
            browser_name: browser.name.to_string(),
            profile: profile.clone(),
            kind: BrowserCleanupKind::Extension,
            name,
            description: format!(
                "扩展 ID: {extension_id} · 版本 {version}；包含本地设置，同步可能会重新安装"
            ),
            path: extension_path.to_string_lossy().into_owned(),
            size,
            selected_by_default: false,
            confidence: "medium".to_string(),
        });
    }
}

fn cleanup_paths(item: &BrowserCleanupItem) -> Vec<PathBuf> {
    let primary = PathBuf::from(&item.path);
    if item.kind != BrowserCleanupKind::Extension {
        return vec![primary];
    }
    let Some(profile_path) = primary.parent().and_then(Path::parent) else {
        return vec![primary];
    };
    extension_cleanup_paths(profile_path, &primary)
}

fn extension_cleanup_paths(profile_path: &Path, extension_path: &Path) -> Vec<PathBuf> {
    let Some(extension_id) = extension_path.file_name() else {
        return vec![extension_path.to_path_buf()];
    };
    let mut paths = vec![extension_path.to_path_buf()];
    for container in [
        "Local Extension Settings",
        "Sync Extension Settings",
        "Extension Rules",
        "Extension Scripts",
    ] {
        let path = profile_path.join(container).join(extension_id);
        if path.is_dir() && is_safe_child(profile_path, &path) {
            paths.push(path);
        }
    }
    paths
}

fn is_safe_child(root: &Path, candidate: &Path) -> bool {
    let Ok(root) = fs::canonicalize(root) else {
        return false;
    };
    let Ok(candidate) = fs::canonicalize(candidate) else {
        return false;
    };
    candidate.starts_with(&root) && candidate != root
}

fn extension_metadata(extension_path: &Path, extension_id: &str) -> (String, String) {
    let Ok(entries) = fs::read_dir(extension_path) else {
        return (extension_id.to_string(), "未知".to_string());
    };
    let mut versions: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
    versions.sort();
    versions.reverse();
    for version_path in versions {
        let Ok(contents) = fs::read_to_string(version_path.join("manifest.json")) else {
            continue;
        };
        let Ok(manifest) = serde_json::from_str::<serde_json::Value>(&contents) else {
            continue;
        };
        let name = manifest
            .get("name")
            .and_then(serde_json::Value::as_str)
            .filter(|name| !name.starts_with("__MSG_"))
            .unwrap_or(extension_id)
            .to_string();
        let version = manifest
            .get("version")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("未知")
            .to_string();
        return (name, version);
    }
    (extension_id.to_string(), "未知".to_string())
}

fn directory_size(path: &Path) -> u64 {
    WalkDir::new(path)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
        .filter_map(|entry| entry.metadata().ok())
        .filter(|metadata| metadata.is_file())
        .map(|metadata| metadata.len())
        .sum()
}

#[cfg(test)]
mod tests {
    use super::{directory_size, discover_profiles, extension_cleanup_paths, extension_metadata};
    use std::fs;
    use std::path::PathBuf;
    use uuid::Uuid;

    fn test_directory(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("rust-yu-{name}-{}", Uuid::new_v4()))
    }

    #[test]
    fn discovers_only_chromium_profiles() -> Result<(), Box<dyn std::error::Error>> {
        let root = test_directory("profiles");
        fs::create_dir_all(root.join("Default"))?;
        fs::create_dir_all(root.join("Profile 2"))?;
        fs::create_dir_all(root.join("System Profile"))?;
        let profiles = discover_profiles(&root)?;
        assert_eq!(profiles.len(), 2);
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn reads_extension_manifest_and_measures_files() -> Result<(), Box<dyn std::error::Error>> {
        let root = test_directory("extension");
        let version = root.join("1.2.3_0");
        fs::create_dir_all(&version)?;
        fs::write(
            version.join("manifest.json"),
            r#"{"name":"Test Extension","version":"1.2.3"}"#,
        )?;
        fs::write(version.join("payload.bin"), [0_u8; 16])?;
        let (name, version) = extension_metadata(&root, "extension-id");
        assert_eq!(name, "Test Extension");
        assert_eq!(version, "1.2.3");
        assert!(directory_size(&root) >= 16);
        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn extension_cleanup_is_scoped_to_selected_id() -> Result<(), Box<dyn std::error::Error>> {
        let profile = test_directory("extension-data");
        let extension = profile.join("Extensions").join("selected-id");
        fs::create_dir_all(&extension)?;
        fs::create_dir_all(profile.join("Local Extension Settings").join("selected-id"))?;
        fs::create_dir_all(profile.join("Local Extension Settings").join("another-id"))?;
        let paths = extension_cleanup_paths(&profile, &extension);
        assert_eq!(paths.len(), 2);
        assert!(paths.iter().all(|path| !path.ends_with("another-id")));
        fs::remove_dir_all(profile)?;
        Ok(())
    }
}
