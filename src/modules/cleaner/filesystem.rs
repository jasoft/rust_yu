use super::models::CleanResult;
use crate::modules::common::error::UninstallerError;
use crate::modules::common::utils;
use crate::modules::scanner::models::Trace;

/// 删除文件痕迹
pub async fn delete_file_trace(trace: &Trace) -> Result<CleanResult, UninstallerError> {
    let path = std::path::PathBuf::from(&trace.path);

    // 检查路径是否存在
    if !path.exists() {
        return Ok(CleanResult {
            trace_id: trace.id.clone(),
            path: trace.path.clone(),
            success: true, // 目标已不存在，视为成功
            error: None,
            bytes_freed: 0,
            backup_id: None,
        });
    }

    // 计算并删除
    let bytes_freed: u64;
    let result = if path.is_dir() {
        // 目录：计算大小后删除
        bytes_freed = utils::calculate_dir_size(&path).unwrap_or(0);
        remove_with_process_release(&path)
    } else {
        // 文件：计算大小后删除
        bytes_freed = path.metadata()?.len();
        remove_with_process_release(&path)
    };

    match result {
        Ok(_) => {
            tracing::info!("已删除: {}", trace.path);

            Ok(CleanResult {
                trace_id: trace.id.clone(),
                path: trace.path.clone(),
                success: true,
                error: None,
                bytes_freed,
                backup_id: None,
            })
        }
        Err(e) => {
            tracing::error!("删除失败 {}: {}", trace.path, e);

            Ok(CleanResult {
                trace_id: trace.id.clone(),
                path: trace.path.clone(),
                success: false,
                error: Some(e.to_string()),
                bytes_freed: 0,
                backup_id: None,
            })
        }
    }
}

