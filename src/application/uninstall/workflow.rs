use super::error::{UninstallError, UninstallErrorCode};
use super::fingerprint::fingerprint_program;
use super::models::{
    CleanupSelection, ResidueReview, UninstallEventPayload, UninstallJob, UninstallOutcome,
    UninstallPhase, UninstallPlan,
};
use super::ports::UninstallPort;
use super::state::UninstallStateMachine;
use crate::modules::scanner::models::Trace;

pub async fn plan_uninstall<P: UninstallPort>(
    port: &P,
    program_id: &str,
) -> Result<UninstallJob, UninstallError> {
    let program = port.resolve_program_by_id(program_id).await?;
    let fingerprint = fingerprint_program(&program);
    port.save_snapshot(&program).await?;

    let plan = UninstallPlan {
        job_id: super::models::UninstallJobId::new(),
        route: crate::modules::uninstall::route_name(program.uninstall_kind).to_string(),
        confirmation_message: format!(
            "确认卸载 {}？卸载器结束后将继续核验和扫描残留。",
            program.name
        ),
        fingerprint,
        program,
    };
    let mut job = UninstallJob::from_plan(plan);
    UninstallStateMachine::new(&mut job)
        .transition(UninstallPhase::Planned, UninstallEventPayload::Planned)?;
    Ok(job)
}

pub async fn execute_uninstall<P: UninstallPort>(
    port: &P,
    job: &mut UninstallJob,
    timeout_secs: u64,
) -> Result<ResidueReview, UninstallError> {
    require_phase(job, UninstallPhase::Planned)?;

    let current = port.resolve_program_by_id(&job.snapshot.program.id).await?;
    if fingerprint_program(&current) != job.snapshot.fingerprint {
        return fail_job(
            job,
            UninstallError::new(
                UninstallErrorCode::TargetChanged,
                "目标程序信息已变化，请重新生成卸载计划",
            ),
        );
    }

    port.ensure_administrator().await?;
    port.save_snapshot(&job.snapshot.program).await?;
    transition(
        job,
        UninstallPhase::RunningUninstaller,
        UninstallEventPayload::UninstallerStarted {
            command_summary: "已交由受控卸载器执行".to_string(),
        },
    )?;

    let execution = match port
        .run_uninstaller(&job.snapshot.program, timeout_secs)
        .await
    {
        Ok(result) => result,
        Err(error) => return fail_job(job, error),
    };
    if execution.user_cancelled {
        return fail_job(
            job,
            UninstallError::new(UninstallErrorCode::UninstallerCancelled, "用户取消了卸载"),
        );
    }
    if !execution.successful {
        return fail_job(
            job,
            UninstallError::new(UninstallErrorCode::UninstallerFailed, "卸载器返回失败状态"),
        );
    }
    transition(
        job,
        UninstallPhase::VerifyingRemoval,
        UninstallEventPayload::UninstallerCompleted {
            exit_code: execution.exit_code,
            reboot_required: execution.reboot_required,
        },
    )?;

    let verification = match port
        .verify_removal(&job.snapshot.program, timeout_secs)
        .await
    {
        Ok(result) => result,
        Err(error) => return fail_job(job, error),
    };
    if !verification.removed {
        return fail_job(
            job,
            UninstallError::new(
                UninstallErrorCode::RemovalNotConfirmed,
                "卸载命令已结束，但程序仍未确认移除",
            ),
        );
    }
    transition(
        job,
        UninstallPhase::ScanningResidues,
        UninstallEventPayload::RemovalVerified { removed: true },
    )?;

    let traces = match port.scan_residues(&job.snapshot.program).await {
        Ok(traces) => traces
            .into_iter()
            .filter(|trace| trace.exists)
            .collect::<Vec<_>>(),
        Err(error) => return fail_job(job, error),
    };
    job.snapshot.traces = traces.clone();
    job.residue_review = ResidueReview {
        traces,
        default_selected_ids: Vec::new(),
    };
    if job.residue_review.traces.is_empty() {
        transition(
            job,
            UninstallPhase::Completed,
            UninstallEventPayload::ResiduesScanned { count: 0 },
        )?;
        job.outcome = Some(UninstallOutcome {
            success: true,
            message: "卸载完成，未发现残留".to_string(),
            exit_code: execution.exit_code,
            reboot_required: execution.reboot_required,
            ..UninstallOutcome::default()
        });
    } else {
        transition(
            job,
            UninstallPhase::AwaitingCleanupConfirmation,
            UninstallEventPayload::ResiduesScanned {
                count: job.residue_review.traces.len(),
            },
        )?;
    }

    Ok(job.residue_review.clone())
}

