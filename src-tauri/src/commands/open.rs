use std::path::{Path, PathBuf};

use super::CommandError;

fn resolve_install_location(raw_path: &str) -> Result<PathBuf, CommandError> {
    let trimmed = raw_path.trim();
    if trimmed.is_empty() {
        return Err(CommandError::with_code(
            "invalid_install_location",
            "安装目录路径为空",
        ));
    }

    let path = PathBuf::from(trimmed);
    let canonical_path = std::fs::canonicalize(&path).map_err(|error| {
        CommandError::with_code(
            "install_location_not_found",
            format!("安装目录不存在或无法访问: {error}"),
        )
    })?;
    if !canonical_path.is_dir() {
        return Err(CommandError::with_code(
            "install_location_not_directory",
            "安装位置不是一个目录",
        ));
    }

    Ok(canonical_path)
}

#[cfg(windows)]
fn explorer_argument_path(path: &Path) -> PathBuf {
    use std::ffi::OsString;
    use std::os::windows::ffi::{OsStrExt, OsStringExt};

    let wide_path: Vec<u16> = path.as_os_str().encode_wide().collect();
    let extended_prefix: Vec<u16> = OsString::from(r"\\?\").encode_wide().collect();
    if !wide_path.starts_with(&extended_prefix) {
        return path.to_path_buf();
    }

    let remaining = &wide_path[extended_prefix.len()..];
    let unc_prefix: Vec<u16> = OsString::from(r"UNC\").encode_wide().collect();
    if remaining.starts_with(&unc_prefix) {
        let mut normal_path = vec![b'\\' as u16, b'\\' as u16];
        normal_path.extend_from_slice(&remaining[unc_prefix.len()..]);
        return PathBuf::from(OsString::from_wide(&normal_path));
    }

    PathBuf::from(OsString::from_wide(remaining))
}

#[cfg(windows)]
fn open_in_explorer(path: &Path) -> Result<(), CommandError> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;

    use windows::core::PCWSTR;
    use windows::Win32::UI::Shell::ShellExecuteW;
    use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;

    let operation: Vec<u16> = OsStr::new("open")
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let target_path = explorer_argument_path(path);
    let target: Vec<u16> = target_path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    // ShellExecuteW 直接使用 UTF-16 路径，确保中文安装目录不会在传递给资源管理器时乱码。
    let result = unsafe {
        ShellExecuteW(
            None,
            PCWSTR(operation.as_ptr()),
            PCWSTR(target.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };
    let result_code = result.0 as usize;
    if result_code <= 32 {
        return Err(CommandError::with_code(
            "open_install_location_failed",
            format!("Windows 无法打开安装目录，ShellExecuteW 返回值: {result_code}"),
        ));
    }

    Ok(())
}

#[tauri::command]
pub async fn open_program_install_location(path: String) -> Result<(), CommandError> {
    tauri::async_runtime::spawn_blocking(move || {
        let canonical_path = resolve_install_location(&path)?;

        #[cfg(windows)]
        {
            open_in_explorer(&canonical_path)
        }

        #[cfg(not(windows))]
        {
            let _ = canonical_path;
            Err(CommandError::with_code(
                "unsupported_platform",
                "打开安装目录仅支持 Windows",
            ))
        }
    })
    .await
    .map_err(|error| CommandError::new(format!("打开安装目录任务执行失败: {error}")))?
}

#[cfg(test)]
mod tests {
    use super::resolve_install_location;

    #[test]
    fn rejects_empty_install_location() {
        let error = resolve_install_location("   ").expect_err("empty path should be rejected");

        assert_eq!(error.code.as_deref(), Some("invalid_install_location"));
    }

    #[test]
    fn accepts_an_existing_directory() {
        let path = std::env::current_dir().expect("test process should have a current directory");

        assert!(resolve_install_location(path.to_string_lossy().as_ref()).is_ok());
    }

    #[cfg(windows)]
    #[test]
    fn explorer_argument_path_removes_verbatim_drive_prefix() {
        let path =
            super::explorer_argument_path(std::path::Path::new(r"\\?\C:\Program Files\Rust Yu"));

        assert_eq!(path, std::path::PathBuf::from(r"C:\Program Files\Rust Yu"));
    }
}
