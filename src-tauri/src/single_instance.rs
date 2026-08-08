#[cfg(windows)]
use windows::Win32::Foundation::HANDLE;

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
            let name = format!(
                r"Global\RustYu-{}",
                std::env::var("USERNAME")
                    .unwrap_or_else(|_| unsafe { GetCurrentProcessId().to_string() })
            );
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
