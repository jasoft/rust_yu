use super::fingerprint::UninstallTargetFingerprint;
use crate::modules::cleaner::models::CleanResult;
use crate::modules::lister::models::InstalledProgram;
use crate::modules::scanner::models::Trace;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UninstallJobId(pub String);

impl UninstallJobId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string())
    }
}

impl Default for UninstallJobId {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UninstallPhase {
    Planned,
    RunningUninstaller,
    VerifyingRemoval,
    ScanningResidues,
    AwaitingCleanupConfirmation,
    CleaningResidues,
    Completed,
    Cancelled,
    Failed,
}

impl UninstallPhase {
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Cancelled | Self::Failed)
    }

    pub const fn can_cancel(self) -> bool {
        matches!(
            self,
            Self::Planned | Self::ScanningResidues | Self::AwaitingCleanupConfirmation
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UninstallPlan {
    pub job_id: UninstallJobId,
    pub program: InstalledProgram,
    pub fingerprint: UninstallTargetFingerprint,
    pub route: String,
    pub confirmation_message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UninstallJobSnapshot {
    pub job_id: UninstallJobId,
    pub program: InstalledProgram,
    pub fingerprint: UninstallTargetFingerprint,
    pub route: String,
    pub traces: Vec<Trace>,
    pub selected_trace_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum UninstallEventPayload {
    Planned,
    UninstallerStarted {
        command_summary: String,
    },
    UninstallerCompleted {
        exit_code: Option<u32>,
        reboot_required: bool,
    },
    RemovalVerified {
        removed: bool,
    },
    ResiduesScanned {
        count: usize,
    },
    CleanupStarted {
        count: usize,
    },
    CleanupCompleted {
        success_count: usize,
        failed_count: usize,
    },
    Finished {
        success: bool,
        message: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UninstallEvent {
    pub job_id: UninstallJobId,
    pub sequence: u64,
    pub phase: UninstallPhase,
    pub payload: UninstallEventPayload,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResidueReview {
    pub traces: Vec<Trace>,
    /// 默认选择所有非关键目标；中、低置信度会在确认框中明确警告。
    pub default_selected_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupSelection {
    pub trace_ids: Vec<String>,
    pub confirm: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UninstallOutcome {
    pub success: bool,
    pub message: String,
    pub exit_code: Option<u32>,
    pub reboot_required: bool,
    pub traces_found: usize,
    pub traces_cleaned: usize,
    pub bytes_freed: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UninstallJob {
    pub snapshot: UninstallJobSnapshot,
    pub phase: UninstallPhase,
    pub next_sequence: u64,
    pub events: Vec<UninstallEvent>,
    pub residue_review: ResidueReview,
    #[serde(default)]
    pub cleanup_results: Vec<CleanResult>,
    pub outcome: Option<UninstallOutcome>,
}

impl UninstallJob {
    pub fn from_plan(plan: UninstallPlan) -> Self {
        let snapshot = UninstallJobSnapshot {
            job_id: plan.job_id,
            program: plan.program,
            fingerprint: plan.fingerprint,
            route: plan.route,
            traces: Vec::new(),
            selected_trace_ids: Vec::new(),
        };
        Self {
            snapshot,
            phase: UninstallPhase::Planned,
            next_sequence: 1,
            events: Vec::new(),
            residue_review: ResidueReview::default(),
            cleanup_results: Vec::new(),
            outcome: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::UninstallJobId;
    use std::collections::HashSet;

    #[test]
    fn generated_job_ids_are_unique() {
        let ids = (0..32).map(|_| UninstallJobId::new()).collect::<Vec<_>>();
        let unique = ids.iter().collect::<HashSet<_>>();

        assert_eq!(unique.len(), ids.len());
    }
}
