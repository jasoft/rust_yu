pub mod error;
pub mod install_path;
pub mod task_definition;
pub mod task_scheduler;
pub mod token;

pub use error::{ElevationError, ElevationErrorCode};
pub use install_path::{validate_protected_executable, validate_protected_install_path};
pub use task_definition::{TaskDefinition, ELEVATED_ENTRY_ARGUMENT};
pub use task_scheduler::{
    create_or_repair_current_user_task, inspect_current_user_task, remove_all_product_tasks,
    remove_current_user_task, run_current_user_task, validate_current_user_task,
};
pub use token::{current_token_state, TokenState};
