fn main() {
    // AppManifest 是 Tauri v2 的应用 command ACL 来源；新增 handler 必须同步加入此列表。
    // capability/default.json 只引用 allow 权限，不开放任意动态命令。
    const COMMANDS: &[&str] = &[
        "scan_browser_data",
        "clean_browser_data",
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
    let attributes = tauri_build::Attributes::new()
        .app_manifest(tauri_build::AppManifest::new().commands(COMMANDS));
    if let Err(error) = tauri_build::try_build(attributes) {
        panic!("Tauri build configuration failed: {error}");
    }
}
