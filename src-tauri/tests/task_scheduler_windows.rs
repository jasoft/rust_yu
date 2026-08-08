#![cfg(windows)]

use rust_yu_tauri_lib::elevation::{
    create_or_repair_current_user_task, inspect_current_user_task, remove_all_product_tasks,
    run_current_user_task, validate_current_user_task,
};
use std::path::PathBuf;

#[test]
#[ignore = "requires an administrator Windows test terminal"]
fn protected_task_lifecycle_uses_system_binary_and_cleans_up() {
    let system_root = std::env::var_os("SystemRoot").expect("SystemRoot is required");
    let executable = PathBuf::from(system_root)
        .join("System32")
        .join("whoami.exe");

    remove_all_product_tasks().expect("stale test task should be removable");
    create_or_repair_current_user_task(&executable).expect("task should be created");
    assert!(inspect_current_user_task()
        .expect("task should be inspected")
        .is_some());
    validate_current_user_task(&executable).expect("task definition should match");
    run_current_user_task().expect("task should run");
    remove_all_product_tasks().expect("task should be removed");
    assert!(inspect_current_user_task()
        .expect("task should be inspected")
        .is_none());
}
