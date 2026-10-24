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
}
