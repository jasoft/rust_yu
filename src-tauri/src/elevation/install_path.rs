use super::error::{ElevationError, ElevationErrorCode};
use std::path::{Path, PathBuf};

pub fn validate_protected_install_path(path: &Path) -> Result<PathBuf, ElevationError> {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        return Err(unsafe_path("安装目录必须是本地绝对路径"));
    };
    let text = absolute.to_string_lossy().to_lowercase();
    if text.starts_with("\\\\") || text.contains("\\\\?\\") {
        return Err(unsafe_path("不允许使用 UNC 或设备路径"));
    }
    if is_user_writable_location(&absolute) {
        return Err(unsafe_path("安装目录位于普通用户可写位置"));
    }
    let normalized = text.replace('/', "\\");
    if !(normalized.starts_with("c:\\program files\\")
        || normalized.starts_with("c:\\program files (x86)\\"))
    {
        return Err(unsafe_path("正式安装目录必须位于 Program Files"));
    }
    if absolute
        .components()
        .any(|component| component.as_os_str() == "..")
    {
        return Err(unsafe_path("安装目录不能包含父目录跳转"));
    }
    if absolute.exists() {
        let metadata = std::fs::symlink_metadata(&absolute)
            .map_err(|error| unsafe_path(format!("无法检查安装目录: {error}")))?;
        if metadata.file_type().is_symlink() {
            return Err(unsafe_path("安装目录不能是符号链接"));
        }
    }
    Ok(absolute)
}

pub fn validate_protected_executable(path: &Path) -> Result<PathBuf, ElevationError> {
    let parent = path
        .parent()
        .ok_or_else(|| unsafe_path("管理员入口 EXE 缺少父目录"))?;
    let executable = validate_protected_install_path(parent)?;
    let file = if path.is_absolute() {
        path.to_path_buf()
    } else {
        executable.join(path)
    };
    if !file.is_file() {
        return Err(unsafe_path("管理员入口 EXE 不存在"));
    }
    let metadata = std::fs::symlink_metadata(&file)
        .map_err(|error| unsafe_path(format!("无法检查管理员入口 EXE: {error}")))?;
    if metadata.file_type().is_symlink() {
        return Err(unsafe_path("管理员入口 EXE 不能是符号链接"));
    }
    Ok(file)
}

fn is_user_writable_location(path: &Path) -> bool {
    let candidates = [
        std::env::var_os("TEMP"),
        std::env::var_os("TMP"),
        std::env::var_os("LOCALAPPDATA"),
        std::env::var_os("APPDATA"),
        std::env::var_os("USERPROFILE"),
    ];
    candidates
        .iter()
        .flatten()
        .map(PathBuf::from)
        .any(|candidate| path.starts_with(candidate))
}

fn unsafe_path(message: impl Into<String>) -> ElevationError {
    ElevationError::new(ElevationErrorCode::UnsafeInstallLocation, message)
}

#[cfg(test)]
mod tests {
    use super::validate_protected_install_path;
    use std::path::Path;

    #[test]
    fn rejects_user_writable_and_non_program_files_locations() {
        assert!(validate_protected_install_path(Path::new(r"C:\Temp\RustYu")).is_err());
        assert!(validate_protected_install_path(Path::new(r"C:\Users\Public\RustYu")).is_err());
        assert!(validate_protected_install_path(Path::new(r"C:\Program Files\Rust Yu")).is_ok());
    }

    #[test]
    fn rejects_unc_paths() {
        assert!(validate_protected_install_path(Path::new(r"\\server\share\RustYu")).is_err());
    }
}
