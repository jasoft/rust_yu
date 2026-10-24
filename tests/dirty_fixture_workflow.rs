//! 脏卸载夹具的完整清理与恢复闭环验收（RustYu 自身能力，不借助 GUI）。
//!
//! 前置：管理员 PowerShell + 已构建的 inno-dirty 夹具安装包。
//! 运行：`cargo test --test dirty_fixture_workflow -- --ignored --nocapture`
//!
//! 覆盖链路：安装夹具 → 执行其"脏卸载器"（只删 ARP 键）→ RustYu 扫描残留
//! → **夹具精确归属白名单预检（任一越界即整批失败，绝不删除）**
//! → clean_reviewed_traces（备份 + 删除）→ 复扫断言 → restore_session
//! → 核验文件/注册表内容恢复 → Reset 清场。
//!
//! 安全闸门：
//! - RUST_YU_STORAGE_DIR 指向本测试的 UUID 独立目录，备份与会话全部隔离，
//!   绝不落入真实用户存储；
//! - 服务与计划任务目前没有配置级备份/恢复（requires_backup=false），
//!   在实现之前**拒绝删除**并列为未完成项，不把不可恢复的破坏当作成功；
//! - Session Manager\Environment 中的应用变量按当前精确归属规则同样拒删，
//!   列为未完成项（待"别名+用途后缀"精确规则落地）。

#![cfg(windows)]

use rust_yu_lib::modules::backup::restore_session;
use rust_yu_lib::modules::cleaner::clean_reviewed_traces;
use rust_yu_lib::modules::lister;
use rust_yu_lib::modules::lister::models::{InstallSourceSelector, ListProgramsQuery};
use rust_yu_lib::modules::scanner;
use rust_yu_lib::modules::scanner::models::{Trace, TraceType};
use std::path::{Path, PathBuf};
use std::process::Command;

const APP_NAME: &str = "RustYu Dirty Traces Test App";
const INSTALL_DIR: &str = r"C:\Program Files\RustYu Dirty Traces Test App";
const SERVICE_NAME: &str = "RustYuDirtyTraces";
const TASK_NAME: &str = "RustYuDirtyTracesStartTask";
const ARP_SUBKEY: &str =
    r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\rust_yu_dirty_traces_is1";

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

/// 执行夹具自带的"脏卸载器"：只删除 ARP 键，其余痕迹全部保留。
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

// ---------- 夹具精确归属白名单：任一越界即整批失败，绝不删除 ----------

fn normalize_path_key(path: &str) -> String {
    path.trim_end_matches('\\').to_lowercase()
}

/// 读取防火墙规则容器 (值名 => 规则数据) 的当前快照。
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

/// 夹具归属白名单：静态已知痕迹 + 安装前后防火墙规则差集（精确 GUID）。
struct FixtureAllowlist {
    files: Vec<String>,
    registry_keys: Vec<String>,
    registry_values: Vec<(String, String)>,
    /// 安装前后差集得到的夹具防火墙规则值名（GUID），均已核验 Name/App 归属
    firewall_rule_names: Vec<String>,
}

impl FixtureAllowlist {
    /// 在"Reset 之后、安装之前"取防火墙基线。
    fn capture_firewall_baseline() -> std::collections::HashMap<String, String> {
        read_firewall_rule_snapshot()
    }

