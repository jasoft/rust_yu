//! 同目录原子替换：写入失败时保留上一份完整记录，禁止先删除旧文件。

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::MetadataExt;
use std::path::Path;
use windows::core::PCWSTR;
use windows::Win32::Storage::FileSystem::{
    MoveFileExW, FILE_ATTRIBUTE_REPARSE_POINT, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
    MOVE_FILE_FLAGS,
};

/// 只接受普通目录链；备份、日志不得穿过目录联接或其他重解析点。
pub(crate) fn validate_directory_chain(directory: &Path) -> io::Result<()> {
    for ancestor in directory.ancestors() {
        if ancestor.as_os_str().is_empty() {
            continue;
        }
        let metadata = fs::symlink_metadata(ancestor)?;
        if !metadata.is_dir() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0 {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "持久化目录不是普通目录",
            ));
        }
    }
    Ok(())
}

pub(crate) fn write_atomic(path: &Path, content: &[u8]) -> io::Result<()> {
    write_atomic_with(path, |file| file.write_all(content))
}

pub(crate) fn rename_no_replace(source: &Path, destination: &Path) -> io::Result<()> {
    for path in [source, destination] {
        let parent = path
            .parent()
            .ok_or_else(|| io::Error::other("落位路径缺少父目录"))?;
        validate_directory_chain(parent)?;
    }
    // 预检查无法排除随后新建目标的竞争；必须让 Windows 原子拒绝覆盖。
    move_file(source, destination, MOVEFILE_WRITE_THROUGH)
}

fn wide_file_path(path: &Path) -> io::Result<Vec<u16>> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("落位路径缺少父目录"))?;
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::other("落位路径缺少文件名"))?;
    // Windows canonicalize 返回扩展长度路径，避免直接 Win32 调用退回 MAX_PATH。
    // 只解析既存父目录，新建目标自身还不存在，不能 canonicalize 整个目标。
    let normalized = fs::canonicalize(parent)?.join(name);
    let mut wide = normalized.as_os_str().encode_wide().collect::<Vec<_>>();
    if wide.contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "文件路径包含空字符",
        ));
    }
    wide.push(0);
    Ok(wide)
}

fn move_file(source: &Path, destination: &Path, flags: MOVE_FILE_FLAGS) -> io::Result<()> {
    let source = wide_file_path(source)?;
    let destination = wide_file_path(destination)?;
    // 两个缓冲区在调用期间存活且以 NUL 结尾；flags 明确区分覆盖和拒绝覆盖。
    unsafe { MoveFileExW(PCWSTR(source.as_ptr()), PCWSTR(destination.as_ptr()), flags) }
        .map_err(|error| io::Error::from_raw_os_error(error.code().0 & 0xffff))
}

fn write_atomic_with(
    path: &Path,
    write: impl FnOnce(&mut File) -> io::Result<()>,
) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("持久化路径缺少父目录"))?;
    validate_directory_chain(parent)?;
    match fs::symlink_metadata(path) {
        Ok(metadata)
            if !metadata.is_file()
                || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT.0 != 0 =>
        {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "持久化目标不是普通文件",
            ));
        }
        Ok(_) => {}
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    let temporary = parent.join(format!(".rust-yu-{}.tmp", uuid::Uuid::new_v4()));
    // create_new 保证不会截断既有文件；临时文件必须和目标位于同一个卷。
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary)?;
    let result = (|| {
        write(&mut file)?;
        file.flush()?;
        file.sync_all()?;
        drop(file);
        // Windows rename 失败时不能 remove_file(旧清单) 后重试；那会丢掉唯一恢复依据。
        move_file(
            &temporary,
            path,
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    })();
    if result.is_err() {
        // 只清理本次独占创建的 UUID 临时文件，旧目标始终保留。
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::windows::fs::OpenOptionsExt;

    struct TestDirectory(std::path::PathBuf);

    impl TestDirectory {
        fn new(label: &str) -> Self {
            let path =
                std::env::temp_dir().join(format!("rust-yu-{label}-{}", uuid::Uuid::new_v4()));
            fs::create_dir(&path).expect("可创建 UUID 测试目录");
            Self(path)
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn restore_publication_supports_long_unicode_paths() {
        let scope = TestDirectory::new("long-publish");
        let mut parent = scope.0.clone();
        for _ in 0..8 {
            parent = parent.join("中文路径长目录长目录长目录长目录长目录长目录");
        }
        fs::create_dir_all(&parent).expect("可创建长中文路径");
        let source = parent.join("完整载荷.tmp");
        let destination = parent.join("恢复结果.json");
        assert!(destination.as_os_str().encode_wide().count() > 260);
        write_atomic(&source, b"original").expect("长路径原子写入");
        rename_no_replace(&source, &destination).expect("长路径安全落位");
        assert_eq!(
            fs::read(&destination).expect("读回长路径原始字节"),
            b"original"
        );
        write_atomic(&destination, b"replacement").expect("长路径原子替换");
        assert_eq!(fs::read(&destination).expect("替换后字节"), b"replacement");
    }

    #[test]
    fn restore_publication_preserves_target_created_after_precheck() {
        let scope = TestDirectory::new("restore-publish");
        let root = &scope.0;
        let temporary = root.join("complete.tmp");
        let destination = root.join("恢复目标.json");
        fs::write(&temporary, b"original-backup").expect("临时载荷写完");
        assert!(!destination.exists(), "预检查时目标尚不存在");
        // 模拟预检查之后、原子落位之前，另一进程创建了用户的新数据。
        fs::write(&destination, b"later-user-data").expect("并发写者创建目标");
        assert!(
            rename_no_replace(&temporary, &destination).is_err(),
            "落位必须原子拒绝覆盖"
        );
        assert_eq!(
            fs::read(&destination).expect("新数据仍可读取"),
            b"later-user-data"
        );
        assert_eq!(
            fs::read(&temporary).expect("失败后备份仍保留"),
            b"original-backup"
        );
    }

    #[test]
    fn interrupted_write_and_denied_replace_preserve_previous_bytes() {
        let scope = TestDirectory::new("atomic");
        let root = &scope.0;
        let path = root.join("清单.json");
        write_atomic(&path, b"original").expect("首次写入成功");
        let result = write_atomic_with(&path, |file| {
            file.write_all(b"partial")?;
            Err(io::Error::other("simulated disk write failure"))
        });
        assert!(result.is_err());
        assert_eq!(fs::read(&path).expect("旧数据可读"), b"original");
        // 禁止共享删除模拟真实的文件占用；替换失败不能删除或截断旧清单。
        let busy = OpenOptions::new()
            .read(true)
            .share_mode(1)
            .open(&path)
            .expect("占用目标");
        assert!(write_atomic(&path, b"replacement").is_err());
        assert_eq!(fs::read(&path).expect("旧数据仍可读"), b"original");
        drop(busy);
        write_atomic(&path, b"replacement").expect("释放占用后可重试");
        assert_eq!(fs::read(&path).expect("新数据可读"), b"replacement");
        assert_eq!(
            fs::read_dir(root).expect("目录可读").count(),
            1,
            "失败不遗留临时文件"
        );
    }
}
