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
use std::io::Write;
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
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
        ])
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
    assert!(
        ok && text.trim().eq_ignore_ascii_case("true"),
        "本测试需要管理员 PowerShell 运行（服务/HKLM/防火墙操作）"
    );
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
    let arp_key =
        "HKLM:\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\rust_yu_dirty_traces_is1";
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
        Self::at(
            repository_root()
                .join("docs")
                .join("testing")
                .join("run-logs")
                .join(run_id),
        )
    }

    /// 在指定目录建证据目录（负例测试用临时目录，不污染 run-logs）。
    fn at(dir: PathBuf) -> Self {
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

    /// 归档失败必须保留原始备份存储，不得让后续 Reset/删除存储继续执行。
    fn archive_manifest(&self, label: &str, source: &Path) {
        let content = std::fs::read_to_string(source).unwrap_or_else(|error| {
            panic!("读取 {label} 会话 manifest 失败（保留存储排查）: {error}")
        });
        let dest = self.path().join(format!("backup-manifest-{label}.json"));
        let mut output = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&dest)
            .unwrap_or_else(|error| panic!("归档 {label} manifest 失败（保留存储排查）: {error}"));
        output
            .write_all(content.as_bytes())
            .and_then(|()| output.sync_all())
            .unwrap_or_else(|error| panic!("归档 {label} manifest 失败（保留存储排查）: {error}"));
        drop(output);
        let read_back = std::fs::read_to_string(&dest)
            .unwrap_or_else(|error| panic!("读回验证 {label} manifest 失败: {error}"));
        assert_eq!(read_back, content, "{label} manifest 归档读回内容不一致");
    }
}

/// 文件 SHA256。读取失败是硬错误（拟删项必须可读），不做任何静默回退。
fn file_sha256(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path)
        .map_err(|error| format!("读取拟删文件失败 {} : {error}", path.display()))?;
    let digest = Sha256::digest(&bytes);
    Ok(format!("{digest:x}"))
}

fn read_registry_value_raw(trace_path: &str) -> Result<(String, Vec<u8>), String> {
    let upper = trace_path.to_uppercase();
    let (hive_str, rest) = upper
        .split_once('\\')
        .ok_or_else(|| format!("注册表值路径缺少 hive: {trace_path}"))?;
    let (key_path, value_display) = rest
        .rsplit_once('\\')
        .ok_or_else(|| format!("注册表值路径缺少键/值边界: {trace_path}"))?;
    let value_name = match value_display {
        "(DEFAULT)" => String::new(),
        other => other.to_string(),
    };
    let hive = match hive_str {
        "HKLM" => winreg::enums::HKEY_LOCAL_MACHINE,
        "HKCU" => winreg::enums::HKEY_CURRENT_USER,
        other => return Err(format!("不支持的 hive {other}: {trace_path}")),
    };
    // 键路径在大小写规范化中丢失原大小写——注册表打开不区分大小写，可接受
    let key = winreg::RegKey::predef(hive)
        .open_subkey(key_path.to_lowercase().as_str())
        .map_err(|error| format!("打开注册表键失败 {trace_path} : {error}"))?;
    let raw: winreg::RegValue = key
        .get_raw_value(&value_name)
        .map_err(|error| format!("读取注册表值失败 {trace_path} : {error}"))?;
    Ok((format!("{:?}", raw.vtype), raw.bytes))
}

/// 递归快照目录树：条目名、类型、文件哈希与大小。目录项的读取错误同样
/// 是硬错误——把"读目录失败"当缺失会掩盖越界/权限异常。
fn snapshot_directory_tree(path: &Path) -> Result<serde_json::Value, String> {
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|error| format!("读取目录元数据失败 {} : {error}", path.display()))?;
    if metadata.file_type().is_symlink() {
        return Err(format!(
            "目录候选是重解析点，快照拒绝跟随: {}",
            path.display()
        ));
    }
    if !metadata.is_dir() {
        let digest = file_sha256(path)?;
        return Ok(serde_json::json!({
            "kind": "file", "sha256": digest, "size": metadata.len(),
        }));
    }
    let mut entries = std::fs::read_dir(path)
        .map_err(|error| format!("读取目录失败 {} : {error}", path.display()))?;
    let mut children = Vec::new();
    let mut sorted: Vec<std::fs::DirEntry> = Vec::new();
    while let Some(entry) = entries
        .next()
        .transpose()
        .map_err(|error| format!("枚举目录项失败 {} : {error}", path.display()))?
    {
        sorted.push(entry);
    }
    sorted.sort_by_key(|entry| entry.file_name());
    for entry in sorted {
        let name = entry.file_name().to_string_lossy().into_owned();
        let child = snapshot_directory_tree(&entry.path())?;
        children.push(serde_json::json!({ "name": name, "entry": child }));
    }
    Ok(serde_json::json!({ "kind": "dir", "children": children }))
}

