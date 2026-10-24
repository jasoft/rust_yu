//! 脏卸载夹具的完整清理与恢复闭环验收（RustYu 自身能力，不借助 GUI）。
//!
//! 前置：管理员 PowerShell + 已构建的 inno-dirty 夹具安装包。
//! 运行：`cargo test --test dirty_fixture_workflow -- --ignored --nocapture`
//!
//! 覆盖链路（全部带独立证据）：
//! 安装夹具 → 脏卸载 → 扫描 → **白名单预检**（越界整批拒绝）→
//! **独立快照**（文件 SHA256 / 注册表原始类型与字节，落 run-id 证据目录；
//! 快照写入失败即中止——证据持久化失败不得继续删除）→
//! **只读 dry-run**（plan_for_traces 前后快照零修改）→
//! 清理轮 1（备份+删除，文件/注册表）→ 复扫 → **显式目录候选**（空目录
//! 走同一套归属/重解析点边界/dry-run/备份通道，不恢复隐式父链）→
//! 清理轮 2 → 终扫 → **恢复冲突负例**（后写入的用户数据不被覆盖）→
//! 恢复 → **按独立快照逐项核验**（非存在性）→ 证据归档 → Reset 清场。
//!
//! 未完成项（如实保留，不因测试通过而消失）：
//! - 服务/计划任务：无配置级备份恢复能力，生产清理入口已默认拒删
//!   （tests/service_task_backup_gate.rs 回归），本测试保持 deferred；
//! - Session Manager\Environment 应用变量：精确归属规则待实现，拒删；
//! - 安装目录本体空壳：无目录级 trace 产出，待显式目录候选扩展。
//!
//! 证据：docs/testing/run-logs/dirty-lifecycle-<run-id>/（快照、dry-run
//! 对比、清理与恢复结果、备份 manifest 副本）。测试成功不删证据目录；
//! 失败时隔离存储根同样保留。

#![cfg(windows)]

use rust_yu_lib::modules::backup::{list_sessions, plan_for_traces, restore_session};
use rust_yu_lib::modules::cleaner::clean_reviewed_traces;
use rust_yu_lib::modules::lister;
use rust_yu_lib::modules::lister::models::{InstallSourceSelector, ListProgramsQuery};
use rust_yu_lib::modules::scanner;
use rust_yu_lib::modules::scanner::models::{Trace, TraceType};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::process::Command;

const APP_NAME: &str = "RustYu Dirty Traces Test App";
const INSTALL_DIR: &str = r"C:\Program Files\RustYu Dirty Traces Test App";
const SERVICE_NAME: &str = "RustYuDirtyTraces";
const TASK_NAME: &str = "RustYuDirtyTracesStartTask";
const ARP_SUBKEY: &str =
    r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\rust_yu_dirty_traces_is1";

// ---------- 基础设施 ----------

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn run_powershell(script: &str) -> (bool, String) {
    let output = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command"])
        .arg(script)
        .output()
        .expect("应能启动 powershell");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    (output.status.success(), text)
}

fn assert_admin() {
    let (ok, text) = run_powershell(
        "([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)",
    );
    assert!(ok && text.trim().eq_ignore_ascii_case("true"),
        "本测试需要管理员 PowerShell 运行（服务/HKLM/防火墙操作）");
}

fn registry_key_exists(hive: &str, subkey: &str) -> bool {
    let script = format!("Test-Path -LiteralPath '{hive}:\\{subkey}'");
    let (ok, text) = run_powershell(&script);
    ok && text.trim().eq_ignore_ascii_case("true")
}

fn registry_value_exists(hive: &str, subkey: &str, value: &str) -> bool {
    let script = format!(
        "$null -ne (Get-ItemProperty -LiteralPath '{hive}:\\{subkey}' -Name '{value}' -ErrorAction SilentlyContinue)"
    );
    let (ok, text) = run_powershell(&script);
    ok && text.trim().eq_ignore_ascii_case("true")
}

fn service_exists() -> bool {
    let (ok, text) = run_powershell(&format!(
        "$null -ne (Get-Service -Name '{SERVICE_NAME}' -ErrorAction SilentlyContinue)"
    ));
    ok && text.trim().eq_ignore_ascii_case("true")
}

fn task_exists() -> bool {
    let status = Command::new("schtasks.exe")
        .args(["/Query", "/TN", TASK_NAME])
        .output();
    matches!(&status, Ok(output) if output.status.success())
}

fn reset_fixture() {
    let reset = repository_root()
        .join(r".resources\inno-dirty-traces\Reset-DirtyTracesFixture.ps1")
        .to_string_lossy()
        .into_owned();
    let (ok, text) = run_powershell(&format!(
        "& powershell.exe -NoProfile -ExecutionPolicy Bypass -File '{reset}'"
    ));
    assert!(ok, "Reset 夹具应成功: {text}");
}

