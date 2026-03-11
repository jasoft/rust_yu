pub mod manager;
pub mod models;
pub mod registry_run;
pub mod rollback;
pub mod scheduled_tasks;
pub mod services;
pub mod startup_approved;
pub mod startup_folder;

#[cfg(test)]
pub(crate) static TEST_STARTUP_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
