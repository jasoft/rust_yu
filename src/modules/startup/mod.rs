pub mod manager;
pub mod models;
pub mod registry_run;
pub mod rollback;
pub mod scheduled_tasks;
pub mod services;
pub mod startup_approved;
pub mod startup_folder;

#[cfg(test)]
// 这把锁必须与 lister::storage 的 TEST_STORAGE_ENV_LOCK 是同一个实例：
// RUST_YU_STORAGE_DIR 通过进程级环境变量传递，startup 测试与 cleaner/backup
// 测试若用两把互不相干的锁，并行运行时会互相偷换/删除对方的存储目录，
// 曾导致 backup/cleaner/startup 测试随机失败（并行复现 28/30 轮失败）。
pub(crate) static TEST_STARTUP_ENV_LOCK: &std::sync::Mutex<()> =
    &crate::modules::lister::storage::TEST_STORAGE_ENV_LOCK;