fn install_fixture() {
    let installer = repository_root()
        .join(r".resources\inno-dirty-traces\output\RustYuDirtyTracesSetup.exe")
        .to_string_lossy()
        .into_owned();
    assert!(
        Path::new(&installer).is_file(),
        "缺少夹具安装包 {installer}，请先运行 Build-DirtyTracesFixture.ps1"
    );
    let status = Command::new(&installer)
        .args(["/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART", "/SP-"])
        .status()
        .expect("应能启动夹具安装包");
    assert!(status.success(), "夹具静默安装应成功");
    assert!(
        registry_key_exists("HKLM", ARP_SUBKEY),
        "安装后 ARP 键应存在"
    );
}

fn run_dirty_uninstaller() {
    let arp_key = "HKLM:\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\rust_yu_dirty_traces_is1";
    let (ok, text) = run_powershell(&format!(
        "$arp = Get-ItemProperty -LiteralPath '{arp_key}'; \
         $quiet = $arp.QuietUninstallString; \
         $exe = $quiet; $exeArgs = ''; \
         if ($quiet -match '^\"([^\"]+)\"\\s*(.*)$') {{ $exe = $Matches[1]; $exeArgs = $Matches[2] }}, \
         $p = Start-Process -FilePath $exe -ArgumentList $exeArgs -Wait -PassThru -WindowStyle Hidden; \
         for ($i = 0; $i -lt 30; $i++) {{ if (-not (Test-Path '{arp_key}')) {{ exit 0 }}; Start-Sleep -Seconds 2 }}; \
         Write-Output ('arp still present, launcher exit=' + $p.ExitCode); exit 1"
    ));
    assert!(ok, "脏卸载器应删除自身的 ARP 键: {text}");
    assert!(
        !registry_key_exists("HKLM", ARP_SUBKEY),
        "脏卸载后 ARP 键应消失"
    );
}

fn find_installed_fixture_program() -> rust_yu_lib::modules::lister::models::InstalledProgram {
    let listed = lister::list_programs_with_cache(ListProgramsQuery {
        source: InstallSourceSelector::Registry,
        search: Some(APP_NAME.to_string()),
        refresh: true,
        cache_ttl_seconds: lister::storage::DEFAULT_CACHE_TTL_SECONDS,
    })
    .expect("应能列出已安装程序");
    listed
        .programs
        .into_iter()
        .find(|program| program.name == APP_NAME)
        .unwrap_or_else(|| panic!("夹具 {APP_NAME} 应出现在已安装列表"))
}

// ---------- 证据目录与独立快照 ----------

/// run-id 证据目录：所有快照/结果/manifest 副本持久化于此，测试成功不删。
struct EvidenceDir(PathBuf);

impl EvidenceDir {
    fn new(run_id: &str) -> Self {
        let dir = repository_root()
            .join("docs")
            .join("testing")
            .join("run-logs")
            .join(run_id);
        std::fs::create_dir_all(&dir)
            .unwrap_or_else(|error| panic!("证据目录创建失败（不得继续删除）: {error}"));
        Self(dir)
    }

    fn write_json(&self, name: &str, value: &serde_json::Value) -> PathBuf {
        let path = self.0.join(name);
        let content = serde_json::to_string_pretty(value)
            .unwrap_or_else(|error| panic!("证据序列化失败（不得继续删除）: {error}"));
        std::fs::write(&path, content)
            .unwrap_or_else(|error| panic!("证据写入失败 {path:?}（不得继续删除）: {error}"));
        path
    }

    fn path(&self) -> &PathBuf {
        &self.0
    }
}

fn file_sha256(path: &Path) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    let digest = Sha256::digest(&bytes);
    Some(format!("{digest:x}"))
}

fn read_registry_value_raw(trace_path: &str) -> Option<(String, Vec<u8>)> {
    let upper = trace_path.to_uppercase();
    let (hive_str, rest) = upper.split_once('\\')?;
    let (key_path, value_display) = rest.rsplit_once('\\')?;
    let value_name = match value_display {
        "(DEFAULT)" => String::new(),
        other => other.to_string(),
    };
    // 键路径在大小写规范化中丢失原大小写——注册表打开不区分大小写，可接受
    let hive = match hive_str {
        "HKLM" => winreg::enums::HKEY_LOCAL_MACHINE,
        "HKCU" => winreg::enums::HKEY_CURRENT_USER,
        _ => return None,
    };
    let key = winreg::RegKey::predef(hive)
        .open_subkey(key_path.to_lowercase().as_str())
        .ok()?;
    let raw: winreg::RegValue = key.get_raw_value(&value_name).ok()?;
    Some((format!("{:?}", raw.vtype), raw.bytes))
}

