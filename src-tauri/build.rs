fn main() {
    // AppManifest 是 Tauri v2 的应用 command ACL 来源；新增 handler 必须同步加入此列表。
    // capability/default.json 只引用 allow 权限，不开放任意动态命令。
    const COMMANDS: &[&str] = &[
        "plan_backup",
        "list_backup_sessions",
        "get_backup_session",
        "restore_backup_session",
        "plan_install_monitor",
        "start_install_monitor",
        "complete_install_monitor",
        "list_install_monitor_sessions",
        "get_install_monitor_session",
        "get_install_monitor_traces",
        "export_install_monitor",
        "get_force_uninstall_startup_target",
        "get_force_uninstall_context_menu",
        "set_force_uninstall_context_menu",
        "capture_hunter_target",
        "scan_browser_data",
        "clean_browser_data",
        "get_program_health",
        "list_programs",
        "warmup_program_metadata",
        "search_programs",
        "scan_traces",
        "clean_traces",
        "list_cleaner_entries",
        "scan_cleaner_entries",
        "clean_cleaner_entries",
        "plan_uninstall",
        "execute_uninstall",
        "clean_uninstall_residues",
        "finish_uninstall",
        "get_uninstall_job",
        "get_reports",
        "get_report",
        "export_report",
        "delete_report",
        "list_startup_items",
        "get_startup_item",
        "list_startup_sources",
        "plan_startup_action",
        "apply_startup_action",
        "rollback_startup_action",
        "plan_add_startup_item",
        "add_startup_item",
    ];
    // GUI 本身必须在管理员令牌下运行；Windows 会在进入 Rust main 之前根据
    // requestedExecutionLevel=requireAdministrator 请求 UAC，拒绝时不会创建 WebView。
    let windows_attributes = tauri_build::WindowsAttributes::new()
        .app_manifest(include_str!("windows-app-manifest.xml"));
    let attributes = tauri_build::Attributes::new()
        .windows_attributes(windows_attributes)
        .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS));
    if let Err(error) = tauri_build::try_build(attributes) {
        panic!("Tauri build configuration failed: {error}");
    }
}