/// 递归快照注册表键：全部值的原始类型/字节 + 子键树（深度上限防失控）。
fn snapshot_registry_key_tree(trace_path: &str) -> Result<serde_json::Value, String> {
    fn snapshot_key(
        hive: winreg::HKEY,
        subkey: &str,
        depth: usize,
    ) -> Result<serde_json::Value, String> {
        if depth > 8 {
            return Err(format!("注册表键树深度超过上限: {subkey}"));
        }
        let key = winreg::RegKey::predef(hive)
            .open_subkey(subkey)
            .map_err(|error| format!("打开注册表键失败 {subkey} : {error}"))?;
        let mut values = Vec::new();
        for result in key.enum_values() {
            let (name, raw) =
                result.map_err(|error| format!("枚举注册表值失败 {subkey} : {error}"))?;
            values.push(serde_json::json!({
                "name": name,
                "type": format!("{:?}", raw.vtype),
                "bytes_hex": raw.bytes.iter().map(|b| format!("{b:02x}")).collect::<String>(),
            }));
        }
        let mut subkeys = Vec::new();
        for result in key.enum_keys() {
            let child = result.map_err(|error| format!("枚举子键失败 {subkey} : {error}"))?;
            let child_tree = snapshot_key(hive, &format!("{subkey}\\{child}"), depth + 1)?;
            subkeys.push(serde_json::json!({ "name": child, "tree": child_tree }));
        }
        Ok(serde_json::json!({ "values": values, "subkeys": subkeys }))
    }

    let upper = trace_path.to_uppercase();
    let (hive_str, subkey) = upper
        .split_once('\\')
        .ok_or_else(|| format!("注册表键路径缺少 hive: {trace_path}"))?;
    let hive = match hive_str {
        "HKLM" => winreg::enums::HKEY_LOCAL_MACHINE,
        "HKCU" => winreg::enums::HKEY_CURRENT_USER,
        other => return Err(format!("不支持的 hive {other}: {trace_path}")),
    };
    snapshot_key(hive, &subkey.to_lowercase(), 0)
}

/// 独立快照：不信任备份模块自身的 payload，测试侧直接读取文件字节哈希
/// 与注册表原始类型/字节。任何拟删项读取失败（含目录/键树枚举失败）
/// 立即整批中止——没有完整证据就不得继续删除。
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
                    if !path.exists() {
                        panic!("拟删除候选不存在，快照中止（不得静默跳过）: {}", trace.path);
                    }
                    // 目录候选取递归树快照；文件候选取哈希
                    let metadata = std::fs::symlink_metadata(path).unwrap_or_else(|error| {
                        panic!("读取候选元数据失败 {}: {error}", trace.path)
                    });
                    // 重解析点（junction/symlink）必须在分派前拒绝：
                    // is_dir() 对 junction 为 false，若先走哈希分支会得到
                    // "拒绝访问"而非明确的越界原因
                    if metadata.file_type().is_symlink() {
                        panic!(
                            "候选是重解析点（junction/symlink），快照中止（不得继续删除）: {}",
                            trace.path
                        );
                    }
                    if metadata.is_dir() {
                        let tree = snapshot_directory_tree(path)
                            .unwrap_or_else(|error| panic!("{error}（快照中止，不得继续删除）"));
                        item["directory_tree"] = tree;
                    } else {
                        let digest = file_sha256(path)
                            .unwrap_or_else(|error| panic!("{error}（快照中止，不得继续删除）"));
                        item["sha256"] = serde_json::json!(digest);
                        item["size"] = serde_json::json!(metadata.len());
                    }
                }
                TraceType::RegistryValue => {
                    let (vtype, bytes) = read_registry_value_raw(&trace.path)
                        .unwrap_or_else(|error| panic!("{error}（快照中止，不得继续删除）"));
                    item["registry_type"] = serde_json::json!(vtype);
                    item["registry_bytes_hex"] = serde_json::json!(bytes
                        .iter()
                        .map(|b| format!("{b:02x}"))
                        .collect::<String>());
                }
                TraceType::RegistryKey => {
                    let tree = snapshot_registry_key_tree(&trace.path)
                        .unwrap_or_else(|error| panic!("{error}（快照中止，不得继续删除）"));
                    item["registry_tree"] = tree;
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
    assert_eq!(
        before_items.len(),
        after_items.len(),
        "dry-run 不得增删候选"
    );
    for (b, a) in before_items.iter().zip(after_items.iter()) {
        assert_eq!(b, a, "dry-run 产生了修改: {b} -> {a}");
    }
}