/// 独立快照：不信任备份模块自身的 payload，测试侧直接读取文件字节哈希
/// 与注册表原始类型/字节。持久化失败即 panic——不得在无证据情况下删除。
fn capture_independent_snapshot(
    label: &str,
    traces: &[Trace],
    evidence: &EvidenceDir,
) -> serde_json::Value {
    let items: Vec<serde_json::Value> = traces
        .iter()
        .map(|trace| {
            let mut item = serde_json::json!({
                "path": trace.path,
                "trace_type": format!("{:?}", trace.trace_type),
            });
            match trace.trace_type {
                TraceType::File | TraceType::AppData | TraceType::Shortcut => {
                    let path = Path::new(&trace.path);
                    item["sha256"] = match file_sha256(path) {
                        Some(digest) => serde_json::json!(digest),
                        None => serde_json::json!(null),
                    };
                    item["size"] = path
                        .metadata()
                        .map(|meta| serde_json::json!(meta.len()))
                        .unwrap_or(serde_json::json!(null));
                }
                TraceType::RegistryValue => {
                    match read_registry_value_raw(&trace.path) {
                        Some((vtype, bytes)) => {
                            item["registry_type"] = serde_json::json!(vtype);
                            item["registry_bytes_hex"] =
                                serde_json::json!(bytes.iter().map(|b| format!("{b:02x}")).collect::<String>());
                        }
                        None => {
                            item["registry_type"] = serde_json::json!(null);
                        }
                    }
                }
                TraceType::RegistryKey => {
                    item["exists"] = serde_json::json!(true);
                }
                _ => {}
            }
            item
        })
        .collect();
    let snapshot = serde_json::json!({
        "label": label,
        "captured_at": chrono::Utc::now().to_rfc3339(),
        "items": items,
    });
    evidence.write_json(&format!("snapshot-{label}.json"), &snapshot);
    snapshot
}

/// 断言 dry-run 前后快照零修改（只读性验证）。
fn assert_snapshots_identical(before: &serde_json::Value, after: &serde_json::Value) {
    let before_items = before["items"].as_array().expect("快照结构完整");
    let after_items = after["items"].as_array().expect("快照结构完整");
    assert_eq!(before_items.len(), after_items.len(), "dry-run 不得增删候选");
    for (b, a) in before_items.iter().zip(after_items.iter()) {
        assert_eq!(b, a, "dry-run 产生了修改: {b} -> {a}");
    }
}

/// 恢复后按独立快照逐项核验（冲突豁免清单里的路径跳过并记录）。
fn verify_restored_against_snapshot(
    snapshot: &serde_json::Value,
    conflict_paths: &[String],
    evidence: &EvidenceDir,
) {
    let items = snapshot["items"].as_array().expect("快照结构完整");
    let mut report = Vec::new();
    for item in items {
        let path = item["path"].as_str().expect("路径存在").to_string();
        let conflict = conflict_paths
            .iter()
            .any(|conflict| conflict.eq_ignore_ascii_case(&path));
        let entry = match item["trace_type"].as_str().unwrap_or_default() {
            "File" | "AppData" | "Shortcut" => {
                let expected = item["sha256"].as_str();
                match (expected, file_sha256(Path::new(&path))) {
                    (Some(expected), Some(actual)) if expected == actual => {
                        format!("OK  file-sha256 {path}")
                    }
                    (None, None) => format!("MISS file {path}（快照时已缺失，幂等）"),
                    (expected, actual) => {
                        format!("FAIL file {path}: expected={expected:?} actual={actual:?}")
                    }
                }
            }
            "RegistryValue" => {
                let expected_type = item["registry_type"].as_str();
                let expected_hex = item["registry_bytes_hex"].as_str();
                match (expected_type, read_registry_value_raw(&path)) {
                    (Some(expected_type), Some((actual_type, actual_bytes))) => {
                        let actual_hex: String =
                            actual_bytes.iter().map(|b| format!("{b:02x}")).collect();
                        if expected_type == actual_type
                            && expected_hex == Some(actual_hex.as_str())
                        {
                            format!("OK  registry {path} [{actual_type}]")
                        } else {
                            format!(
                                "FAIL registry {path}: type {expected_type:?} vs {actual_type:?}"
                            )
                        }
                    }
                    (None, None) => format!("MISS registry {path}（快照时已缺失）"),
                    (expected, actual) => {
                        format!("FAIL registry {path}: {expected:?} vs {actual:?}")
                    }
                }
            }
            "RegistryKey" => {
                if registry_key_exists(
                    "HKLM",
                    path.trim_start_matches(r"HKLM\"),
                ) {
                    format!("OK  registry-key {path}")
                } else {
                    format!("FAIL registry-key {path} 未恢复")
                }
            }
            other => format!("SKIP {other} {path}"),
        };
        if conflict && entry.starts_with("FAIL") {
            report.push(format!("CONFLICT-EXPECTED {entry}（后写入的用户数据不覆盖）"));
        } else {
            report.push(entry);
        }
    }
    let text = report.join("\n");
    evidence.write_json(
        "restore-verification.json",
        &serde_json::json!({ "entries": report, "conflict_paths": conflict_paths }),
    );
    println!("[verify]\n{text}");
    assert!(
        report.iter().all(|line| {
            line.starts_with("OK") || line.starts_with("MISS") || line.starts_with("CONFLICT")
        }),
        "恢复核验存在失败项:\n{text}"
    );
}

// ---------- 夹具精确归属白名单 ----------

fn normalize_path_key(path: &str) -> String {
    path.trim_end_matches('\\').to_lowercase()
}

fn read_firewall_rule_snapshot() -> std::collections::HashMap<String, String> {
    let script = r#"
$k = Get-Item 'HKLM:\SYSTEM\CurrentControlSet\Services\SharedAccess\Parameters\FirewallPolicy\FirewallRules'
$out = @()
foreach ($n in $k.GetValueNames()) { $out += ($n -replace '=','==') + '=' + ([string]$k.GetValue($n)) }
$out -join "`n"
"#;
    let (ok, text) = run_powershell(script);
    assert!(ok, "读取防火墙规则应成功: {text}");
    let mut snapshot = std::collections::HashMap::new();
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        if let Some((name, data)) = line.split_once('=') {
            snapshot.insert(name.replace("==", "="), data.to_string());
        }
    }
    snapshot
}

