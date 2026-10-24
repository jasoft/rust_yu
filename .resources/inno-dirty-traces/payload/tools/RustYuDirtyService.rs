// RustYu 脏卸载夹具的常驻 Windows 服务（真实 SCM 服务，不是 cmd 包装）。
// 安装脚本用 sc.exe 注册为 RustYuDirtyTraces（demand start）并启动；夹具自带的
// 卸载器从不 sc delete——用来验证 Rust Yu 卸载器的"遗留服务"检测（停止 + 删除 + 文件清理）。
//
// 单文件实现，手工声明所需 Win32 API，不依赖外部 crate，
// 可用 rustc 直接编译（见 Build-DirtyTracesFixture.ps1）。
#![windows_subsystem = "windows"]

use std::ffi::c_void;

const SERVICE_NAME: &str = "RustYuDirtyTraces";
const SERVICE_WIN32_OWN_PROCESS: u32 = 0x0000_0010;
const SERVICE_STOPPED: u32 = 0x0000_0001;
const SERVICE_RUNNING: u32 = 0x0000_0004;
const SERVICE_ACCEPT_STOP: u32 = 0x0000_0001;
const SERVICE_CONTROL_STOP: u32 = 0x0000_0001;
const ERROR_SUCCESS: u32 = 0;
const INFINITE: u32 = 0xFFFF_FFFF;

#[repr(C)]
struct ServiceTableEntryW {
    service_name: *const u16,
    service_proc: Option<unsafe extern "system" fn(u32, *mut *mut u16)>,
}

#[repr(C)]
struct ServiceStatus {
    service_type: u32,
    current_state: u32,
    controls_accepted: u32,
    win32_exit_code: u32,
    service_specific_exit_code: u32,
    check_point: u32,
    wait_hint: u32,
}

type ServiceControlHandler = unsafe extern "system" fn(
    control: u32,
    event_type: u32,
    event_data: *mut c_void,
    context: *mut c_void,
) -> u32;

#[link(name = "advapi32")]
extern "system" {
    fn StartServiceCtrlDispatcherW(table: *const ServiceTableEntryW) -> i32;
    fn RegisterServiceCtrlHandlerExW(
        service_name: *const u16,
        handler: Option<ServiceControlHandler>,
        context: *mut c_void,
    ) -> *mut c_void;
    fn SetServiceStatus(service_status_handle: *mut c_void, service_status: *const ServiceStatus) -> i32;
}

#[link(name = "kernel32")]
extern "system" {
    fn CreateEventW(
        event_attributes: *mut c_void,
        manual_reset: i32,
        initial_state: i32,
        name: *const u16,
    ) -> *mut c_void;
    fn WaitForSingleObject(handle: *mut c_void, milliseconds: u32) -> u32;
    fn SetEvent(handle: *mut c_void) -> i32;
    fn CloseHandle(handle: *mut c_void) -> i32;
}

static mut STATUS_HANDLE: *mut c_void = std::ptr::null_mut();
static mut STOP_EVENT: *mut c_void = std::ptr::null_mut();

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

fn report_state(current_state: u32, controls_accepted: u32) {
    let status = ServiceStatus {
        service_type: SERVICE_WIN32_OWN_PROCESS,
        current_state,
        controls_accepted,
        win32_exit_code: 0,
        service_specific_exit_code: 0,
        check_point: 0,
        wait_hint: 0,
    };
    unsafe {
        if !STATUS_HANDLE.is_null() {
            SetServiceStatus(STATUS_HANDLE, &status);
        }
    }
}

unsafe extern "system" fn control_handler(
    control: u32,
    _event_type: u32,
    _event_data: *mut c_void,
    _context: *mut c_void,
) -> u32 {
    if control == SERVICE_CONTROL_STOP && !STOP_EVENT.is_null() {
        SetEvent(STOP_EVENT);
    }
    ERROR_SUCCESS
}

unsafe extern "system" fn service_main(_argc: u32, _argv: *mut *mut u16) {
    let name = wide(SERVICE_NAME);
    STATUS_HANDLE = RegisterServiceCtrlHandlerExW(name.as_ptr(), Some(control_handler), std::ptr::null_mut());
    if STATUS_HANDLE.is_null() {
        return;
    }
    STOP_EVENT = CreateEventW(std::ptr::null_mut(), 1, 0, std::ptr::null());
    if STOP_EVENT.is_null() {
        report_state(SERVICE_STOPPED, 0);
        return;
    }
    // 上报"运行中"后无限等待停止信号：这正是故意遗留的运行中服务
    report_state(SERVICE_RUNNING, SERVICE_ACCEPT_STOP);
    WaitForSingleObject(STOP_EVENT, INFINITE);
    report_state(SERVICE_STOPPED, 0);
    CloseHandle(STOP_EVENT);
    STOP_EVENT = std::ptr::null_mut();
}

fn main() {
    let name = wide(SERVICE_NAME);
    let table = [
        ServiceTableEntryW {
            service_name: name.as_ptr(),
            service_proc: Some(service_main),
        },
        ServiceTableEntryW {
            service_name: std::ptr::null(),
            service_proc: None,
        },
    ];
    unsafe {
        // 直接双击运行（非 SCM 启动）时 dispatcher 失败，返回非 0 退出码便于排查
        if StartServiceCtrlDispatcherW(table.as_ptr()) == 0 {
            std::process::exit(1);
        }
    }
}