/// 恢复后按独立快照逐项核验。读不到就是失败——不存在"快照时已缺失"
/// 的幂等放行（capture 阶段已保证全部候选可读且存在）。冲突豁免清单
/// 中的路径按 CONFLICT 记录（后写入的用户数据不覆盖）。
fn verify_restored_against_snapshot(
    snapshot: &serde_json::Value,
    conflict_paths: &[String],
    evidence: &EvidenceDir,
    report_name: &str,
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
                if item.get("directory_tree").is_some() {
                    // 目录候选：按递归树结构核验（在对应恢复阶段调用时点，
                    // 树内容必须与快照完全一致）
                    match snapshot_directory_tree(Path::new(&path)) {
                        Ok(actual) => {
                            if actual == item["directory_tree"] {
                                format!("OK  dir-tree {path}")
                            } else {
                                format!(
                                    "FAIL dir-tree {path}: 快照 {:?} 实际 {actual:?}",
                                    item["directory_tree"]
                                )
                            }
                        }
                        Err(error) => format!("FAIL dir-tree {path}: {error}"),
                    }
                } else {
                    let expected = item["sha256"].as_str().expect("快照必有哈希");
                    match file_sha256(Path::new(&path)) {
                        Ok(actual) if actual == expected => format!("OK  file-sha256 {path}"),
                        Ok(actual) => {
                            format!("FAIL file {path}: expected={expected} actual={actual}")
                        }
                        Err(error) => format!("FAIL file {path}: {error}"),
                    }
                }
            }
            "RegistryValue" => {
                let expected_type = item["registry_type"].as_str().expect("快照必有类型");
                let expected_hex = item["registry_bytes_hex"].as_str().expect("快照必有字节");
                match read_registry_value_raw(&path) {
                    Ok((actual_type, actual_bytes)) => {
                        let actual_hex: String =
                            actual_bytes.iter().map(|b| format!("{b:02x}")).collect();
                        if expected_type == actual_type && expected_hex == actual_hex {
                            format!("OK  registry {path} [{actual_type}]")
                        } else {
                            format!("FAIL registry {path}: type/bytes 不匹配")
                        }
                    }
                    Err(error) => format!("FAIL registry {path}: {error}"),
                }
            }
            "RegistryKey" => match snapshot_registry_key_tree(&path) {
                Ok(actual) => {
                    if actual == item["registry_tree"] {
                        format!("OK  registry-tree {path}")
                    } else {
                        format!("FAIL registry-tree {path}: 树内容与快照不一致（实际 {actual:?}）")
                    }
                }
                Err(error) => format!("FAIL registry-tree {path}: {error}"),
            },
            other => format!("SKIP {other} {path}"),
        };
        if conflict && entry.starts_with("FAIL") {
            report.push(format!(
                "CONFLICT-EXPECTED {entry}（后写入的用户数据不覆盖）"
            ));
        } else {
            report.push(entry);
        }
    }
    let text = report.join("\n");
    evidence.write_json(
        report_name,
        &serde_json::json!({ "entries": report, "conflict_paths": conflict_paths }),
    );
    println!("[verify:{report_name}]\n{text}");
    assert!(
        report
            .iter()
            .all(|line| line.starts_with("OK") || line.starts_with("CONFLICT")),
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
                && data_lower.contains(
                    r"app=c:\program files\rustyu dirty traces test app\rustyudirtydaemon.exe"
                ),
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
        // 轮 1 文件清理后留下的空父目录：复扫会以 File trace 报出，
        // 作为显式目录候选进入轮 2（同一套归属/dry-run/备份通道）。
        format!(r"{program_data}\RustYu Dirty Traces"),
        format!(r"{local_app_data}\RustYu Dirty Traces"),
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
    // 在设置隔离 env 之前抓默认存储会话快照（收尾时对比，验证隔离未失效）
    let default_sessions_before: Vec<String> = list_sessions()
        .expect("应能列出默认存储会话")
        .into_iter()
        .map(|session| session.id)
        .collect();
    let storage_root =
        std::env::temp_dir().join(format!("rust-yu-workflow-storage-{}", uuid::Uuid::new_v4()));
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
        listed_location,
        Some(INSTALL_DIR),
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
        println!(
            "[trace] {:?} conf={:?} {}",
            trace.trace_type, trace.confidence, trace.path
        );
    });
    assert!(!traces.is_empty(), "脏卸载后应扫描出残留");
    let (deletable, deferred) = partition_traces_by_fixture_allowlist(&traces, &allowlist);
    println!(
        "[allowlist] 可删除 {} 项；未完成 {} 项",
        deletable.len(),
        deferred.len()
    );
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

    // 轮 2 会话与快照传出（供依赖顺序恢复与逐会话核验）
    let mut round2_backup_id: Option<String> = None;
    let mut round2_snapshot: Option<serde_json::Value> = None;
    if !round2_deletable.is_empty() {
        // 目录候选同样走：重解析点边界（partition 内）→ 只读 dry-run → 快照 → 备份+删除
        let dir_snapshot =
            capture_independent_snapshot("before-round2", &round2_deletable, &evidence);
        let dir_plan = plan_for_traces(&round2_deletable);
        let dir_snapshot_after =
            capture_independent_snapshot("after-round2-dry-run", &round2_deletable, &evidence);
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
        round2_backup_id = round2_results
            .iter()
            .find_map(|result| result.backup_id.clone());
        round2_snapshot = Some(dir_snapshot);
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
        final_deletable
            .iter()
            .map(|t| t.path.clone())
            .collect::<Vec<_>>()
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

    // ---- 按依赖顺序恢复全部会话：先轮 2 目录会话（目录为空时恢复成功，
    //      且此刻树内容与轮 2 快照一致），后轮 1 内容会话（文件写入已恢复
    //      的目录）。若顺序颠倒，轮 1 恢复重建的父目录会让轮 2 的空目录
    //      被"不覆盖"语义误判为恢复失败。 ----
    let mut restore_reports: Vec<(String, Vec<rust_yu_lib::modules::backup::BackupItem>)> =
        Vec::new();
    if let (Some(session_id), Some(snapshot)) = (&round2_backup_id, &round2_snapshot) {
        let restore2 = restore_session(session_id).expect("轮 2 会话恢复不应失败");
        for item in &restore2.session.items {
            println!(
                "[restore-round2] kind={:?} state={:?}",
                item.kind, item.state
            );
        }
        assert!(
            restore2.session.items.iter().all(|item| matches!(
                item.state,
                rust_yu_lib::modules::backup::BackupItemState::Restored
            )),
            "轮 2 目录会话（无冲突注入）必须全部恢复成功"
        );
        restore_reports.push((session_id.clone(), restore2.session.items.clone()));
        // 此刻目录树与轮 2 快照逐项一致（文件尚未恢复，目录应为空）
        verify_restored_against_snapshot(
            snapshot,
            &[],
            &evidence,
            "restore-verification-round2.json",
        );
    }

    // ---- 恢复冲突负例：内容会话恢复前，同路径写入"后来的用户数据" ----
    let conflict_path = PathBuf::from(
        std::env::var("ProgramData").unwrap_or_default()
            + r"\RustYu Dirty Traces\telemetry\pending.json",
    );
    if let Some(parent) = conflict_path.parent() {
        std::fs::create_dir_all(parent).expect("应能重建冲突目录");
    }
    std::fs::write(&conflict_path, b"later-user-data").expect("应能写入后来的用户数据");
    let conflict_paths = vec![conflict_path.to_string_lossy().into_owned()];

    // ---- 轮 1 内容会话恢复（无冲突的其余项构成完整字节恢复正例） ----
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
    restore_reports.push((backup_id.clone(), restore.session.items.clone()));
    evidence.write_json(
        "restore-result.json",
        &serde_json::json!({
            "sessions": restore_reports.iter().map(|(id, items)| serde_json::json!({
                "session_id": id,
                "items": items.iter().map(|i| serde_json::json!({
                    "path": i.original_path, "state": format!("{:?}", i.state),
                })).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
        }),
    );

    // ---- 按独立快照逐项核验（非存在性；冲突项豁免并记录） ----
    verify_restored_against_snapshot(
        &snapshot,
        &conflict_paths,
        &evidence,
        "restore-verification-round1.json",
    );
    // 冲突路径的恢复项应为失败态而非静默跳过
    assert!(
        restore.session.items.iter().any(|item| {
            item.original_path
                .eq_ignore_ascii_case(conflict_paths[0].as_str())
                && matches!(
                    item.state,
                    rust_yu_lib::modules::backup::BackupItemState::RestoreFailed
                )
        }),
        "冲突路径的恢复项必须显式 RestoreFailed，不得静默跳过"
    );
    // 无冲突完整恢复正例：除冲突项外必须全部 Restored（完整字节恢复）
    let non_conflict_restored = restore
        .session
        .items
        .iter()
        .filter(|item| {
            !item
                .original_path
                .eq_ignore_ascii_case(conflict_paths[0].as_str())
        })
        .all(|item| {
            matches!(
                item.state,
                rust_yu_lib::modules::backup::BackupItemState::Restored
            )
        });
    assert!(
        non_conflict_restored,
        "除冲突项外，轮 1 会话必须全部完整恢复（无冲突正例）"
    );

    // ---- 归档全部会话 manifest：读取/写入/读回验证必须全部成功，
    //      之后才允许清理唯一存储；任一失败即保留存储并中止 ----
    let mut sessions_to_archive = vec![("round1", backup_id.clone())];
    if let Some(id2) = &round2_backup_id {
        sessions_to_archive.push(("round2", id2.clone()));
    }
    for (label, session_id) in &sessions_to_archive {
        let source = storage_root
            .join("backups")
            .join(session_id)
            .join("manifest.json");
        evidence.archive_manifest(label, &source);
    }

    // ---- 清场：清测试冲突文件与夹具足迹；证据目录保留 ----
    let _ = std::fs::remove_file(&conflict_path);
    reset_fixture();
    // 全部 manifest 已验证归档，才允许清理唯一隔离存储
    std::fs::remove_dir_all(&storage_root).expect("清理隔离存储应成功（证据已全部归档）");
    // 显式恢复环境变量后再做默认存储前后对比（不再用隔离根调用任何 API）
    drop(_env_restore);
    let default_after: Vec<String> = list_sessions()
        .expect("应能列出默认存储会话")
        .into_iter()
        .map(|session| session.id)
        .collect();
    let leaked: Vec<String> = default_after
        .into_iter()
        .filter(|id| !default_sessions_before.contains(id))
        .collect();
    assert!(
        leaked.is_empty(),
        "默认用户存储出现了新会话（隔离失效）: {leaked:?}"
    );
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

// ---------- 可控失败夹具：即使断言 panic 也只清理本次独占创建的资源 ----------

struct FixtureScope {
    root: PathBuf,
    junctions: Vec<PathBuf>,
}

impl FixtureScope {
    fn new(label: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("rust-yu-evidence-{label}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir(&root).expect("独占创建 UUID 测试目录");
        Self {
            root,
            junctions: Vec::new(),
        }
    }

    fn evidence(&self) -> EvidenceDir {
        EvidenceDir::at(self.root.join("evidence"))
    }

    fn junction(&mut self, target: &Path) -> PathBuf {
        assert!(
            target.starts_with(&self.root),
            "联接仅指向本测试夹具内的目录"
        );
        let junction = self.root.join(format!("link-{}", self.junctions.len()));
        self.junctions.push(junction.clone());
        let (ok, text) = run_powershell(&format!(
            "New-Item -ItemType Junction -Path '{}' -Target '{}' | Out-Null; if ($?) {{ exit 0 }} else {{ exit 1 }}",
            junction.to_string_lossy().replace('\'', "''"), target.to_string_lossy().replace('\'', "''")
        ));
        assert!(ok, "创建测试 junction 应成功: {text}");
        junction
    }
}

impl Drop for FixtureScope {
    fn drop(&mut self) {
        // 先移除联接本身，避免递归清场意外遍历其目标；root 是本次独占的 UUID 根。
        for junction in &self.junctions {
            let _ = std::fs::remove_dir(junction);
        }
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn with_fixture(label: &str, test: impl FnOnce(&mut FixtureScope)) {
    let mut scope = FixtureScope::new(label);
    let root = scope.root.clone();
    test(&mut scope);
    drop(scope);
    match std::fs::symlink_metadata(&root) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        other => panic!("夹具清场必须确认不存在，不能把读取失败当不存在: {root:?} {other:?}"),
    }
}

fn assert_panics_with(expected: &str, test: impl FnOnce()) {
    let payload =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(test)).expect_err("负例必须中止执行");
    let message = payload
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| payload.downcast_ref::<&str>().copied())
        .unwrap_or("非字符串 panic");
    assert!(
        message.contains(expected),
        "错误原因必须匹配 {expected:?}，实际: {message}"
    );
}

struct RegistryFixture(String);

impl RegistryFixture {
    fn new() -> (Self, winreg::RegKey) {
        let path = format!(r"Software\rust-yu-test\{}", uuid::Uuid::new_v4());
        let (key, disposition) = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER)
            .create_subkey(&path)
            .expect("创建 UUID 测试键");
        assert_eq!(
            disposition,
            winreg::enums::REG_CREATED_NEW_KEY,
            "只拥有新建的测试键"
        );
        (Self(path), key)
    }
}

impl Drop for RegistryFixture {
    fn drop(&mut self) {
        let _ = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER).delete_subkey_all(&self.0);
    }
}

#[test]
fn reparse_check_rejects_junction_pointing_outside_fixture_tree() {
    with_fixture("reparse", |fixture| {
        let outside = fixture.root.join("outside");
        std::fs::create_dir(&outside).expect("创建夹具外部候选目录");
        std::fs::write(outside.join("payload.txt"), b"x").expect("写入夹具文件");
        let junction = fixture.junction(&outside);
        let traces = vec![Trace::new(
            "Demo App".to_string(),
            TraceType::File,
            junction.join("payload.txt").to_string_lossy().into_owned(),
        )];
        assert!(
            !find_reparse_escalations(&traces).is_empty(),
            "含 junction 的候选必须被拒绝"
        );
        let normal = vec![Trace::new(
            "Demo App".to_string(),
            TraceType::File,
            outside.join("payload.txt").to_string_lossy().into_owned(),
        )];
        assert!(find_reparse_escalations(&normal).is_empty());
        assert_eq!(
            std::fs::read(outside.join("payload.txt")).expect("目标字节保留"),
            b"x"
        );
    });
}

#[test]
fn snapshot_capture_aborts_when_candidate_missing() {
    with_fixture("missing", |fixture| {
        let evidence = fixture.evidence();
        let missing = Trace::new(
            "Demo".to_string(),
            TraceType::File,
            fixture
                .root
                .join("missing.txt")
                .to_string_lossy()
                .into_owned(),
        );
        assert_panics_with("拟删除候选不存在", || {
            capture_independent_snapshot("neg-missing", std::slice::from_ref(&missing), &evidence);
        });
    });
}

#[test]
fn snapshot_capture_aborts_on_junction_directory_candidate() {
    with_fixture("junction", |fixture| {
        let outside = fixture.root.join("outside");
        std::fs::create_dir(&outside).expect("创建夹具目录");
        let junction = fixture.junction(&outside);
        let evidence = fixture.evidence();
        let trace = Trace::new(
            "Demo".to_string(),
            TraceType::File,
            junction.to_string_lossy().into_owned(),
        );
        assert_panics_with("重解析点", || {
            capture_independent_snapshot("neg-junction", std::slice::from_ref(&trace), &evidence);
        });
    });
}

#[test]
fn evidence_persistence_failure_aborts_before_any_deletion() {
    with_fixture("blocked", |fixture| {
        let blocker = fixture.root.join("not-a-directory");
        std::fs::write(&blocker, b"not a dir").expect("写入阻断文件");
        assert_panics_with("证据目录创建失败", || {
            EvidenceDir::at(blocker.join("child"));
        });
        assert_eq!(std::fs::read(&blocker).expect("原文件保留"), b"not a dir");
    });
}

#[test]
fn restore_verification_fails_when_restored_file_is_missing() {
    with_fixture("verify-missing", |fixture| {
        let file = fixture.root.join("data.txt");
        std::fs::write(&file, b"payload").expect("写入夹具文件");
        let trace = Trace::new(
            "Demo".to_string(),
            TraceType::File,
            file.to_string_lossy().into_owned(),
        );
        let evidence = fixture.evidence();
        let snapshot =
            capture_independent_snapshot("neg-verify", std::slice::from_ref(&trace), &evidence);
        std::fs::remove_file(&file).expect("仅移除本次夹具文件");
        assert_panics_with("恢复核验存在失败项", || {
            verify_restored_against_snapshot(&snapshot, &[], &evidence, "neg-verify-report.json");
        });
    });
}

#[test]
fn restore_verification_fails_on_registry_value_mismatch() {
    with_fixture("registry", |fixture| {
        let (registry, key) = RegistryFixture::new();
        let path = registry.0.clone();
        key.set_value("Watched", &"original-value")
            .expect("写入测试值");
        let trace = Trace::new(
            "Demo".to_string(),
            TraceType::RegistryValue,
            format!(r"HKCU\{path}\Watched"),
        );
        let evidence = fixture.evidence();
        let snapshot =
            capture_independent_snapshot("neg-reg", std::slice::from_ref(&trace), &evidence);
        key.set_value("Watched", &"tampered-value")
            .expect("只修改测试值");
        assert_panics_with("恢复核验存在失败项", || {
            verify_restored_against_snapshot(&snapshot, &[], &evidence, "neg-reg-report.json");
        });
        drop(key);
        drop(registry);
        match winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER).open_subkey(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            _ => panic!("UUID 测试注册表键必须清理，不把其他读取错误当成不存在"),
        }
    });
}

#[test]
fn manifest_archive_failure_preserves_source_and_blocks_later_cleanup() {
    with_fixture("archive-failure", |fixture| {
        let source = fixture.root.join("manifest.json");
        let original = b"{\"id\":\"fixture\",\"items\":[]}";
        std::fs::write(&source, original).expect("写入真实源清单");
        let evidence = fixture.evidence();
        // 证据目录已成功创建，仅阻断归档目标；这才是 manifest 写入失败负例。
        std::fs::create_dir(evidence.path().join("backup-manifest-round1.json"))
            .expect("阻断归档文件创建");
        let continued = std::cell::Cell::new(false);
        assert_panics_with("归档 round1 manifest 失败", || {
            evidence.archive_manifest("round1", &source);
            continued.set(true);
            std::fs::remove_file(&source).expect("仅模拟归档成功后的清场");
        });
        assert!(!continued.get(), "归档失败后不得进入后续清场");
        assert_eq!(std::fs::read(&source).expect("原始清单必须保留"), original);
    });
}

#[test]
fn manifest_archive_preserves_exact_bytes_and_refuses_overwrite() {
    with_fixture("archive-success", |fixture| {
        let source = fixture.root.join("manifest.json");
        let original = "{\"name\":\"中文夹具\",\"items\":[]}\r\n".as_bytes();
        std::fs::write(&source, original).expect("写入源清单");
        let evidence = fixture.evidence();
        evidence.archive_manifest("round1", &source);
        let dest = evidence.path().join("backup-manifest-round1.json");
        assert_eq!(std::fs::read(&dest).expect("归档字节可读"), original);
        std::fs::write(&source, b"changed").expect("修改自建源以验证不覆盖旧证据");
        assert_panics_with("归档 round1 manifest 失败", || {
            evidence.archive_manifest("round1", &source)
        });
        assert_eq!(std::fs::read(&dest).expect("旧证据完整保留"), original);
    });
}