struct FixtureAllowlist {
    files: Vec<String>,
    registry_keys: Vec<String>,
    registry_values: Vec<(String, String)>,
    firewall_rule_names: Vec<String>,
}

impl FixtureAllowlist {
    fn capture_firewall_baseline() -> std::collections::HashMap<String, String> {
        read_firewall_rule_snapshot()
    }

    fn from_baseline(baseline: &std::collections::HashMap<String, String>) -> Self {
        let current = read_firewall_rule_snapshot();
        let added: Vec<(String, String)> = current
            .iter()
            .filter(|(name, _)| !baseline.contains_key(*name))
            .map(|(name, data)| (name.clone(), data.clone()))
            .collect();
        println!("[allowlist] 防火墙规则安装差集: {} 条", added.len());
        assert_eq!(
            added.len(),
            1,
            "夹具应恰好新增 1 条防火墙规则（差集异常即整批失败）"
        );
        let (name, data) = &added[0];
        let data_lower = data.to_lowercase();
        assert!(
            data_lower.contains("name=rustyu dirty traces daemon")
                && data_lower.contains(r"app=c:\program files\rustyu dirty traces test app\rustyudirtydaemon.exe"),
            "差集规则必须核验 Name= 与 App= 归属: {data}"
        );

        Self {
            files: expected_fixture_file_paths(),
            registry_keys: expected_fixture_registry_keys(),
            registry_values: expected_fixture_registry_values(),
            firewall_rule_names: vec![name.clone()],
        }
    }
}

fn expected_fixture_file_paths() -> Vec<String> {
    let install_dir = INSTALL_DIR.to_string();
    let program_data = std::env::var("ProgramData").unwrap_or_default();
    let local_app_data = std::env::var("LOCALAPPDATA").unwrap_or_default();
    [
        format!(r"{install_dir}\logs\daemon-heartbeat.log"),
        format!(r"{install_dir}\logs\service-heartbeat.log"),
        format!(r"{install_dir}\RustYuDirtyDaemon.exe"),
        format!(r"{install_dir}\RustYuDirtyService.exe"),
        format!(r"{program_data}\RustYu Dirty Traces\cache\index.bin"),
        format!(r"{program_data}\RustYu Dirty Traces\register-extra.log"),
        format!(r"{program_data}\RustYu Dirty Traces\telemetry\pending.json"),
        format!(r"{program_data}\RustYu Dirty Traces\traces-manifest.json"),
        format!(r"{local_app_data}\RustYu Dirty Traces\leftover-user-state.json"),
    ]
    .into_iter()
    .map(|path| normalize_path_key(&path))
    .collect()
}

fn expected_fixture_registry_keys() -> Vec<String> {
    [r"HKLM\SOFTWARE\RustYu Fixture Labs\RustYu Dirty Traces".to_string()]
        .into_iter()
        .map(|path| normalize_path_key(&path))
        .collect()
}

