//! 脏卸载夹具的完整清理与恢复闭环验收（RustYu 自身能力，不借助 GUI）。
//!
//! 前置：管理员 PowerShell + 已构建的 inno-dirty 夹具安装包。
//! 运行：`cargo test --test dirty_fixture_workflow -- --ignored --nocapture`
//!
//! 覆盖链路：安装夹具 → 执行其"脏卸载器"（只删 ARP 键）→ RustYu 扫描残留
//! → clean_reviewed_traces（备份 + 删除）→ 复扫断言残留消失 → restore_session
//! → 核验文件/注册表内容恢复 → Reset 清场。
//!
//! 已知能力边界（断言矩阵如实记录，不算失败）：
//! - 防火墙规则 / 运行中进程 / 环境变量暂无 TraceType 与扫描器支持；
//! - 服务与计划任务 `requires_backup == false`，删除后不可恢复（测试只验证删除）。

#![cfg(windows)]

use rust_yu_lib::modules::backup::restore_session;
use rust_yu_lib::modules::cleaner::clean_reviewed_traces;
use rust_yu_lib::modules::lister;
use rust_yu_lib::modules::lister::models::{InstallSourceSelector, ListProgramsQuery};
use rust_yu_lib::modules::scanner;
use rust_yu_lib::modules::scanner::models::{Trace, TraceType};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

const APP_NAME: &str = "RustYu Dirty Traces Test App";
const INSTALL_DIR: &str = r"C:\Program Files\RustYu Dirty Traces Test App";
const SERVICE_NAME: &str = "RustYuDirtyTraces";
const TASK_NAME: &str = "RustYuDirtyTracesStartTask";

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

fn fixture_installer() -> PathBuf {
    repository_root()
        .join(r".resources\inno-dirty-traces\output\RustYuDirtyTracesSetup.exe")
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

fn registry_value_exists(hive: &str, subkey: &str, value: &str) -> bool {
    let script = format!(
        "$null -ne (Get-ItemProperty -LiteralPath '{hive}:\\{subkey}' -Name '{value}' -ErrorAction SilentlyContinue)"
    );
    let (ok, text) = run_powershell(&script);
    ok && text.trim().eq_ignore_ascii_case("true")
}

fn registry_key_exists(hive: &str, subkey: &str) -> bool {
    let script = format!("Test-Path -LiteralPath '{hive}:\\{subkey}'");
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
    let installer = fixture_installer().to_string_lossy().into_owned();
    assert!(
        Path::new(&installer).is_file(),
        "缺少夹具安装包 {installer}，请先运行 Build-DirtyTracesFixture.ps1"
    );
    let status = Command::new(&installer)
        .args(["/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART", "/SP-"])
        .status()
        .expect("应能启动夹具安装包");
    assert!(status.success(), "夹具静默安装应成功");
    assert!(registry_key_exists("HKLM", r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\rust_yu_dirty_traces_is1"),
        "安装后 ARP 键应存在");
}

/// 执行夹具自带的"脏卸载器"：只删除 ARP 键，其余痕迹全部保留。
fn run_dirty_uninstaller() {
    let (ok, text) = run_powershell(&format!(
        "$arp = Get-ItemProperty -LiteralPath 'HKLM:\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\rust_yu_dirty_traces_is1'; \
         $quiet = $arp.QuietUninstallString; \
         $exe = $quiet; $exeArgs = ''; \
         if ($quiet -match '^\"([^\"]+)\"\\s*(.*)$') {{ $exe = $Matches[1]; $exeArgs = $Matches[2] }}, \
         $p = Start-Process -FilePath $exe -ArgumentList $exeArgs -Wait -PassThru -WindowStyle Hidden; \
         for ($i = 0; $i -lt 30; $i++) {{ if (-not (Test-Path 'HKLM:\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\rust_yu_dirty_traces_is1')) {{ exit 0 }}; Start-Sleep -Seconds 2 }}; \
         Write-Output ('arp still present, launcher exit=' + $p.ExitCode); exit 1"
    ));
    assert!(ok, "脏卸载器应删除自身的 ARP 键: {text}");
    assert!(
        !registry_key_exists("HKLM", r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\rust_yu_dirty_traces_is1"),
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

#[tokio::test]
#[ignore = "需要管理员 PowerShell 与已构建的 inno-dirty 夹具安装包"]
async fn inno_dirty_fixture_full_cleanup_and_restore_workflow() {
    assert_admin();
    reset_fixture();
    install_fixture();

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

    let has = |predicate: &dyn Fn(&Trace) -> bool| traces.iter().any(predicate);
    // 漏报断言：核心残留类型必须被扫出（防火墙/环境变量/进程无扫描器支持，见文件头）
    assert!(
        has(&|t| t.trace_type == TraceType::File && t.path.contains("RustYu Dirty Traces Test App")),
        "安装目录文件残留应被扫出"
    );
    assert!(
        has(&|t| t.trace_type == TraceType::RegistryKey && t.path.contains(r"RustYu Fixture Labs")),
        "HKLM 厂商键残留应被扫出"
    );
    assert!(
        has(&|t| t.trace_type == TraceType::Service && t.path.contains(SERVICE_NAME)),
        "运行中的服务残留应被扫出"
    );
    assert!(
        has(&|t| t.trace_type == TraceType::ScheduledTask && t.path.contains(TASK_NAME)),
        "计划任务残留应被扫出"
    );

    // ---- 备份 + 清理（UserReviewed：卸载结果页确认后的真实清理入口） ----
    let results = clean_reviewed_traces(traces.clone(), true)
        .await
        .expect("reviewed 清理不应整体失败");
    assert_eq!(results.len(), traces.len(), "每个残留都应有清理结果");

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
        "全部残留清理应成功（失败 {} 项）:\n{}",
        failures.len(),
        failures.join("\n")
    );

    // ---- 复扫断言残留消失 ----
    let remaining = scanner::scan_all_traces_for_program(&program, None)
        .await
        .expect("复扫不应失败")
        .into_iter()
        .filter(|trace| trace.exists)
        .collect::<Vec<Trace>>();
    remaining.iter().for_each(|trace| {
        println!("[remaining] {:?} {}", trace.trace_type, trace.path);
    });
    assert!(
        remaining.is_empty(),
        "清理后复扫应无残留，仍剩 {} 项",
        remaining.len()
    );
    assert!(!service_exists(), "服务应已被删除");
    assert!(!task_exists(), "计划任务应已被删除");
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
    // 服务/计划任务 requires_backup=false：不参与备份，恢复后应保持不存在（能力边界，如实记录）
    assert!(!service_exists(), "服务不属于备份范围，恢复不应重建服务");

    // ---- 清场 ----
    reset_fixture();
    tokio::time::sleep(Duration::from_millis(200)).await;
}