/// 删除路径；若目标被"以该文件为可执行映像的运行中进程"占用（映像锁会让
/// remove_file 报错，或在 NTFS POSIX 语义下返回成功但延迟消失），先结束
/// 这些进程再重试一次。只结束映像路径与目标完全一致的进程——目标本身
/// 已通过备份与安全检查，映像恰好落在该路径下的进程必然属于被清理软件。
fn remove_with_process_release(path: &std::path::Path) -> std::io::Result<()> {
    let first = remove_once(path);
    let pending = |error: &std::io::Result<()>| match error {
        Ok(()) => path.exists(),
        Err(error) => is_busy(error),
    };
    if !pending(&first) {
        // 删除成功后向上清理"空壳"父目录：真实卸载器都会移除残留的空目录
        // 树，否则复扫会再次扫出这些空壳（只删空目录，非空即停；共享根
        // 目录是硬边界，绝不停靠删除）
        prune_empty_parent_directories(path);
        return first;
    }

    let released = terminate_processes_with_image(path);
    if released == 0 {
        return first;
    }
    tracing::warn!(
        path = %path.display(),
        count = released,
        "目标文件被运行中进程占用，已结束占用进程并重试删除"
    );

    // 进程退出后句柄释放需要一点时间，短暂轮询等待目标真正可删
    for _ in 0..30 {
        if !path.exists() {
            prune_empty_parent_directories(path);
            return Ok(());
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    remove_once(path)
}

fn remove_once(path: &std::path::Path) -> std::io::Result<()> {
    if path.is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    }
}

fn is_busy(error: &std::io::Error) -> bool {
    matches!(
        error.raw_os_error(),
        Some(32) | Some(5) | Some(33) // ERROR_SHARING_VIOLATION / ACCESS_DENIED / ERROR_LOCK_VIOLATION
    )
}

/// 结束"可执行映像路径与目标一致"的进程，返回结束的进程数。
/// 绝不会命中自身：RustYu 自身映像不会是待清理的残留文件。
fn terminate_processes_with_image(path: &std::path::Path) -> usize {
    let mut system = sysinfo::System::new();
    system.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
    let target = normalize_for_compare(path);
    let my_pid = sysinfo::Pid::from_u32(std::process::id());

    system
        .processes()
        .values()
        .filter(|process| {
            process.pid() != my_pid
                && process
                    .exe()
                    .is_some_and(|exe| normalize_for_compare(exe) == target)
        })
        .map(|process| {
            let pid = process.pid();
            let killed = process.kill();
            tracing::warn!(pid = pid.as_u32(), killed, "结束占用目标文件的进程");
            killed
        })
        .filter(|killed| *killed)
        .count()
}

fn normalize_for_compare(path: &std::path::Path) -> String {
    let text = path.to_string_lossy().to_lowercase();
    match text.strip_prefix(r"\\?\") {
        Some(stripped) => stripped.to_string(),
        None => text,
    }
}

/// 向上删除空的父目录链：从 start 的父目录开始，遇到非空目录或共享根
/// 边界即停止。remove_dir 只会删除空目录（OS 原子保证），因此并发下新
/// 创建文件只会让链路自然中断，不存在误删风险。
fn prune_empty_parent_directories(start: &std::path::Path) {
    let mut current = start.parent().map(std::path::Path::to_path_buf);
    while let Some(dir) = current {
        if is_shared_root_boundary(&dir) {
            break;
        }
        match std::fs::remove_dir(&dir) {
            Ok(()) => {
                tracing::info!(path = %dir.display(), "已清理空父目录");
                current = dir.parent().map(std::path::Path::to_path_buf);
            }
            Err(_) => break,
        }
    }
}

/// 终局空目录收尾入口：目标自身已被删除时，补一次父目录链清理。
/// 逐项删除期间 prune 只能在"当时已空"时成功；同目录最后一批文件删除
/// 后不会再有触发点，因此清理循环结束后统一补跑。
pub fn prune_parent_chain_if_empty(path: &std::path::Path) {
    if !path.exists() {
        prune_empty_parent_directories(path);
    }
}

/// 空目录清理的硬边界：常见共享根与盘根绝不停靠删除。
fn is_shared_root_boundary(dir: &std::path::Path) -> bool {
    let normalized = normalize_for_compare(dir);
    let mut roots: Vec<String> = vec![r"c:\".to_string()];
    for key in [
        "ProgramData",
        "ProgramFiles",
        "ProgramFiles(x86)",
        "ProgramW6432",
        "LOCALAPPDATA",
        "APPDATA",
        "USERPROFILE",
        "SystemRoot",
        "TEMP",
    ] {
        if let Ok(value) = std::env::var(key) {
            roots.push(value);
        }
    }
    roots.iter().any(|value| {
        normalized == value.trim_end_matches('\\').to_lowercase()
    }) || crate::modules::scanner::scope::is_protected_shared_path(dir)
}

#[cfg(test)]
mod tests {
    use super::{normalize_for_compare, remove_with_process_release};
    use std::path::Path;

    #[test]
    fn normalize_strips_verbatim_prefix_case_insensitively() {
        assert_eq!(
            normalize_for_compare(Path::new(r"\\?\C:\Program Files\Demo\App.EXE")),
            r"c:\program files\demo\app.exe"
        );
        assert_eq!(
            normalize_for_compare(Path::new(r"C:\Program Files\Demo\App.EXE")),
            r"c:\program files\demo\app.exe"
        );
    }

    #[test]
    fn remove_with_process_release_deletes_ordinary_file() {
        let root = std::env::temp_dir().join(format!(
            "rust-yu-fs-release-test-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).expect("应能创建测试目录");
        let file = root.join("leftover.txt");
        std::fs::write(&file, b"data").expect("应能写入测试文件");

        remove_with_process_release(&file).expect("无占用进程时应直接删除");
        assert!(!file.exists());

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn prune_removes_empty_parent_chain_but_keeps_non_empty_and_boundary() {
        let base = std::env::temp_dir().join(format!(
            "rust-yu-prune-test-{}",
            uuid::Uuid::new_v4()
        ));
        // 空链：a/b/c/file.txt 删文件后 a/b/c 全空应被清到 base 之下（base 非空含另一文件则保留）
        let empty_chain_leaf = base.join("a").join("b").join("c");
        std::fs::create_dir_all(&empty_chain_leaf).expect("应能创建空链目录");
        let file = empty_chain_leaf.join("file.txt");
        std::fs::write(&file, b"x").expect("应能写入文件");
        std::fs::write(base.join("keep.txt"), b"keep").expect("应能写入保留文件");

        remove_with_process_release(&file).expect("应能删除文件");
        assert!(!empty_chain_leaf.exists(), "空叶子目录应被清理");
        assert!(!base.join("a").exists(), "整条空链 a/b/c 应被清理");
        assert!(base.join("keep.txt").exists(), "非空 base 及其文件必须保留");

        // 边界：直接位于 TEMP 下的文件删除后绝不能尝试删除 TEMP 本身
        let boundary_file = std::env::temp_dir().join(format!(
            "rust-yu-prune-boundary-{}.txt",
            uuid::Uuid::new_v4()
        ));
        std::fs::write(&boundary_file, b"x").expect("应能写入边界文件");
        remove_with_process_release(&boundary_file).expect("应能删除边界文件");
        assert!(std::env::temp_dir().is_dir(), "TEMP 共享根必须原样保留");

        let _ = std::fs::remove_dir_all(base);
    }
}