fn expected_fixture_registry_values() -> Vec<(String, String)> {
    [
        (
            r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Run".to_string(),
            "RustYuDirtyTraces".to_string(),
        ),
        (
            r"HKLM\SOFTWARE\RustYu Fixture Labs\RustYu Dirty Traces".to_string(),
            "InstallRoot".to_string(),
        ),
        (
            r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths\RustYuDirtyDaemon.exe".to_string(),
            "(Default)".to_string(),
        ),
        (
            r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\App Paths\RustYuDirtyDaemon.exe".to_string(),
            "Path".to_string(),
        ),
        (
            r"HKLM\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\App Paths\RustYuDirtyDaemon.exe".to_string(),
            "(Default)".to_string(),
        ),
        (
            r"HKLM\SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\App Paths\RustYuDirtyDaemon.exe".to_string(),
            "Path".to_string(),
        ),
    ]
    .into_iter()
    .map(|(key, value)| (normalize_path_key(&key), value))
    .collect()
}

/// 按夹具白名单对单个 trace 做归属分类。
/// Ok(true)=白名单内；Ok(false)=白名单内但当前策略拒删（Service/Task/
/// Environment）；Err(())=越界候选（整批禁止删除）。
fn classify_trace(trace: &Trace, allowlist: &FixtureAllowlist) -> Result<bool, ()> {
    let normalized = normalize_path_key(&trace.path);
    match trace.trace_type {
        TraceType::File | TraceType::AppData => {
            if allowlist.files.contains(&normalized) {
                Ok(true)
            } else {
                Err(())
            }
        }
        TraceType::RegistryKey => {
            if allowlist.registry_keys.contains(&normalized) {
                Ok(true)
            } else {
                Err(())
            }
        }
        TraceType::RegistryValue => {
            let Some((key_path, value_name)) = normalized.rsplit_once('\\') else {
                return Err(());
            };
            if allowlist
                .registry_values
                .iter()
                .any(|(key, value)| *key == key_path && value.eq_ignore_ascii_case(value_name))
            {
                return Ok(true);
            }
            if key_path.ends_with(r"\firewallrules")
                && allowlist
                    .firewall_rule_names
                    .iter()
                    .any(|name| name.eq_ignore_ascii_case(value_name))
            {
                return Ok(true);
            }
            if key_path.ends_with(r"\session manager\environment")
                && value_name.eq_ignore_ascii_case("RUSTYU_DIRTY_TRACES_HOME")
            {
                return Ok(false);
            }
            Err(())
        }
        TraceType::Service => {
            if normalized == format!("service://{SERVICE_NAME}").to_lowercase() {
                Ok(false)
            } else {
                Err(())
            }
        }
        TraceType::ScheduledTask => {
            if normalized == format!("task://\\{TASK_NAME}").to_lowercase() {
                Ok(false)
            } else {
                Err(())
            }
        }
        _ => Err(()),
    }
}

fn find_reparse_escalations(traces: &[Trace]) -> Vec<String> {
    let mut violations = Vec::new();
    for trace in traces {
        if !matches!(trace.trace_type, TraceType::File | TraceType::AppData) {
            continue;
        }
        let path = PathBuf::from(&trace.path);
        let mut current = Some(path.as_path());
        while let Some(candidate) = current {
            if let Ok(metadata) = std::fs::symlink_metadata(candidate) {
                if metadata.file_type().is_symlink() {
                    violations.push(format!(
                        "{} 路径链含重解析点: {}",
                        trace.path,
                        candidate.display()
                    ));
                    break;
                }
            }
            current = candidate.parent();
        }
    }
    violations
}

fn partition_traces_by_fixture_allowlist(
    traces: &[Trace],
    allowlist: &FixtureAllowlist,
) -> (Vec<Trace>, Vec<Trace>) {
    let reparse_violations = find_reparse_escalations(traces);
    assert!(
        reparse_violations.is_empty(),
        "候选路径链包含重解析点，整批禁止删除并失败:\n{}",
        reparse_violations.join("\n")
    );

    let mut deletable = Vec::new();
    let mut deferred = Vec::new();
    for trace in traces {
        match classify_trace(trace, allowlist) {
            Ok(true) => deletable.push(trace.clone()),
            Ok(false) => {
                println!(
                    "[deferred] {:?} {}（无配置备份恢复/精确归属规则待实现）",
                    trace.trace_type, trace.path
                );
                deferred.push(trace.clone());
            }
            Err(()) => panic!(
                "扫描结果包含白名单之外的候选，整批禁止删除并失败: {:?} {}",
                trace.trace_type, trace.path
            ),
        }
    }
    (deletable, deferred)
}

// ---------- 主流程 ----------

