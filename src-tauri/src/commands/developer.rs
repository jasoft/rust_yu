use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, State};
use tokio::process::Command;
use tokio::sync::Mutex;
use tokio::time::timeout;

use super::{require_administrator, CommandError};

const INSTALL_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Debug, Default)]
pub struct DeveloperFixtureInstaller {
    operation: Mutex<()>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DeveloperFixtureKind {
    Msi,
    Inno,
    Nsis,
    /// PowerShell 脚本夹具（如故意留痕的卸载测试夹具）。
    Script,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeveloperFixture {
    pub id: String,
    pub kind: DeveloperFixtureKind,
    pub path: String,
    pub available: bool,
    pub size: Option<u64>,
    /// 该夹具当前是否已安装在系统中（按 ARP 卸载键判断）。
    pub installed: bool,
}

#[derive(Debug, Deserialize)]
pub struct InstallDeveloperFixturesRequest {
    pub fixture_ids: Vec<String>,
    pub confirm: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum FixtureInstallStatus {
    Installed,
    RebootRequired,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
pub struct FixtureInstallResult {
    pub fixture_id: String,
    pub status: FixtureInstallStatus,
    pub exit_code: Option<i32>,
    pub error_code: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct InstallDeveloperFixturesResponse {
    pub results: Vec<FixtureInstallResult>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DeveloperFixtureInstallProgress {
    pub fixture_id: String,
    pub index: usize,
    pub total: usize,
    pub phase: String,
    pub status: Option<FixtureInstallStatus>,
}

#[derive(Debug, Clone, Copy)]
struct FixtureDefinition {
    id: &'static str,
    kind: DeveloperFixtureKind,
    relative_path: &'static str,
    /// 判断夹具是否已安装的 ARP 卸载键列表（形如 "HKLM\SOFTWARE\..."）。
    /// 多数安装器在 WOW64 视图还会有镜像键，命中任意一个即视为已安装。
    installed_keys: &'static [&'static str],
}

const FIXTURES: &[FixtureDefinition] = &[
    FixtureDefinition {
        id: "xplorer-msi",
        kind: DeveloperFixtureKind::Msi,
        relative_path: "Xplorer_0.3.1_x64.msi",
        installed_keys: &[
            r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\{93CA8C7C-F9B0-4FA3-B0EE-7DFDE701112A}",
            r"HKLM\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\{93CA8C7C-F9B0-4FA3-B0EE-7DFDE701112A}",
        ],
    },
    FixtureDefinition {
        id: "legacy-inno",
        kind: DeveloperFixtureKind::Inno,
        relative_path: "inno-legacy-test/output/RustYuLegacyTestSetup.exe",
        installed_keys: &[
            r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\rust_yu_legacy_test_app_is1",
            r"HKLM\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\rust_yu_legacy_test_app_is1",
        ],
    },
    FixtureDefinition {
        id: "inno-dirty",
        kind: DeveloperFixtureKind::Inno,
        relative_path: "inno-dirty-traces/output/RustYuDirtyTracesSetup.exe",
        installed_keys: &[
            r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\rust_yu_dirty_traces_is1",
            r"HKLM\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\rust_yu_dirty_traces_is1",
        ],
    },
    FixtureDefinition {
        id: "deep-nsis-machine",
        kind: DeveloperFixtureKind::Nsis,
        relative_path: "nsis-random-fixtures/output/RustYuDeepScanMachineSetup.exe",
        installed_keys: &[r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\RustYuDeepScanMachine"],
    },
    FixtureDefinition {
        id: "deep-nsis-user",
        kind: DeveloperFixtureKind::Nsis,
        relative_path: "nsis-random-fixtures/output/RustYuDeepScanUserSetup.exe",
        installed_keys: &[r"HKCU\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\RustYuDeepScanUser"],
    },
    FixtureDefinition {
        id: "deep-nsis-sparse",
        kind: DeveloperFixtureKind::Nsis,
        relative_path: "nsis-random-fixtures/output/RustYuDeepScanSparseSetup.exe",
        installed_keys: &[r"HKCU\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\RustYuDeepScanSparse"],
    },
    FixtureDefinition {
        id: "leftover-machine",
        kind: DeveloperFixtureKind::Script,
        relative_path: "leftover-trace-fixtures/Install-LeftoverFixtures.ps1",
        installed_keys: &[r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\RustYuLeftoverMachine"],
    },
    FixtureDefinition {
        id: "leftover-user",
        kind: DeveloperFixtureKind::Script,
        relative_path: "leftover-trace-fixtures/Install-LeftoverFixtures.ps1",
        installed_keys: &[r"HKCU\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\RustYuLeftoverUser"],
    },
    FixtureDefinition {
        id: "leftover-sparse",
        kind: DeveloperFixtureKind::Script,
        relative_path: "leftover-trace-fixtures/Install-LeftoverFixtures.ps1",
        installed_keys: &[r"HKCU\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\RustYuLeftoverSparse"],
    },
];

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")))
        .join(".resources")
}

fn fixture_path(definition: FixtureDefinition) -> PathBuf {
    fixture_root().join(definition.relative_path)
}

/// `canonicalize` 在 Windows 上会返回 `\\?\C:\...`。Windows Installer 对这种
/// 扩展路径并不稳定，常以 1619（无法打开安装包）失败，因此启动外部安装器前恢复
/// 为普通 DOS/UNC 路径；白名单校验仍使用 canonical path，不降低路径安全边界。
fn installer_argument_path(path: &Path) -> PathBuf {
    let display = path.as_os_str().to_string_lossy();
    if let Some(unc_path) = display.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{unc_path}"));
    }
    display
        .strip_prefix(r"\\?\")
        .map_or_else(|| path.to_path_buf(), PathBuf::from)
}

/// 按 ARP 卸载键判断夹具是否已安装。命名注册表视图（WOW64 镜像）下的
/// 键也算命中，避免 32 位安装器装的夹具被误报为未安装。
fn registry_key_exists(key_path: &str) -> bool {
    let Some((hive_str, subkey)) = key_path.split_once('\\') else {
        return false;
    };
    let hive = match hive_str.to_ascii_uppercase().as_str() {
        "HKLM" | "HKEY_LOCAL_MACHINE" => winreg::enums::HKEY_LOCAL_MACHINE,
        "HKCU" | "HKEY_CURRENT_USER" => winreg::enums::HKEY_CURRENT_USER,
        _ => return false,
    };
    winreg::RegKey::predef(hive).open_subkey(subkey).is_ok()
}

fn fixture_catalog() -> Vec<DeveloperFixture> {
    FIXTURES
        .iter()
        .copied()
        .map(|definition| {
            let path = fixture_path(definition);
            let metadata = std::fs::metadata(&path).ok();
            DeveloperFixture {
                id: definition.id.to_string(),
                kind: definition.kind,
                path: path.to_string_lossy().into_owned(),
                available: metadata.as_ref().is_some_and(std::fs::Metadata::is_file),
                size: metadata.map(|value| value.len()),
                installed: definition
                    .installed_keys
                    .iter()
                    .any(|key| registry_key_exists(key)),
            }
        })
        .collect()
}

fn build_install_plan(
    request: &InstallDeveloperFixturesRequest,
) -> Result<Vec<(FixtureDefinition, PathBuf)>, CommandError> {
    if !request.confirm {
        return Err(CommandError::with_code(
            "developer_fixture_confirmation_required",
            "Fixture installation requires explicit confirmation",
        ));
    }
    if request.fixture_ids.is_empty() {
        return Err(CommandError::with_code(
            "developer_fixture_selection_required",
            "Select at least one fixture",
        ));
    }

    let mut requested = HashSet::new();
    for fixture_id in &request.fixture_ids {
        if !requested.insert(fixture_id.as_str()) {
            return Err(CommandError::with_code(
                "developer_fixture_duplicate",
                "The fixture selection contains duplicates",
            ));
        }
        if !FIXTURES
            .iter()
            .any(|definition| definition.id == fixture_id)
        {
            return Err(CommandError::with_code(
                "developer_fixture_unknown",
                "The requested fixture is not in the developer allowlist",
            ));
        }
    }

    let canonical_root = std::fs::canonicalize(fixture_root()).map_err(|_| {
        CommandError::with_code(
            "developer_fixture_root_missing",
            "The repository fixture directory is unavailable",
        )
    })?;
    let mut plan = Vec::with_capacity(requested.len());
    for definition in FIXTURES
        .iter()
        .copied()
        .filter(|definition| requested.contains(definition.id))
    {
        let canonical_path = std::fs::canonicalize(fixture_path(definition)).map_err(|_| {
            CommandError::with_code(
                "developer_fixture_missing",
                "A selected fixture installer is unavailable",
            )
        })?;
        if !canonical_path.starts_with(&canonical_root) || !canonical_path.is_file() {
            return Err(CommandError::with_code(
                "developer_fixture_path_invalid",
                "The selected fixture path failed validation",
            ));
        }
        plan.push((definition, canonical_path));
    }
    Ok(plan)
}

#[tauri::command]
pub async fn list_developer_fixtures() -> Result<Vec<DeveloperFixture>, CommandError> {
    tauri::async_runtime::spawn_blocking(fixture_catalog)
        .await
        .map_err(|error| {
            CommandError::with_code(
                "developer_fixture_catalog_failed",
                format!("Fixture catalog task failed: {error}"),
            )
        })
}

#[tauri::command]
pub async fn install_developer_fixtures(
    app: AppHandle,
    installer: State<'_, DeveloperFixtureInstaller>,
    request: InstallDeveloperFixturesRequest,
) -> Result<InstallDeveloperFixturesResponse, CommandError> {
    require_administrator()?;
    let _operation = installer.operation.try_lock().map_err(|_| {
        CommandError::with_code(
            "developer_fixture_install_busy",
            "Another fixture installation is already running",
        )
    })?;
    let plan = tauri::async_runtime::spawn_blocking(move || build_install_plan(&request))
        .await
        .map_err(|error| {
            CommandError::with_code(
                "developer_fixture_plan_failed",
                format!("Fixture installation plan failed: {error}"),
            )
        })??;

    let total = plan.len();
    let mut results = Vec::with_capacity(total);
    for (offset, (definition, path)) in plan.into_iter().enumerate() {
        let index = offset + 1;
        let _ = app.emit(
            "developer-fixture-install-progress",
            DeveloperFixtureInstallProgress {
                fixture_id: definition.id.to_string(),
                index,
                total,
                phase: "installing".to_string(),
                status: None,
            },
        );

        let result = run_fixture_installer(definition, &path).await;
        let _ = app.emit(
            "developer-fixture-install-progress",
            DeveloperFixtureInstallProgress {
                fixture_id: definition.id.to_string(),
                index,
                total,
                phase: "completed".to_string(),
                status: Some(result.status),
            },
        );
        results.push(result);
    }

    Ok(InstallDeveloperFixturesResponse { results })
}

async fn run_fixture_installer(definition: FixtureDefinition, path: &Path) -> FixtureInstallResult {
    let installer_path = installer_argument_path(path);
    let mut command = match definition.kind {
        DeveloperFixtureKind::Msi => {
            let mut command = Command::new("msiexec.exe");
            command.args(["/i"]);
            command.arg(&installer_path);
            command.args(["/qn", "/norestart"]);
            command
        }
        DeveloperFixtureKind::Inno => {
            let mut command = Command::new(&installer_path);
            command.args(["/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART", "/SP-"]);
            command
        }
        DeveloperFixtureKind::Nsis => {
            let mut command = Command::new(&installer_path);
            command.arg("/S");
            command
        }
        DeveloperFixtureKind::Script => {
            // 留痕夹具是 PowerShell 脚本：显式使用 System32 的 Windows PowerShell
            // 并只安装当前夹具。不要用 MSIX 打包的 pwsh——它的注册表虚拟化沙箱
            // 会让 HKLM 写入出现"成功但不落盘"的假象。
            let powershell = std::env::var("SystemRoot")
                .map(|root| Path::new(&root).join(r"System32\WindowsPowerShell\v1.0\powershell.exe"))
                .unwrap_or_else(|_| Path::new(r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe").to_path_buf());
            let mut command = Command::new(powershell);
            command.args([
                "-NoProfile",
                "-ExecutionPolicy",
                "Bypass",
                "-WindowStyle",
                "Hidden",
                "-File",
            ]);
            command.arg(&installer_path);
            command.args(["-FixtureIds", definition.id]);
            command
        }
    };
    command.kill_on_drop(true);

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(_) => {
            return failed_result(definition.id, None, "developer_fixture_launch_failed");
        }
    };
    let exit_status = match timeout(INSTALL_TIMEOUT, child.wait()).await {
        Ok(Ok(status)) => status,
        Ok(Err(_)) => {
            return failed_result(definition.id, None, "developer_fixture_wait_failed");
        }
        Err(_) => {
            let _ = child.kill().await;
            return failed_result(definition.id, None, "developer_fixture_timed_out");
        }
    };
    let exit_code = exit_status.code();
    let status = match exit_code {
        Some(0) => FixtureInstallStatus::Installed,
        Some(1641 | 3010) => FixtureInstallStatus::RebootRequired,
        _ => FixtureInstallStatus::Failed,
    };
    FixtureInstallResult {
        fixture_id: definition.id.to_string(),
        status,
        exit_code,
        error_code: (status == FixtureInstallStatus::Failed)
            .then(|| "developer_fixture_installer_failed".to_string()),
    }
}

fn failed_result(
    fixture_id: &str,
    exit_code: Option<i32>,
    error_code: &str,
) -> FixtureInstallResult {
    FixtureInstallResult {
        fixture_id: fixture_id.to_string(),
        status: FixtureInstallStatus::Failed,
        exit_code,
        error_code: Some(error_code.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_install_plan, fixture_catalog, installer_argument_path,
        InstallDeveloperFixturesRequest, FIXTURES,
    };
    use std::path::Path;

    #[test]
    fn catalog_contains_only_the_fixed_installable_fixtures() {
        let catalog = fixture_catalog();
        assert_eq!(catalog.len(), 9);
        assert_eq!(catalog[0].id, "xplorer-msi");
        assert_eq!(catalog[1].id, "legacy-inno");
        assert_eq!(catalog[2].id, "inno-dirty");
        assert_eq!(catalog[3].id, "deep-nsis-machine");
        assert_eq!(catalog[4].id, "deep-nsis-user");
        assert_eq!(catalog[5].id, "deep-nsis-sparse");
        assert_eq!(catalog[6].id, "leftover-machine");
        assert_eq!(catalog[7].id, "leftover-user");
        assert_eq!(catalog[8].id, "leftover-sparse");
        assert!(catalog.iter().all(|fixture| fixture.available));
        assert!(catalog.iter().all(|fixture| fixture.size.is_some()));
    }

    #[test]
    fn every_fixture_declares_an_install_detection_key() {
        for definition in FIXTURES {
            assert!(
                !definition.installed_keys.is_empty(),
                "fixture {} must declare at least one ARP key for installed detection",
                definition.id
            );
            for key in definition.installed_keys {
                let hive = key.split('\\').next().unwrap_or("");
                assert!(
                    matches!(hive, "HKLM" | "HKCU"),
                    "fixture {} uses unsupported hive in {key}",
                    definition.id
                );
                assert!(
                    key.contains(r"\Uninstall\"),
                    "fixture {} detection key must point at an ARP uninstall key: {key}",
                    definition.id
                );
            }
        }
    }

    #[test]
    fn installation_requires_confirmation_and_selection() {
        let missing_confirmation = build_install_plan(&InstallDeveloperFixturesRequest {
            fixture_ids: vec![FIXTURES[0].id.to_string()],
            confirm: false,
        });
        assert_eq!(
            missing_confirmation.err().and_then(|error| error.code),
            Some("developer_fixture_confirmation_required".to_string())
        );

        let empty = build_install_plan(&InstallDeveloperFixturesRequest {
            fixture_ids: Vec::new(),
            confirm: true,
        });
        assert_eq!(
            empty.err().and_then(|error| error.code),
            Some("developer_fixture_selection_required".to_string())
        );
    }

    #[test]
    fn installation_rejects_unknown_and_duplicate_ids() {
        let unknown = build_install_plan(&InstallDeveloperFixturesRequest {
            fixture_ids: vec!["outside-allowlist".to_string()],
            confirm: true,
        });
        assert_eq!(
            unknown.err().and_then(|error| error.code),
            Some("developer_fixture_unknown".to_string())
        );

        let duplicate = build_install_plan(&InstallDeveloperFixturesRequest {
            fixture_ids: vec![FIXTURES[0].id.to_string(), FIXTURES[0].id.to_string()],
            confirm: true,
        });
        assert_eq!(
            duplicate.err().and_then(|error| error.code),
            Some("developer_fixture_duplicate".to_string())
        );
    }

    #[test]
    fn installation_uses_stable_catalog_order() {
        let plan = build_install_plan(&InstallDeveloperFixturesRequest {
            fixture_ids: vec![FIXTURES[1].id.to_string(), FIXTURES[0].id.to_string()],
            confirm: true,
        })
        .unwrap_or_else(|error| panic!("fixture plan should be valid: {error}"));

        assert_eq!(plan[0].0.id, FIXTURES[0].id);
        assert_eq!(plan[1].0.id, FIXTURES[1].id);
    }

    #[test]
    fn installer_arguments_do_not_use_windows_verbatim_paths() {
        assert_eq!(
            installer_argument_path(Path::new(r"\\?\C:\fixtures\demo.msi")),
            Path::new(r"C:\fixtures\demo.msi")
        );
        assert_eq!(
            installer_argument_path(Path::new(r"\\?\UNC\server\fixtures\demo.msi")),
            Path::new(r"\\server\fixtures\demo.msi")
        );
    }
}
