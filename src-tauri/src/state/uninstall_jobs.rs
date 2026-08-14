use rust_yu_lib::application::uninstall::{
    UninstallError, UninstallErrorCode, UninstallJob, UninstallPhase,
};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};

#[derive(Debug, Default)]
struct CoordinatorState {
    active_job: Option<UninstallJob>,
    planning: bool,
    operation_in_flight: bool,
    cancellation: Option<Arc<AtomicBool>>,
}

/// Tauri 进程内的单 job 协调器。
///
/// 锁只保护快照和短状态转换；实际卸载器、扫描与清理永远在锁外执行。
#[derive(Debug, Default)]
pub struct UninstallCoordinator {
    state: Mutex<CoordinatorState>,
}

impl UninstallCoordinator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn begin_plan(&self) -> Result<(), UninstallError> {
        let mut state = self.lock_state()?;
        if state.planning || state.operation_in_flight || has_active_job(&state) {
            return Err(UninstallError::new(
                UninstallErrorCode::JobConflict,
                "已有未完成的卸载任务",
            ));
        }
        state.planning = true;
        Ok(())
    }

    pub fn commit_plan(&self, job: UninstallJob) -> Result<(), UninstallError> {
        let mut state = self.lock_state()?;
        if !state.planning {
            return Err(UninstallError::invalid_state("没有正在提交的卸载计划"));
        }
        state.active_job = Some(job);
        state.cancellation = Some(Arc::new(AtomicBool::new(false)));
        state.planning = false;
        Ok(())
    }

    pub fn abort_plan(&self) -> Result<(), UninstallError> {
        let mut state = self.lock_state()?;
        state.planning = false;
        Ok(())
    }

    pub fn begin_operation(
        &self,
        job_id: &str,
        expected_phase: UninstallPhase,
    ) -> Result<UninstallJob, UninstallError> {
        let mut state = self.lock_state()?;
        if state.planning || state.operation_in_flight {
            return Err(UninstallError::new(
                UninstallErrorCode::JobConflict,
                "已有卸载操作正在执行",
            ));
        }
        let job = state
            .active_job
            .as_ref()
            .ok_or_else(|| UninstallError::new(UninstallErrorCode::JobNotFound, "卸载任务不存在"))?
            .clone();
        if job.snapshot.job_id.0 != job_id {
            return Err(UninstallError::new(
                UninstallErrorCode::JobNotFound,
                "卸载任务不存在",
            ));
        }
        if job.phase != expected_phase {
            return Err(UninstallError::new(
                UninstallErrorCode::InvalidJobState,
                format!("当前任务阶段为 {:?}，不能执行该操作", job.phase),
            ));
        }
        state.operation_in_flight = true;
        Ok(job)
    }

    pub fn commit_operation(&self, job: UninstallJob) -> Result<(), UninstallError> {
        let mut state = self.lock_state()?;
        if !state.operation_in_flight {
            return Err(UninstallError::invalid_state("没有正在提交的卸载操作"));
        }
        state.active_job = Some(job);
        state.operation_in_flight = false;
        Ok(())
    }

    pub fn abort_operation(&self) -> Result<(), UninstallError> {
        let mut state = self.lock_state()?;
        state.operation_in_flight = false;
        Ok(())
    }

    pub fn snapshot(&self, job_id: &str) -> Result<UninstallJob, UninstallError> {
        let state = self.lock_state()?;
        let job = state.active_job.as_ref().ok_or_else(|| {
            UninstallError::new(UninstallErrorCode::JobNotFound, "卸载任务不存在")
        })?;
        if job.snapshot.job_id.0 != job_id {
            return Err(UninstallError::new(
                UninstallErrorCode::JobNotFound,
                "卸载任务不存在",
            ));
        }
        Ok(job.clone())
    }

    pub fn cancellation_token(&self, job_id: &str) -> Result<Arc<AtomicBool>, UninstallError> {
        let state = self.lock_state()?;
        verify_job_id(&state, job_id)?;
        state.cancellation.as_ref().cloned().ok_or_else(|| {
            UninstallError::new(
                UninstallErrorCode::InvalidJobState,
                "当前卸载任务没有可用的取消信号",
            )
        })
    }

    pub fn request_cancellation(&self, job_id: &str) -> Result<(), UninstallError> {
        let state = self.lock_state()?;
        verify_job_id(&state, job_id)?;
        if !state.operation_in_flight {
            return Err(UninstallError::new(
                UninstallErrorCode::InvalidJobState,
                "当前没有可中断的卸载扫描",
            ));
        }
        let cancellation = state.cancellation.as_ref().ok_or_else(|| {
            UninstallError::new(
                UninstallErrorCode::InvalidJobState,
                "当前卸载任务没有可用的取消信号",
            )
        })?;
        cancellation.store(true, Ordering::SeqCst);
        Ok(())
    }

    fn lock_state(&self) -> Result<std::sync::MutexGuard<'_, CoordinatorState>, UninstallError> {
        self.state.lock().map_err(|_| {
            UninstallError::new(UninstallErrorCode::JobConflict, "卸载任务状态锁已损坏")
        })
    }
}

