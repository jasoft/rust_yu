#[allow(dead_code)]
pub mod browser_cleaner;
pub mod cleaner;
pub mod common;
// 该模块只由 Tauri UI 调用；CLI 目标仍会编译共享模块树。
#[allow(dead_code)]
pub mod fluent_cleaner;
pub mod lister;
pub mod reporter;
pub mod scanner;
pub mod startup;
pub mod uninstall;
