//! 同目录原子替换：写入失败时保留上一份完整记录，禁止先删除旧文件。

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::MetadataExt;
use std::path::Path;
use windows::core::PCWSTR;
use windows::Win32::Storage::FileSystem::{
    MoveFileExW, FILE_ATTRIBUTE_REPARSE_POINT, MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH,
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
        let source = temporary
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>();
        let destination = path
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect::<Vec<_>>();
        // Windows rename 失败时不能 remove_file(旧清单) 后重试；那会丢掉唯一恢复依据。
        unsafe {
            MoveFileExW(
                PCWSTR(source.as_ptr()),
                PCWSTR(destination.as_ptr()),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        }
        .map_err(|error| io::Error::from_raw_os_error(error.code().0 & 0xffff))
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

    #[test]
    fn interrupted_write_and_denied_replace_preserve_previous_bytes() {
        let root = std::env::temp_dir().join(format!("rust-yu-atomic-{}", uuid::Uuid::new_v4()));
        fs::create_dir(&root).expect("测试目录可创建");
        struct Scope(std::path::PathBuf);
        impl Drop for Scope {
            fn drop(&mut self) {
                let _ = fs::remove_dir_all(&self.0);
            }
        }
        let _scope = Scope(root.clone());
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
            fs::read_dir(&root).expect("目录可读").count(),
            1,
            "失败不遗留临时文件"
        );
    }
}
