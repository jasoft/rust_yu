// RustYu 脏卸载夹具的常驻守护进程。
// 安装后一直运行、且不会被夹具自带的卸载器结束——用来验证 Rust Yu 卸载器的
// "卸载前检测运行中进程"（sysinfo 枚举 + Kill Process）路径。
//
// 停止方式：触发命名事件 Global\RustYuDirtyTracesStop（SetEvent），或直接结束进程。
// 单文件实现，不依赖外部 crate，可用 rustc 直接编译（见 Build-DirtyTracesFixture.ps1）。
#![windows_subsystem = "windows"]

use std::ffi::c_void;
use std::path::PathBuf;

const STOP_EVENT_NAME: &str = r"Global\RustYuDirtyTracesStop";
const TICK_MS: u32 = 1000;
// 每 60 个 tick（约 60 秒）覆盖写一次心跳，避免日志无限增长
const HEARTBEAT_TICKS: u32 = 60;
const WAIT_OBJECT_0: u32 = 0x0000_0000;
const ERROR_ALREADY_EXISTS: u32 = 183;

#[link(name = "kernel32")]
extern "system" {
    fn CreateEventW(
        event_attributes: *mut c_void,
        manual_reset: i32,
        initial_state: i32,
        name: *const u16,
    ) -> *mut c_void;
    fn WaitForSingleObject(handle: *mut c_void, milliseconds: u32) -> u32;
    fn CloseHandle(handle: *mut c_void) -> i32;
    fn GetLastError() -> u32;
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0)
}

fn heartbeat_path() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|error| format!("current_exe failed: {error}"))?;
    let exe_dir = exe.parent().map(PathBuf::from).ok_or_else(|| String::from("exe has no parent dir"))?;
    let logs_dir = exe_dir.join("logs");
    std::fs::create_dir_all(&logs_dir).map_err(|error| format!("create logs dir failed: {error}"))?;
    Ok(logs_dir.join("daemon-heartbeat.log"))
}

fn run(stop_event: *mut c_void) -> Result<(), String> {
    let heartbeat = heartbeat_path()?;
    let header = format!("pid={} started={}\n", std::process::id(), unix_now());
    std::fs::write(&heartbeat, &header).map_err(|error| format!("write heartbeat failed: {error}"))?;

    let mut tick: u32 = 0;
    loop {
        // 等待停止信号；每秒醒来一次是为了让杀进程之外的停止方式也能及时生效
        let wait_result = unsafe { WaitForSingleObject(stop_event, TICK_MS) };
        if wait_result == WAIT_OBJECT_0 {
            break;
        }
        tick = tick.wrapping_add(1);
        if tick % HEARTBEAT_TICKS == 0 {
            let _ = std::fs::write(&heartbeat, format!("{header}heartbeat={}\n", unix_now()));
        }
    }
    Ok(())
}

fn main() {
    let name = wide(STOP_EVENT_NAME);
    let stop_event = unsafe { CreateEventW(std::ptr::null_mut(), 1, 0, name.as_ptr()) };
    // 命名事件已存在说明有另一个守护进程在跑，直接退出避免重复驻留
    if stop_event.is_null() || unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        std::process::exit(0);
    }
    if let Err(error) = run(stop_event) {
        let _ = error;
        std::process::exit(1);
    }
    unsafe {
        CloseHandle(stop_event);
    }
}