fn verify_job_id(state: &CoordinatorState, job_id: &str) -> Result<(), UninstallError> {
    let job = state
        .active_job
        .as_ref()
        .ok_or_else(|| UninstallError::new(UninstallErrorCode::JobNotFound, "卸载任务不存在"))?;
    if job.snapshot.job_id.0 != job_id {
        return Err(UninstallError::new(
            UninstallErrorCode::JobNotFound,
            "卸载任务不存在",
        ));
    }
    Ok(())
}

fn has_active_job(state: &CoordinatorState) -> bool {
    state
        .active_job
        .as_ref()
        .is_some_and(|job| !job.phase.is_terminal())
}

#[cfg(test)]
mod tests {
    use super::UninstallCoordinator;
    use rust_yu_lib::application::uninstall::{
        fingerprint_program, UninstallJob, UninstallJobId, UninstallPhase, UninstallPlan,
        UninstallTargetFingerprint,
    };
    use rust_yu_lib::modules::lister::models::{InstallSource, InstalledProgram};

    fn planned_job() -> UninstallJob {
        let program = InstalledProgram::new("Demo".to_string(), InstallSource::Registry);
        UninstallJob::from_plan(UninstallPlan {
            job_id: UninstallJobId::new(),
            fingerprint: UninstallTargetFingerprint(fingerprint_program(&program).0),
            route: "legacy".to_string(),
            confirmation_message: "confirm".to_string(),
            program,
        })
    }

    #[test]
    fn only_one_active_job_can_be_planned() {
        let coordinator = UninstallCoordinator::new();
        coordinator
            .begin_plan()
            .expect("first plan should reserve slot");
        assert!(coordinator.begin_plan().is_err());
        coordinator
            .commit_plan(planned_job())
            .expect("plan should commit");
        assert!(coordinator.begin_plan().is_err());
    }

    #[test]
    fn unknown_job_and_invalid_phase_are_structured_errors() {
        let coordinator = UninstallCoordinator::new();
        let unknown = coordinator
            .begin_operation("missing", UninstallPhase::Planned)
            .expect_err("unknown job should be rejected");
        assert_eq!(unknown.code.as_str(), "job_not_found");

        coordinator.begin_plan().expect("slot should be available");
        let job = planned_job();
        let job_id = job.snapshot.job_id.0.clone();
        coordinator.commit_plan(job).expect("plan should commit");
        let invalid = coordinator
            .begin_operation(&job_id, UninstallPhase::AwaitingCleanupConfirmation)
            .expect_err("wrong phase should be rejected");
        assert_eq!(invalid.code.as_str(), "invalid_job_state");
    }

    #[test]
    fn terminal_job_allows_next_plan_and_snapshot_is_read_only() {
        let coordinator = UninstallCoordinator::new();
        coordinator
            .begin_plan()
            .expect("first plan should reserve slot");
        let mut job = planned_job();
        let job_id = job.snapshot.job_id.0.clone();
        job.phase = UninstallPhase::Completed;
        coordinator
            .commit_plan(job)
            .expect("terminal job should commit");
        let snapshot = coordinator
            .snapshot(&job_id)
            .expect("snapshot should exist");
        assert_eq!(snapshot.phase, UninstallPhase::Completed);
        coordinator
            .begin_plan()
            .expect("terminal job should release slot");
    }

    #[test]
    fn in_flight_operation_exposes_a_shared_cancellation_signal() {
        let coordinator = UninstallCoordinator::new();
        coordinator.begin_plan().expect("slot should be available");
        let job = planned_job();
        let job_id = job.snapshot.job_id.0.clone();
        coordinator.commit_plan(job).expect("plan should commit");
        let token = coordinator
            .cancellation_token(&job_id)
            .expect("planned job should have a cancellation token");
        coordinator
            .begin_operation(&job_id, UninstallPhase::Planned)
            .expect("operation should start");

        coordinator
            .request_cancellation(&job_id)
            .expect("in-flight scan should accept cancellation");

        assert!(token.load(std::sync::atomic::Ordering::SeqCst));
    }
}
