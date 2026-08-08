pub mod error;
pub mod fingerprint;
pub mod models;
pub mod state;

pub use error::{UninstallError, UninstallErrorCode};
pub use fingerprint::{fingerprint_program, UninstallTargetFingerprint};
pub use models::{
    CleanupSelection, ResidueReview, UninstallEvent, UninstallEventPayload, UninstallJob,
    UninstallJobId, UninstallJobSnapshot, UninstallOutcome, UninstallPhase, UninstallPlan,
};
pub use state::{StateTransitionError, UninstallStateMachine};
