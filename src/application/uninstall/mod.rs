pub mod error;
pub mod fingerprint;
pub mod models;
pub mod ports;
pub mod production;
pub mod state;
pub mod workflow;

pub use error::{UninstallError, UninstallErrorCode};
pub use fingerprint::{fingerprint_program, UninstallTargetFingerprint};
pub use models::{
    CleanupSelection, ResidueReview, UninstallEvent, UninstallEventPayload, UninstallJob,
    UninstallJobId, UninstallJobSnapshot, UninstallOutcome, UninstallPhase, UninstallPlan,
};
pub use ports::{CleanedTrace, RemovalVerification, UninstallPort, UninstallerExecution};
pub use production::ProductionUninstallPort;
pub use state::{StateTransitionError, UninstallStateMachine};
pub use workflow::{
    clean_uninstall_residues, clean_uninstall_residues_with_progress, execute_uninstall,
    execute_uninstall_with_progress, plan_uninstall,
};
