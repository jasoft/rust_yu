pub mod commands;
pub mod modules;

pub use modules::common::error::UninstallerError;
pub use modules::common::utils;
pub use modules::lister;
pub use modules::scanner;
pub use modules::cleaner;
pub use modules::reporter;