#[tokio::test]
#[ignore = "需要管理员 PowerShell 与已构建的 inno-dirty 夹具安装包"]
async fn inno_dirty_fixture_full_cleanup_and_restore_workflow() {
    assert_admin();

    let run_id = format!(
        "dirty-lifecycle-{}",
        chrono::Utc::now().format("%Y%m%d-%H%M%S")
    );
    let evidence = EvidenceDir::new(&run_id);
    println!("[evidence] {}", evidence.path().display());

    // 隔离存储（RAII：仅恢复 env；根目录留给成功收尾清理，失败时保留为证据）
    struct EnvRestore(Option<std::ffi::OsString>);
    impl Drop for EnvRestore {
        fn drop(&mut self) {
            match self.0.take() {
                Some(value) => std::env::set_var("RUST_YU_STORAGE_DIR", value),
                None => std::env::remove_var("RUST_YU_STORAGE_DIR"),
            }
        }
    }
    let storage_root = std::env::temp_dir().join(format!("rust-yu-workflow-storage-{}", uuid::Uuid::new_v4()));
    let _env_restore = EnvRestore(std::env::var_os("RUST_YU_STORAGE_DIR"));
    std::env::set_var("RUST_YU_STORAGE_DIR", &storage_root);

    reset_fixture();
    let firewall_baseline = FixtureAllowlist::capture_firewall_baseline();
    install_fixture();
    let allowlist = FixtureAllowlist::from_baseline(&firewall_baseline);

    let program = find_installed_fixture_program();
    let listed_location = program
        .install_location
        .as_deref()
        .map(|path| path.trim_end_matches('\\'));
    assert_eq!(
        listed_location, Some(INSTALL_DIR),
        "夹具安装目录应被 lister 正确识别"
    );

    run_dirty_uninstaller();

    // ---- 扫描与白名单预检 ----
    let traces = scanner::scan_all_traces_for_program(&program, None)
        .await
        .expect("残留扫描不应失败")
        .into_iter()
        .filter(|trace| trace.exists)
        .collect::<Vec<Trace>>();
    traces.iter().for_each(|trace| {
        println!("[trace] {:?} conf={:?} {}", trace.trace_type, trace.confidence, trace.path);
    });
    assert!(!traces.is_empty(), "脏卸载后应扫描出残留");
    let (deletable, deferred) = partition_traces_by_fixture_allowlist(&traces, &allowlist);
    println!("[allowlist] 可删除 {} 项；未完成 {} 项", deletable.len(), deferred.len());
    assert!(!deletable.is_empty(), "白名单内应存在可删除残留");

    // ---- 独立快照（先于一切删除；持久化失败即中止） ----
    let snapshot = capture_independent_snapshot("before-cleanup", &deletable, &evidence);

    // ---- 只读 dry-run：计划生成前后快照必须零修改 ----
    let plan = plan_for_traces(&deletable);
    println!(
        "[dry-run] plan items={} eligible={} unsupported={}",
        plan.items.len(),
        plan.eligible_count,
        plan.unsupported_count
    );
    assert_eq!(plan.items.len(), deletable.len(), "dry-run 应覆盖全部候选");
    assert!(plan.eligible_count >= 1, "dry-run 应存在可备份项");
    let snapshot_after_dry = capture_independent_snapshot("after-dry-run", &deletable, &evidence);
    assert_snapshots_identical(&snapshot, &snapshot_after_dry);

    // ---- 清理轮 1：备份 + 删除（文件/注册表） ----
    let results = clean_reviewed_traces(deletable.clone(), true)
        .await
        .expect("reviewed 清理不应整体失败");
    assert_eq!(results.len(), deletable.len());
    let mut failures = Vec::new();
    for result in &results {
        println!(
            "[clean] success={} backup={:?} path={} error={:?}",
            result.success, result.backup_id, result.path, result.error
        );
        if !result.success {
            failures.push(format!("{} => {:?}", result.path, result.error));
        }
    }
    assert!(
        failures.is_empty(),
        "白名单内全部残留清理应成功（失败 {} 项）:\n{}",
        failures.len(),
        failures.join("\n")
    );
    evidence.write_json(
        "cleanup-round-1.json",
        &serde_json::json!({ "results": results.iter().map(|r| serde_json::json!({
            "path": r.path, "success": r.success, "backup_id": r.backup_id, "error": r.error,
        })).collect::<Vec<_>>() }),
    );

    // ---- 复扫：空目录作为显式目录候选进入轮 2 ----
    let remaining = scanner::scan_all_traces_for_program(&program, None)
        .await
        .expect("复扫不应失败")
        .into_iter()
        .filter(|trace| trace.exists)
        .collect::<Vec<Trace>>();
    let (round2_deletable, remaining_deferred) =
        partition_traces_by_fixture_allowlist(&remaining, &allowlist);
    println!(
        "[round2] 显式目录候选 {} 项；deferred {} 项",
        round2_deletable.len(),
        remaining_deferred.len()
    );
    for trace in &round2_deletable {
        println!("[round2-candidate] {:?} {}", trace.trace_type, trace.path);
    }

    if !round2_deletable.is_empty() {
        // 目录候选同样走：重解析点边界（partition 内）→ 只读 dry-run → 快照 → 备份+删除
        let dir_snapshot = capture_independent_snapshot("before-round2", &round2_deletable, &evidence);
        let dir_plan = plan_for_traces(&round2_deletable);
        let dir_snapshot_after = capture_independent_snapshot("after-round2-dry-run", &round2_deletable, &evidence);
        assert_snapshots_identical(&dir_snapshot, &dir_snapshot_after);
        assert_eq!(dir_plan.items.len(), round2_deletable.len());

        let round2_results = clean_reviewed_traces(round2_deletable.clone(), true)
            .await
            .expect("目录候选清理不应整体失败");
        for result in &round2_results {
            println!(
                "[round2-clean] success={} path={} error={:?}",
                result.success, result.path, result.error
            );
            assert!(
                result.success,
                "显式目录候选清理应成功: {} => {:?}",
                result.path, result.error
            );
        }
        evidence.write_json(
            "cleanup-round-2.json",
            &serde_json::json!({ "results": round2_results.iter().map(|r| serde_json::json!({
                "path": r.path, "success": r.success, "error": r.error,
            })).collect::<Vec<_>>() }),
        );
    }

    // ---- 终扫：只剩 deferred（服务/任务；环境变量由直接读取断言） ----
    let final_traces = scanner::scan_all_traces_for_program(&program, None)
        .await
        .expect("终扫不应失败")
        .into_iter()
        .filter(|trace| trace.exists)
        .collect::<Vec<Trace>>();
    final_traces.iter().for_each(|trace| {
        println!("[final] {:?} {}", trace.trace_type, trace.path);
    });
    let (final_deletable, final_deferred) =
        partition_traces_by_fixture_allowlist(&final_traces, &allowlist);
    assert!(
        final_deletable.is_empty(),
        "两轮清理后终扫应无白名单内残留: {:?}",
        final_deletable.iter().map(|t| t.path.clone()).collect::<Vec<_>>()
    );
    assert!(!final_deferred.is_empty(), "未完成项应保持可见");
    assert!(service_exists(), "未完成项：服务应仍在（生产闸拒删）");
    assert!(task_exists(), "未完成项：计划任务应仍在（生产闸拒删）");
    assert!(
        registry_value_exists(
            "HKLM",
            r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment",
            "RUSTYU_DIRTY_TRACES_HOME"
        ),
        "未完成项：环境变量应仍在（精确归属规则待实现）"
    );

    // ---- 恢复冲突负例：清理后同路径写入"后来的用户数据"，恢复不得覆盖 ----
    let conflict_path = PathBuf::from(
        std::env::var("ProgramData").unwrap_or_default() + r"\RustYu Dirty Traces\telemetry\pending.json",
    );
    if let Some(parent) = conflict_path.parent() {
        std::fs::create_dir_all(parent).expect("应能重建冲突目录");
    }
    std::fs::write(&conflict_path, b"later-user-data")
        .expect("应能写入后来的用户数据");
    let conflict_paths = vec![conflict_path.to_string_lossy().into_owned()];

    // ---- 恢复 ----
    let backup_id = results
        .iter()
        .find_map(|result| result.backup_id.clone())
        .expect("至少应有一个带备份的清理结果");
    let restore = restore_session(&backup_id).expect("恢复会话不应失败");
    for item in &restore.session.items {
        println!("[restore] kind={:?} state={:?}", item.kind, item.state);
    }
    // 冲突文件必须保持"后来的用户数据"（不覆盖）
    assert_eq!(
        std::fs::read(&conflict_path).unwrap_or_default(),
        b"later-user-data".to_vec(),
        "恢复不得覆盖后写入的用户数据"
    );
    evidence.write_json(
        "restore-result.json",
        &serde_json::json!({
            "session_id": backup_id,
            "items": restore.session.items.iter().map(|i| serde_json::json!({
                "path": i.original_path, "state": format!("{:?}", i.state),
            })).collect::<Vec<_>>(),
        }),
    );

    // ---- 按独立快照逐项核验（非存在性；冲突项豁免并记录） ----
    verify_restored_against_snapshot(&snapshot, &conflict_paths, &evidence);
    // 冲突路径的恢复项应为失败态而非静默跳过
    assert!(
        restore.session.items.iter().any(|item| {
            item.original_path.eq_ignore_ascii_case(conflict_paths[0].as_str())
                && matches!(
                    item.state,
                    rust_yu_lib::modules::backup::BackupItemState::RestoreFailed
                )
        }),
        "冲突路径的恢复项必须显式 RestoreFailed，不得静默跳过"
    );

    // ---- 备份 manifest 副本归档到证据目录（隔离根随后按成功收尾清理） ----
    let manifest_source = storage_root.join("backups").join(&backup_id).join("manifest.json");
    if let Ok(content) = std::fs::read_to_string(&manifest_source) {
        let _ = std::fs::write(
            evidence.path().join("backup-manifest.json"),
            content,
        );
    }

    // ---- 清场：清测试冲突文件与夹具足迹；证据目录保留 ----
    let _ = std::fs::remove_file(&conflict_path);
    reset_fixture();
    let _ = std::fs::remove_dir_all(&storage_root);
    let _ = list_sessions(); // 触达一次默认存储，确保未写入（隔离根已清）
    println!("[evidence] 完成，证据保留于 {}", evidence.path().display());
}

