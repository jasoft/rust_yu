//! 生产备份安全闸回归：尚无配置级备份与恢复能力的服务/计划任务候选，
//! 在清理公共入口必须被默认拒绝实际删除（两个授权入口一致），备份准备
//! 失败时同样不允许删除；文件/注册表清理继续受备份闸保护。
//!
//! 隔离保证（RAII guard 覆盖整个 await 生命周期，panic/unwind 也恢复）：
//! - 备份 manifest 必须落在指定的 UUID 隔离根；
//! - 默认用户存储（%APPDATA%\rust-yu）不得新增任何会话；
//! - 测试结束后 RUST_YU_STORAGE_DIR 恢复原值。
//!
//! 全部使用虚构的 service:// task:// 名字与临时文件，不触碰真实服务，
//! 也不删除默认存储里的任何未知会话。

#![cfg(windows)]

use rust_yu_lib::modules::backup::list_sessions;
use rust_yu_lib::modules::cleaner::{clean_reviewed_traces, clean_traces};
use rust_yu_lib::modules::scanner::models::{Confidence, Trace, TraceType};
use std::ffi::OsString;
use std::path::PathBuf;

const STORAGE_ENV: &str = "RUST_YU_STORAGE_DIR";
const GATE_MARKER: &str = "服务/计划任务尚不支持配置级备份与恢复";

// 同进程内的三个测试都改写 RUST_YU_STORAGE_DIR，必须串行（与 lib 侧
// TEST_STORAGE_ENV_LOCK 同理；跨进程由 cargo 的进程隔离保证）。
static GATE_TEST_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// 隔离 guard：构造时把存储指向 UUID 根，Drop 时恢复原值并清理测试根。
/// 必须覆盖测试体整个 await 生命周期（持有到作用域结束即可），异常展开
/// 时 Drop 依然恢复——此前版本的 `FnOnce() -> Future` 助手在未 await 的
/// Future 上提前恢复环境变量，导致备份泄漏进默认用户存储（已确认泄漏
/// 会话 25036b9b/31d1b315，按指示保留不动）。
struct StorageGuard {
    previous: Option<OsString>,
    root: PathBuf,
}

impl StorageGuard {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "rust-yu-gate-test-{}",
            uuid::Uuid::new_v4()
        ));
        let previous = std::env::var_os(STORAGE_ENV);
        std::env::set_var(STORAGE_ENV, &root);
        Self { previous, root }
    }

    fn root(&self) -> &PathBuf {
        &self.root
    }
}

