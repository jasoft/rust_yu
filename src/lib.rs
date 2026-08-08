pub mod commands;
pub mod modules;

pub use modules::browser_cleaner;
pub use modules::cleaner;
pub use modules::common::error::UninstallerError;
pub use modules::common::utils;
pub use modules::fluent_cleaner;
pub use modules::lister;
pub use modules::reporter;
pub use modules::scanner;
pub use modules::startup;
pub use modules::uninstall;