// ---------- 无破坏性负例：白名单与重解析点闸门必须能拒绝越界候选 ----------

fn sample_allowlist() -> FixtureAllowlist {
    FixtureAllowlist {
        files: vec![normalize_path_key(r"C:\Program Files\Demo App\a.txt")],
        registry_keys: vec![normalize_path_key(r"HKLM\SOFTWARE\Demo Vendor\Demo App")],
        registry_values: vec![(
            normalize_path_key(r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Run"),
            "DemoApp".to_string(),
        )],
        firewall_rule_names: vec!["{11111111-2222-3333-4444-555555555555}".to_string()],
    }
}

#[test]
fn allowlist_rejects_unrelated_candidates() {
    let allowlist = sample_allowlist();

    // 同容器（FirewallRules）但不在安装差集内的无关规则 GUID：必须整批拒绝
    let stranger_rule = Trace::new(
        "Demo App".to_string(),
        TraceType::RegistryValue,
        r"HKLM\SYSTEM\CurrentControlSet\Services\SharedAccess\Parameters\FirewallPolicy\FirewallRules\{99999999-8888-7777-6666-555555555555}".to_string(),
    );
    assert!(classify_trace(&stranger_rule, &allowlist).is_err());

    // 白名单外的文件路径：拒绝
    let stranger_file = Trace::new(
        "Demo App".to_string(),
        TraceType::File,
        r"C:\Program Files\Other App\b.txt".to_string(),
    );
    assert!(classify_trace(&stranger_file, &allowlist).is_err());

    // Run 容器里他人的值：拒绝
    let stranger_run = Trace::new(
        "Demo App".to_string(),
        TraceType::RegistryValue,
        r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Run\OtherVendor".to_string(),
    );
    assert!(classify_trace(&stranger_run, &allowlist).is_err());

    // 白名单内的候选：放行
    let owned = Trace::new(
        "Demo App".to_string(),
        TraceType::File,
        r"C:\Program Files\Demo App\a.txt".to_string(),
    );
    assert_eq!(classify_trace(&owned, &allowlist), Ok(true));
}

#[test]
fn reparse_check_rejects_junction_pointing_outside_fixture_tree() {
    // junction 不需要管理员权限即可创建；仅验证预检拒绝，不删除任何数据
    let root = std::env::temp_dir().join(format!(
        "rust-yu-reparse-test-{}",
        uuid::Uuid::new_v4()
    ));
    let outside = root.join("outside");
    std::fs::create_dir_all(&outside).expect("应能创建外部目录");
    std::fs::write(outside.join("payload.txt"), b"x").expect("应能写入外部文件");

    let junction = root.join("inside-junction");
    let (ok, text) = run_powershell(&format!(
        "New-Item -ItemType Junction -Path '{}' -Target '{}' | Out-Null; if ($?) {{ exit 0 }} else {{ exit 1 }}",
        junction.display(),
        outside.display()
    ));
    assert!(ok, "创建测试 junction 应成功: {text}");

    let traces = vec![Trace::new(
        "Demo App".to_string(),
        TraceType::File,
        junction.join("payload.txt").to_string_lossy().into_owned(),
    )];
    let violations = find_reparse_escalations(&traces);
    assert!(
        !violations.is_empty(),
        "含 junction 的候选必须被重解析点预检拒绝"
    );

    let normal = vec![Trace::new(
        "Demo App".to_string(),
        TraceType::File,
        outside.join("payload.txt").to_string_lossy().into_owned(),
    )];
    assert!(find_reparse_escalations(&normal).is_empty());

    let _ = std::fs::remove_dir_all(&root);
}