    /// 安装完成后计算差集并核验每条规则的 Name 与 App 路径归属；
    /// 差集为空或多条规则都视为夹具异常，直接失败。
    fn from_baseline(baseline: &std::collections::HashMap<String, String>) -> Self {
        let current = read_firewall_rule_snapshot();
        let added: Vec<(String, String)> = current
            .iter()
            .filter(|(name, _)| !baseline.contains_key(*name))
            .map(|(name, data)| (name.clone(), data.clone()))
            .collect();
        println!("[allowlist] 防火墙规则安装差集: {} 条", added.len());
        for (name, data) in &added {
            println!("[allowlist] +rule {name}: {data}");
        }
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

/// 夹具已知会留下的文件/AppData 残留（小写规范化后精确相等）。
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

/// 夹具已知的注册表键残留（键 trace，小写精确）。
fn expected_fixture_registry_keys() -> Vec<String> {
    [r"HKLM\SOFTWARE\RustYu Fixture Labs\RustYu Dirty Traces".to_string()]
        .into_iter()
        .map(|path| normalize_path_key(&path))
        .collect()
}

/// 夹具已知的注册表值残留：(键路径小写, 值展示名)。防火墙规则值名是动态
/// GUID，按容器键前缀特判；Environment 值当前按精确规则拒删（deferred）。
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
/// Ok(true)=白名单内；Ok(false)=白名单外的已知 deferred 类（Service/ScheduledTask/
/// Environment——仍要求精确匹配夹具名字）；Err(())=越界候选（整批禁止删除）。
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
            // 防火墙规则：只认安装差集里核验过 Name/App 归属的精确 GUID，
            // 绝不接受"位于容器内"这种宽泛证据
            if key_path.ends_with(r"\firewallrules")
                && allowlist
                    .firewall_rule_names
                    .iter()
                    .any(|name| name.eq_ignore_ascii_case(value_name))
            {
                return Ok(true);
            }
            // Environment 应用变量：已知夹具痕迹，但当前精确规则下拒删（deferred）
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

/// 文件/AppData 目标及其全部祖先必须不含重解析点（junction/symlink），
/// 防止"文本路径在夹具根内、实际解析跳到夹具外"的越界删除。
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
                    violations.push(format!("{} 路径链含重解析点: {}", trace.path, candidate.display()));
                    break;
                }
            }
            current = candidate.parent();
        }
    }
    violations
}