pub async fn clean_uninstall_residues<P: UninstallPort>(
    port: &P,
    job: &mut UninstallJob,
    selection: CleanupSelection,
) -> Result<UninstallOutcome, UninstallError> {
    require_phase(job, UninstallPhase::AwaitingCleanupConfirmation)?;
    if !selection.confirm {
        return Err(UninstallError::new(
            UninstallErrorCode::ConfirmationRequired,
            "清理残留必须明确确认",
        ));
    }

    let requested = selection.trace_ids;
    let mut selected = Vec::with_capacity(requested.len());
    for trace_id in &requested {
        let trace = job
            .snapshot
            .traces
            .iter()
            .find(|candidate| candidate.id == *trace_id)
            .ok_or_else(|| {
                UninstallError::new(
                    UninstallErrorCode::TraceNotInPlan,
                    "清理目标不属于当前卸载计划",
                )
            })?;
        if selected.iter().any(|item: &Trace| item.id == trace.id) {
            return Err(UninstallError::new(
                UninstallErrorCode::TraceNotInPlan,
                "清理目标包含重复的 trace id",
            ));
        }
        selected.push(trace.clone());
    }

    if selected.is_empty() {
        transition(
            job,
            UninstallPhase::Completed,
            UninstallEventPayload::Finished {
                success: true,
                message: "卸载完成，已跳过残留清理".to_string(),
            },
        )?;
        let outcome = UninstallOutcome {
            success: true,
            message: "卸载完成，已跳过残留清理".to_string(),
            traces_found: job.snapshot.traces.len(),
            ..UninstallOutcome::default()
        };
        job.outcome = Some(outcome.clone());
        return Ok(outcome);
    }

    job.snapshot.selected_trace_ids = requested;
    transition(
        job,
        UninstallPhase::CleaningResidues,
        UninstallEventPayload::CleanupStarted {
            count: selected.len(),
        },
    )?;
    let cleaned = match port.clean_traces(&selected).await {
        Ok(results) => results,
        Err(error) => return fail_job(job, error),
    };
    port.invalidate_cache(&job.snapshot.program.id).await?;
    let success_count = cleaned.iter().filter(|item| item.success).count();
    let failed_count = cleaned.len().saturating_sub(success_count);
    let bytes_freed = cleaned
        .iter()
        .filter(|item| item.success)
        .map(|item| item.bytes_freed)
        .sum();
    let outcome = UninstallOutcome {
        success: failed_count == 0,
        message: format!("卸载完成，已清理 {success_count} 项残留"),
        traces_found: job.snapshot.traces.len(),
        traces_cleaned: success_count,
        bytes_freed,
        ..UninstallOutcome::default()
    };
    transition(
        job,
        if outcome.success {
            UninstallPhase::Completed
        } else {
            UninstallPhase::Failed
        },
        UninstallEventPayload::CleanupCompleted {
            success_count,
            failed_count,
        },
    )?;
    job.outcome = Some(outcome.clone());
    Ok(outcome)
}

fn require_phase(job: &UninstallJob, expected: UninstallPhase) -> Result<(), UninstallError> {
    if job.phase != expected {
        return Err(UninstallError::invalid_state(format!(
            "当前卸载阶段为 {:?}，需要 {:?}",
            job.phase, expected
        )));
    }
    Ok(())
}

fn transition(
    job: &mut UninstallJob,
    phase: UninstallPhase,
    payload: UninstallEventPayload,
) -> Result<(), UninstallError> {
    UninstallStateMachine::new(job)
        .transition(phase, payload)
        .map(|_| ())
}

fn fail_job<T>(job: &mut UninstallJob, error: UninstallError) -> Result<T, UninstallError> {
    if !job.phase.is_terminal() {
        let _ = transition(
            job,
            UninstallPhase::Failed,
            UninstallEventPayload::Finished {
                success: false,
                message: error.message.clone(),
            },
        );
    }
    Err(error)
}

#[cfg(test)]
mod tests {
    use super::{clean_uninstall_residues, execute_uninstall, plan_uninstall};
    use crate::application::uninstall::error::{UninstallError, UninstallErrorCode};
    use crate::application::uninstall::models::{CleanupSelection, UninstallJob};
    use crate::application::uninstall::ports::{
        CleanedTrace, RemovalVerification, UninstallPort, UninstallerExecution,
    };
    use crate::modules::lister::models::{InstallSource, InstalledProgram};
    use crate::modules::scanner::models::{Confidence, Trace, TraceType};
    use async_trait::async_trait;
    use std::sync::{Arc, Mutex};

    #[derive(Clone)]
    struct FakePort {
        calls: Arc<Mutex<Vec<&'static str>>>,
        program: InstalledProgram,
        traces: Vec<Trace>,
        fail_scan: bool,
    }

