use super::error::{UninstallError, UninstallErrorCode};
use super::models::{UninstallEvent, UninstallEventPayload, UninstallJob, UninstallPhase};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StateTransitionError;

pub struct UninstallStateMachine<'a> {
    job: &'a mut UninstallJob,
}

impl<'a> UninstallStateMachine<'a> {
    pub fn new(job: &'a mut UninstallJob) -> Self {
        Self { job }
    }

    pub fn transition(
        &mut self,
        next: UninstallPhase,
        payload: UninstallEventPayload,
    ) -> Result<UninstallEvent, UninstallError> {
        if !is_valid_transition(self.job.phase, next) {
            return Err(UninstallError::new(
                UninstallErrorCode::InvalidJobState,
                format!("不能从 {:?} 转换到 {:?}", self.job.phase, next),
            ));
        }

        self.job.phase = next;
        let event = UninstallEvent {
            job_id: self.job.snapshot.job_id.clone(),
            sequence: self.job.next_sequence,
            phase: next,
            payload,
        };
        self.job.next_sequence = self.job.next_sequence.saturating_add(1);
        self.job.events.push(event.clone());
        Ok(event)
    }

    pub fn cancel(&mut self) -> Result<UninstallEvent, UninstallError> {
        if !self.job.phase.can_cancel() {
            return Err(UninstallError::new(
                UninstallErrorCode::InvalidJobState,
                "卸载执行阶段不能取消；请等待卸载器完成",
            ));
        }
        self.transition(
            UninstallPhase::Cancelled,
            UninstallEventPayload::Finished {
                success: false,
                message: "用户取消卸载".to_string(),
            },
        )
    }
}

fn is_valid_transition(current: UninstallPhase, next: UninstallPhase) -> bool {
    matches!(
        (current, next),
        (UninstallPhase::Planned, UninstallPhase::RunningUninstaller)
            | (UninstallPhase::Planned, UninstallPhase::Cancelled)
            | (
                UninstallPhase::RunningUninstaller,
                UninstallPhase::VerifyingRemoval
            )
            | (UninstallPhase::RunningUninstaller, UninstallPhase::Failed)
            | (
                UninstallPhase::VerifyingRemoval,
                UninstallPhase::ScanningResidues
            )
            | (UninstallPhase::VerifyingRemoval, UninstallPhase::Failed)
            | (
                UninstallPhase::ScanningResidues,
                UninstallPhase::AwaitingCleanupConfirmation
            )
            | (UninstallPhase::ScanningResidues, UninstallPhase::Completed)
            | (UninstallPhase::ScanningResidues, UninstallPhase::Failed)
            | (
                UninstallPhase::AwaitingCleanupConfirmation,
                UninstallPhase::CleaningResidues
            )
            | (
                UninstallPhase::AwaitingCleanupConfirmation,
                UninstallPhase::Completed
            )
            | (
                UninstallPhase::AwaitingCleanupConfirmation,
                UninstallPhase::Cancelled
            )
            | (UninstallPhase::CleaningResidues, UninstallPhase::Completed)
            | (UninstallPhase::CleaningResidues, UninstallPhase::Failed)
    )
}

#[cfg(test)]
mod tests {
    use super::{UninstallPhase, UninstallStateMachine};
    use crate::application::uninstall::fingerprint::UninstallTargetFingerprint;
    use crate::application::uninstall::models::{
        UninstallEventPayload, UninstallJob, UninstallJobId, UninstallPlan,
    };
    use crate::modules::lister::models::{InstallSource, InstalledProgram};

    fn job() -> UninstallJob {
        let program = InstalledProgram::new("Demo".to_string(), InstallSource::Registry);
        UninstallJob::from_plan(UninstallPlan {
            job_id: UninstallJobId::new(),
            fingerprint: UninstallTargetFingerprint("test".to_string()),
            route: "legacy".to_string(),
            confirmation_message: "confirm".to_string(),
            program,
        })
    }

    #[test]
    fn valid_transitions_and_monotonic_event_sequence() {
        let mut job = job();
        let mut state = UninstallStateMachine::new(&mut job);
        let first = state
            .transition(
                UninstallPhase::RunningUninstaller,
                UninstallEventPayload::UninstallerStarted {
                    command_summary: "uninstall".to_string(),
                },
            )
            .expect("planned job should start");
        let second = state
            .transition(
                UninstallPhase::VerifyingRemoval,
                UninstallEventPayload::UninstallerCompleted {
                    exit_code: Some(0),
                    reboot_required: false,
                },
            )
            .expect("running job should verify");

        assert_eq!(first.sequence, 1);
        assert_eq!(second.sequence, 2);
    }

    #[test]
    fn execution_cannot_skip_verification_and_scanning() {
        let mut job = job();
        let mut state = UninstallStateMachine::new(&mut job);
        let error = state
            .transition(
                UninstallPhase::AwaitingCleanupConfirmation,
                UninstallEventPayload::ResiduesScanned { count: 0 },
            )
            .expect_err("running phase must not skip verification and scan");

        assert_eq!(error.code.as_str(), "invalid_job_state");
    }

    #[test]
    fn running_phase_cannot_cancel_but_planned_phase_can() {
        let mut planned = job();
        let mut planned_state = UninstallStateMachine::new(&mut planned);
        planned_state.cancel().expect("planned job can cancel");

        let mut running = job();
        let mut running_state = UninstallStateMachine::new(&mut running);
        running_state
            .transition(
                UninstallPhase::RunningUninstaller,
                UninstallEventPayload::UninstallerStarted {
                    command_summary: "uninstall".to_string(),
                },
            )
            .expect("planned job should start");
        assert!(running_state.cancel().is_err());
    }
}