/// 按夹具白名单对扫描结果做逐项归属预检 + 重解析点预检。
/// 返回 (可删除集合, 未完成集合)；出现任何白名单之外或含重解析点的候选
/// 直接 panic，整批不进入清理——宁可失败也不删除未确认归属的目标。
fn partition_traces_by_fixture_allowlist(traces: &[Trace], allowlist: &FixtureAllowlist) -> (Vec<Trace>, Vec<Trace>) {
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

#[tokio::test]
#[ignore = "需要管理员 PowerShell 与已构建的 inno-dirty 夹具安装包"]
async fn inno_dirty_fixture_full_cleanup_and_restore_workflow() {
    assert_admin();

    // ---- 安全闸门：备份/会话写入 UUID 独立存储目录，与真实用户存储隔离 ----
    let storage_root = std::env::temp_dir().join(format!(
        "rust-yu-workflow-storage-{}",
        uuid::Uuid::new_v4()
    ));
    let previous_storage = std::env::var_os("RUST_YU_STORAGE_DIR");
    std::env::set_var("RUST_YU_STORAGE_DIR", &storage_root);

    let result = inno_dirty_workflow_inner().await;

    match previous_storage {
        Some(value) => std::env::set_var("RUST_YU_STORAGE_DIR", value),
        None => std::env::remove_var("RUST_YU_STORAGE_DIR"),
    }
    let _ = std::fs::remove_dir_all(&storage_root);
    result
}

async fn inno_dirty_workflow_inner() {
    reset_fixture();
    // 安装前取防火墙规则基线：安装后用精确差集确定夹具创建的规则 GUID
    let firewall_baseline = FixtureAllowlist::capture_firewall_baseline();
    install_fixture();
    let allowlist = FixtureAllowlist::from_baseline(&firewall_baseline);

    // 卸载前先通过真实 lister 拿到程序档案（含安装目录），供卸载后关联扫描
    let program = find_installed_fixture_program();
    // Inno 写入的 InstallLocation 可能带尾部反斜杠，比较前统一规范化
    let listed_location = program
        .install_location
        .as_deref()
        .map(|path| path.trim_end_matches('\\'));
    assert_eq!(
        listed_location, Some(INSTALL_DIR),
        "夹具安装目录应被 lister 正确识别"
    );

    run_dirty_uninstaller();

    // ---- RustYu 扫描残留（真实 workflow 的扫描入口） ----
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

    // ---- 夹具精确归属预检：任一越界即整批失败（此断言之后才允许删除） ----
    let (deletable, deferred) = partition_traces_by_fixture_allowlist(&traces, &allowlist);
    println!("[allowlist] 可删除 {} 项；未完成（拒删待能力补齐）{} 项", deletable.len(), deferred.len());
    deferred.iter().for_each(|trace| {
        println!("[deferred] {:?} {}（无配置备份恢复/精确归属规则待实现）", trace.trace_type, trace.path);
    });
    assert!(!deletable.is_empty(), "白名单内应存在可删除残留");

    // ---- 备份 + 清理（UserReviewed：卸载结果页确认后的真实清理入口） ----
    let results = clean_reviewed_traces(deletable.clone(), true)
        .await
        .expect("reviewed 清理不应整体失败");
    assert_eq!(results.len(), deletable.len(), "每个待删残留都应有清理结果");

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

    // ---- 复扫：残留只剩"未完成"集合（服务/任务/环境变量），其余必须消失 ----
    let remaining = scanner::scan_all_traces_for_program(&program, None)
        .await
        .expect("复扫不应失败")
        .into_iter()
        .filter(|trace| trace.exists)
        .collect::<Vec<Trace>>();
    remaining.iter().for_each(|trace| {
        println!("[remaining] {:?} {}", trace.trace_type, trace.path);
    });
    let (remaining_deletable, remaining_deferred) =
        partition_traces_by_fixture_allowlist(&remaining, &allowlist);
    assert!(
        remaining_deletable.is_empty(),
        "可删除残留清理后复扫应消失，仍剩 {} 项",
        remaining_deletable.len()
    );
    assert_eq!(
        remaining_deferred.len(),
        deferred.len(),
        "未完成项应原样保留（未删除）"
    );
    // 服务/任务未删除：仍存在（未完成项不谎报成功）
    assert!(service_exists(), "未完成项：服务应仍在（未删除）");
    assert!(task_exists(), "未完成项：计划任务应仍在（未删除）");
    assert!(
        registry_value_exists(
            "HKLM",
            r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment",
            "RUSTYU_DIRTY_TRACES_HOME"
        ),
        "未完成项：环境变量应仍在（当前精确规则拒删）"
    );
    assert!(
        !Path::new(INSTALL_DIR).exists(),
        "安装目录应已被删除"
    );

    // ---- 恢复：备份会话应能把文件与注册表内容还原 ----
    let backup_id = results
        .iter()
        .find_map(|result| result.backup_id.clone())
        .expect("至少应有一个带备份的清理结果");
    let restore = restore_session(&backup_id).expect("恢复会话不应失败");
    for item in &restore.session.items {
        println!("[restore] kind={:?} state={:?}", item.kind, item.state);
    }
    assert!(
        restore.session.items.iter().all(|item| item.state == rust_yu_lib::modules::backup::BackupItemState::Restored),
        "恢复后所有可备份项应为 Restored 状态:\n{:?}",
        restore.session.items
    );

    let daemon_exe = Path::new(INSTALL_DIR).join("RustYuDirtyDaemon.exe");
    assert!(daemon_exe.is_file(), "恢复后守护进程 exe 应回到原位");
    assert!(
        registry_key_exists("HKLM", r"SOFTWARE\RustYu Fixture Labs\RustYu Dirty Traces"),
        "恢复后厂商键应回到原位"
    );
    assert!(
        registry_value_exists(
            "HKLM",
            r"SOFTWARE\Microsoft\Windows\CurrentVersion\Run",
            "RustYuDirtyTraces"
        ),
        "恢复后 HKLM Run 启动值应回到原位"
    );

    // ---- 清场：未完成项与测试足迹由夹具 Reset 统一清理 ----
    reset_fixture();
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

    // 文本路径位于"夹具目录"下，但路径链含 junction：预检必须拒绝
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

    // 普通路径（无重解析点）应通过
    let normal = vec![Trace::new(
        "Demo App".to_string(),
        TraceType::File,
        outside.join("payload.txt").to_string_lossy().into_owned(),
    )];
    assert!(find_reparse_escalations(&normal).is_empty());

    let _ = std::fs::remove_dir_all(&root);
}