impl Drop for StorageGuard {
    fn drop(&mut self) {
        match self.previous.take() {
            Some(value) => std::env::set_var(STORAGE_ENV, value),
            None => std::env::remove_var(STORAGE_ENV),
        }
        // 只清理本测试创建的 UUID 根，绝不触碰默认存储
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

/// 默认（未隔离）用户存储的会话 id 快照。调用时必须处于 guard 之外。
fn default_storage_session_ids() -> Vec<String> {
    list_sessions()
        .expect("应能列出默认存储会话")
        .into_iter()
        .map(|session| session.id)
        .collect()
}

/// 标准隔离收尾断言：env 恢复原值 + 默认用户存储零新增。
fn assert_env_restored_and_default_storage_untouched(
    previous: &Option<OsString>,
    before: &[String],
) {
    assert_eq!(
        std::env::var_os(STORAGE_ENV),
        previous.clone(),
        "测试结束后 RUST_YU_STORAGE_DIR 必须恢复原值"
    );
    let after = default_storage_session_ids();
    let new_sessions: Vec<String> = after
        .into_iter()
        .filter(|id| !before.contains(id))
        .collect();
    assert!(
        new_sessions.is_empty(),
        "默认用户存储不得新增会话（隔离失效）: {new_sessions:?}"
    );
}

fn fake_service_trace() -> Trace {
    Trace::new(
        "Gate Test App".to_string(),
        TraceType::Service,
        "service://RustYuGateNoSuchService".to_string(),
    )
    .with_confidence(Confidence::High)
}

fn fake_task_trace() -> Trace {
    Trace::new(
        "Gate Test App".to_string(),
        TraceType::ScheduledTask,
        r"task://\RustYuGateNoSuchTask".to_string(),
    )
    .with_confidence(Confidence::High)
}

fn temp_file_trace(root: &PathBuf, name: &str) -> Trace {
    let file = root.join(name);
    std::fs::write(&file, b"gate fixture").expect("应能写入测试文件");
    Trace::new(
        "Gate Test App".to_string(),
        TraceType::File,
        file.to_string_lossy().into_owned(),
    )
    .with_confidence(Confidence::High)
}

#[tokio::test]
async fn reviewed_cleanup_rejects_service_and_task_without_supported_backup() {
    let _lock = GATE_TEST_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let previous = std::env::var_os(STORAGE_ENV);
    let before = default_storage_session_ids();

    let mut _backup_session: Option<String> = None;
    {
        let _guard = StorageGuard::new();
        let workdir = std::env::temp_dir().join(format!(
            "rust-yu-gate-work-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&workdir).expect("应能创建工作目录");
        let file_trace = temp_file_trace(&workdir, "leftover.txt");
        let traces = vec![file_trace, fake_service_trace(), fake_task_trace()];

        let results = clean_reviewed_traces(traces, true)
            .await
            .expect("reviewed 清理不应整体失败");
        assert_eq!(results.len(), 3, "每个候选都应有结果");

        for result in &results {
            println!(
                "[gate] success={} path={} error={:?}",
                result.success, result.path, result.error
            );
        }
        let file_result = results
            .iter()
            .find(|result| result.path.ends_with("leftover.txt"))
            .expect("应有文件结果");
        assert!(file_result.success, "文件清理应正常成功（受备份闸保护）");
        _backup_session = file_result.backup_id.clone();

        for result in results.iter().filter(|result| {
            result.path.starts_with("service://") || result.path.starts_with("task://")
        }) {
            assert!(
                !result.success,
                "服务/任务候选必须被拒绝实际删除: {}",
                result.path
            );
            let error = result.error.as_deref().unwrap_or_default();
            assert!(
                error.contains(GATE_MARKER),
                "拒绝原因必须准确说明未支持备份能力，实际: {error}"
            );
        }

        let _ = std::fs::remove_dir_all(workdir);
    }

    assert_env_restored_and_default_storage_untouched(&previous, &before);
}

#[tokio::test]
async fn backup_manifest_lands_in_isolated_root_and_default_storage_untouched() {
    let _lock = GATE_TEST_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let previous = std::env::var_os(STORAGE_ENV);
    let before = default_storage_session_ids();

    {
        let guard = StorageGuard::new();
        let workdir = std::env::temp_dir().join(format!(
            "rust-yu-gate-work3-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&workdir).expect("应能创建工作目录");
        let traces = vec![temp_file_trace(&workdir, "isolated.txt")];

        let results = clean_reviewed_traces(traces, true)
            .await
            .expect("清理应成功");
        assert!(results[0].success, "文件应被删除");
        let session_id = results[0]
            .backup_id
            .clone()
            .expect("文件清理应产生备份会话");

        // 备份 manifest 必须位于 UUID 隔离根（guard 仍存活、根未清理）
        let manifest = guard
            .root()
            .join("backups")
            .join(&session_id)
            .join("manifest.json");
        assert!(
            manifest.is_file(),
            "备份 manifest 必须位于隔离根: {}",
            manifest.display()
        );
        // guard 存活期间默认根不可见：隔离根中的会话不应出现在默认列表
        let _ = std::fs::remove_dir_all(workdir);
        drop(guard);
    }

    assert_eq!(
        std::env::var_os(STORAGE_ENV),
        previous.clone(),
        "测试结束后环境变量必须恢复原值"
    );
    let after = default_storage_session_ids();
    let new_sessions: Vec<String> = after
        .into_iter()
        .filter(|id| !before.contains(id))
        .collect();
    assert!(
        new_sessions.is_empty(),
        "默认用户存储不得新增会话（隔离失效）: {new_sessions:?}"
    );
}

#[tokio::test]
async fn safe_defaults_cleanup_rejects_service_and_task_consistently() {
    let _lock = GATE_TEST_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let previous = std::env::var_os(STORAGE_ENV);
    let before = default_storage_session_ids();

    {
        let _guard = StorageGuard::new();
        let results = clean_traces(vec![fake_service_trace(), fake_task_trace()], true)
            .await
            .expect("SafeDefaults 清理不应整体失败（逐项拒绝）");
        assert_eq!(results.len(), 2);
        for result in &results {
            assert!(
                !result.success,
                "SafeDefaults 与 UserReviewed 两个入口必须一致拒绝: {}",
                result.path
            );
            assert!(
                result.error.as_deref().unwrap_or_default().contains(GATE_MARKER)
                    || result.error.as_deref().unwrap_or_default().contains("共享")
                    || result.error.as_deref().unwrap_or_default().contains("关键系统"),
                "拒绝原因应可见，实际: {:?}",
                result.error
            );
        }
    }

    assert_eq!(
        std::env::var_os(STORAGE_ENV),
        previous.clone(),
        "环境变量必须恢复"
    );
    let after = default_storage_session_ids();
    let new_sessions: Vec<String> = after
        .into_iter()
        .filter(|id| !before.contains(id))
        .collect();
    assert!(
        new_sessions.is_empty(),
        "默认用户存储不得新增会话: {new_sessions:?}"
    );
}

#[tokio::test]
async fn backup_failure_blocks_everything_and_env_recovers_from_blocker() {
    let _lock = GATE_TEST_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let previous = std::env::var_os(STORAGE_ENV);
    let before = default_storage_session_ids();

    {
        let guard = StorageGuard::new();
        // 让备份会话目录创建失败：把存储根指到一个已存在的文件路径。
        // 该覆盖会被 guard 的 Drop 用原值覆盖回去，不会泄漏到后续测试。
        let blocker = std::env::temp_dir().join(format!(
            "rust-yu-gate-blocker-{}.txt",
            uuid::Uuid::new_v4()
        ));
        std::fs::write(&blocker, b"not a directory").expect("应能写入阻断文件");
        std::env::set_var(STORAGE_ENV, &blocker);

        let workdir = std::env::temp_dir().join(format!(
            "rust-yu-gate-work2-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&workdir).expect("应能创建工作目录");
        let traces = vec![temp_file_trace(&workdir, "must-survive.txt"), fake_service_trace()];

        let results = clean_reviewed_traces(traces, true)
            .await
            .expect("备份失败应逐项拒绝而非整体崩溃");
        assert_eq!(results.len(), 2);
        for result in &results {
            assert!(
                !result.success,
                "备份准备失败时任何候选都不得被删除: {}",
                result.path
            );
        }
        let survivor = workdir.join("must-survive.txt");
        assert!(
            survivor.exists(),
            "备份失败时文件候选必须保留在原位"
        );

        let _ = std::fs::remove_dir_all(workdir);
        let _ = std::fs::remove_file(&blocker);
        let _ = guard.root(); // 引用一次避免未使用警告
        drop(guard);
    }

    // blocker 覆盖必须被 guard 恢复，不得泄漏到同进程后续测试
    assert_eq!(
        std::env::var_os(STORAGE_ENV),
        previous.clone(),
        "blocker 泄漏必须被恢复"
    );
    let after = default_storage_session_ids();
    let new_sessions: Vec<String> = after
        .into_iter()
        .filter(|id| !before.contains(id))
        .collect();
    assert!(
        new_sessions.is_empty(),
        "默认用户存储不得新增会话: {new_sessions:?}"
    );
}
