//! 生产备份安全闸回归：尚无配置级备份与恢复能力的服务/计划任务候选，
//! 在清理公共入口必须被默认拒绝实际删除（两个授权入口一致），备份准备
//! 失败时同样不允许删除；文件/注册表清理继续受备份闸保护。
//!
//! 全部使用虚构的 service:// task:// 名字与临时文件，不触碰真实服务。

#![cfg(windows)]

use rust_yu_lib::modules::cleaner::{clean_reviewed_traces, clean_traces};
use rust_yu_lib::modules::scanner::models::{Confidence, Trace, TraceType};
use std::path::PathBuf;

const GATE_MARKER: &str = "服务/计划任务尚不支持配置级备份与恢复";

// 同进程内的三个测试都改写 RUST_YU_STORAGE_DIR，必须串行（与 lib 侧
// TEST_STORAGE_ENV_LOCK 同理；跨进程由 cargo 的进程隔离保证）。
static GATE_TEST_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn with_isolated_storage<T>(test: impl FnOnce() -> T) -> T {
    // 独立进程内的集成测试：cargo 的每个测试二进制是独立进程，环境变量
    // 修改不会影响 lib 单测；这里用 UUID 目录隔离备份会话。
    let root = std::env::temp_dir().join(format!(
        "rust-yu-gate-test-{}",
        uuid::Uuid::new_v4()
    ));
    let previous = std::env::var_os("RUST_YU_STORAGE_DIR");
    std::env::set_var("RUST_YU_STORAGE_DIR", &root);
    let result = test();
    match previous {
        Some(value) => std::env::set_var("RUST_YU_STORAGE_DIR", value),
        None => std::env::remove_var("RUST_YU_STORAGE_DIR"),
    }
    let _ = std::fs::remove_dir_all(&root);
    result
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
    let _guard = GATE_TEST_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    with_isolated_storage(|| async {
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
    })
    .await
}

#[tokio::test]
async fn safe_defaults_cleanup_rejects_service_and_task_consistently() {
    let _guard = GATE_TEST_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    with_isolated_storage(|| async {
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
    })
    .await
}

#[tokio::test]
async fn backup_failure_blocks_everything_including_service_and_task() {
    let _guard = GATE_TEST_ENV_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    with_isolated_storage(|| async {
        // 让备份会话目录创建失败：把存储根指到一个已存在的文件路径
        let blocker = std::env::temp_dir().join(format!(
            "rust-yu-gate-blocker-{}.txt",
            uuid::Uuid::new_v4()
        ));
        std::fs::write(&blocker, b"not a directory").expect("应能写入阻断文件");
        std::env::set_var("RUST_YU_STORAGE_DIR", &blocker);

        let workdir = std::env::temp_dir().join(format!(
            "rust-yu-gate-work2-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&workdir).expect("应能创建工作目录");
        let file_trace = temp_file_trace(&workdir, "must-survive.txt");
        let traces = vec![file_trace, fake_service_trace()];

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

        // 文件必须原样存在（未被删除）；服务名是虚构的，无需验证系统侧
        let survivor = workdir.join("must-survive.txt");
        assert!(
            survivor.exists(),
            "备份失败时文件候选必须保留在原位"
        );

        let _ = std::fs::remove_dir_all(workdir);
        let _ = std::fs::remove_file(blocker);
    })
    .await
}
