use std::env;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mode = env::args()
        .nth(1)
        .unwrap_or_else(|| String::from("interactive"));
    let exe_dir = current_exe_dir()?;
    let worker_script = exe_dir.join("UninstallWorker.ps1");
    if !worker_script.is_file() {
        return Err(format!("Missing worker script: {}", worker_script.display()));
    }

    let mut command = Command::new(powershell_path());
    command
        .arg("-NoProfile")
        .arg("-ExecutionPolicy")
        .arg("Bypass")
        .arg("-File")
        .arg(worker_script)
        .arg("-Mode")
        .arg(mode)
        .creation_flags(CREATE_NO_WINDOW);

    command
        .spawn()
        .map_err(|error| format!("Failed to spawn uninstall worker: {error}"))?;

    Ok(())
}

fn current_exe_dir() -> Result<PathBuf, String> {
    let exe = env::current_exe().map_err(|error| format!("Failed to locate current exe: {error}"))?;
    exe.parent()
        .map(PathBuf::from)
        .ok_or_else(|| String::from("Executable has no parent directory"))
}

fn powershell_path() -> PathBuf {
    env::var_os("SystemRoot")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"))
        .join("System32")
        .join("WindowsPowerShell")
        .join("v1.0")
        .join("powershell.exe")
}