    impl FakePort {
        fn new() -> Self {
            let mut program = InstalledProgram::new("Demo".to_string(), InstallSource::Registry);
            program.id = "demo-id".to_string();
            program.uninstall_string = Some("demo-uninstall /S".to_string());
            Self {
                calls: Arc::new(Mutex::new(Vec::new())),
                program,
                traces: vec![Trace::new(
                    "Demo".to_string(),
                    TraceType::File,
                    r"C:\Demo.leftover".to_string(),
                )
                .with_confidence(Confidence::High)],
                fail_scan: false,
            }
        }

        fn record(&self, name: &'static str) {
            self.calls
                .lock()
                .expect("test mutex should not be poisoned")
                .push(name);
        }
    }

    #[async_trait]
    impl UninstallPort for FakePort {
        async fn resolve_program_by_id(
            &self,
            _program_id: &str,
        ) -> Result<InstalledProgram, UninstallError> {
            self.record("resolve");
            Ok(self.program.clone())
        }

        async fn save_snapshot(&self, _program: &InstalledProgram) -> Result<(), UninstallError> {
            self.record("snapshot");
            Ok(())
        }

        async fn ensure_administrator(&self) -> Result<(), UninstallError> {
            self.record("precheck");
            Ok(())
        }

        async fn run_uninstaller(
            &self,
            _program: &InstalledProgram,
            _timeout_secs: u64,
        ) -> Result<UninstallerExecution, UninstallError> {
            self.record("run");
            Ok(UninstallerExecution {
                successful: true,
                exit_code: Some(0),
                reboot_required: false,
                user_cancelled: false,
                used_job_object: true,
            })
        }

        async fn verify_removal(
            &self,
            _program: &InstalledProgram,
            _timeout_secs: u64,
        ) -> Result<RemovalVerification, UninstallError> {
            self.record("verify");
            Ok(RemovalVerification {
                removed: true,
                still_registered: false,
                install_dir_exists: false,
                store_package_present: false,
            })
        }

        async fn scan_residues(
            &self,
            _program: &InstalledProgram,
        ) -> Result<Vec<Trace>, UninstallError> {
            self.record("scan");
            if self.fail_scan {
                return Err(UninstallError::new(
                    UninstallErrorCode::ResidueScanFailed,
                    "scan failed",
                ));
            }
            Ok(self.traces.clone())
        }

        async fn clean_traces(
            &self,
            _traces: &[Trace],
        ) -> Result<Vec<CleanedTrace>, UninstallError> {
            self.record("clean");
            Ok(vec![CleanedTrace {
                trace_id_hash: 1,
                success: true,
                bytes_freed: 10,
            }])
        }

        async fn invalidate_cache(&self, _program_id: &str) -> Result<(), UninstallError> {
            self.record("invalidate");
            Ok(())
        }
    }

    async fn planned_job(port: &FakePort) -> UninstallJob {
        plan_uninstall(port, "demo-id")
            .await
            .expect("fake plan should succeed")
    }

    #[tokio::test]
    async fn execute_uses_strict_resolve_snapshot_precheck_run_verify_scan_order() {
        let port = FakePort::new();
        let mut job = planned_job(&port).await;
        execute_uninstall(&port, &mut job, 1)
            .await
            .expect("fake execute should succeed");
        assert_eq!(
            *port
                .calls
                .lock()
                .expect("test mutex should not be poisoned"),
            vec!["resolve", "snapshot", "resolve", "precheck", "snapshot", "run", "verify", "scan"]
        );
    }

    #[tokio::test]
    async fn scan_failure_does_not_call_cleanup() {
        let mut port = FakePort::new();
        port.fail_scan = true;
        let mut job = planned_job(&port).await;
        assert!(execute_uninstall(&port, &mut job, 1).await.is_err());
        assert!(!port
            .calls
            .lock()
            .expect("test mutex should not be poisoned")
            .contains(&"clean"));
    }

    #[tokio::test]
    async fn unknown_trace_id_is_rejected_without_touching_clean_port() {
        let port = FakePort::new();
        let mut job = planned_job(&port).await;
        execute_uninstall(&port, &mut job, 1)
            .await
            .expect("fake execute should succeed");
        let error = clean_uninstall_residues(
            &port,
            &mut job,
            CleanupSelection {
                trace_ids: vec!["not-in-plan".to_string()],
                confirm: true,
            },
        )
        .await
        .expect_err("unknown trace must be rejected");
        assert_eq!(error.code, UninstallErrorCode::TraceNotInPlan);
        assert!(!port
            .calls
            .lock()
            .expect("test mutex should not be poisoned")
            .contains(&"clean"));
    }
}
