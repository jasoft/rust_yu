#[cfg(windows)]
use windows::Win32::Foundation::HANDLE;

#[cfg(windows)]
use std::hash::{Hash, Hasher};

pub struct SingleInstanceGuard {
    #[cfg(windows)]
    handle: HANDLE,
}

impl SingleInstanceGuard {
    pub fn acquire() -> Result<Option<Self>, String> {
        #[cfg(windows)]
        {
            use std::os::windows::ffi::OsStrExt;
            use windows::core::PCWSTR;
            use windows::Win32::Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS};
            use windows::Win32::System::Threading::{CreateMutexW, GetCurrentProcessId};
            let user = std::env::var("USERNAME")
                .unwrap_or_else(|_| unsafe { GetCurrentProcessId().to_string() });
            let name = if std::env::var("RUST_YU_PORTABLE").as_deref() == Ok("1") {
                let storage_root = std::env::var("RUST_YU_STORAGE_DIR").unwrap_or_default();
                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                storage_root.hash(&mut hasher);
                format!(r"Global\RustYu-{user}-portable-{:016x}", hasher.finish())
            } else {
                format!(r"Global\RustYu-{user}")
            };
            let wide = std::ffi::OsStr::new(&name)
                .encode_wide()
                .chain(Some(0))
                .collect::<Vec<_>>();
            let handle = unsafe { CreateMutexW(None, true, PCWSTR(wide.as_ptr())) }
                .map_err(|error| error.to_string())?;
            if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
                unsafe {
                    let _ = CloseHandle(handle);
                }
                return Ok(None);
            }
            return Ok(Some(Self { handle }));
        }
        #[cfg(not(windows))]
        {
            Ok(Some(Self {}))
        }
    }
}

impl Drop for SingleInstanceGuard {
    fn drop(&mut self) {
        #[cfg(windows)]
        unsafe {
            let _ = windows::Win32::Foundation::CloseHandle(self.handle);
        }
    }
}
