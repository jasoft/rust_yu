use crate::application::uninstall::UninstallJob;
use crate::modules::cleaner::models::CleanResult;
use crate::modules::scanner::models::Trace;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 卸载报告
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UninstallerReport {
    pub id: String,
    pub program_name: String,
    pub generated_at: DateTime<Utc>,
    pub traces_found: Vec<Trace>,
    pub traces_removed: Vec<CleanResult>,
    pub total_size_freed: u64,
    pub success: bool,
    pub warnings: Vec<String>,
    /// 保留完整卸载任务快照，历史报告不依赖当前进程内的 coordinator。
    #[serde(default)]
    pub job: Option<UninstallJob>,
}

#[allow(dead_code)]
impl UninstallerReport {
    pub fn new(program_name: String) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            program_name,
            generated_at: Utc::now(),
            traces_found: Vec::new(),
            traces_removed: Vec::new(),
            total_size_freed: 0,
            success: true,
            warnings: Vec::new(),
            job: None,
        }
    }

    pub fn from_job(job: &UninstallJob) -> Self {
        let outcome = job.outcome.as_ref();
        let warnings = job
            .events
            .iter()
            .filter_map(|event| match &event.payload {
                crate::application::uninstall::UninstallEventPayload::Finished {
                    success: false,
                    message,
                } => Some(message.clone()),
                _ => None,
            })
            .collect();
        Self {
            id: job.snapshot.job_id.0.clone(),
            program_name: job.snapshot.program.name.clone(),
            generated_at: Utc::now(),
            traces_found: job.snapshot.traces.clone(),
            traces_removed: job.cleanup_results.clone(),
            total_size_freed: outcome.map_or(0, |value| value.bytes_freed),
            success: outcome.is_some_and(|value| value.success)
                && job.phase == crate::application::uninstall::UninstallPhase::Completed,
            warnings,
            job: Some(job.clone()),
        }
    }

    pub fn with_traces(mut self, traces: Vec<Trace>) -> Self {
        self.traces_found = traces;
        self
    }

    pub fn with_results(mut self, results: Vec<CleanResult>) -> Self {
        self.total_size_freed = results.iter().map(|r| r.bytes_freed).sum();
        self.success = results.iter().all(|r| r.success);
        self.traces_removed = results;
        self
    }

    pub fn add_warning(&mut self, warning: String) {
        self.warnings.push(warning);
    }
}

#[cfg(test)]
mod tests {
    use super::UninstallerReport;
    use crate::modules::cleaner::models::CleanResult;

    #[test]
    fn with_results_updates_size_and_success_state() {
        let report = UninstallerReport::new("Demo App".to_string()).with_results(vec![
            CleanResult {
                trace_id: "1".to_string(),
                path: r"C:\Demo App".to_string(),
                success: true,
                error: None,
                bytes_freed: 512,
                backup_id: None,
            },
            CleanResult {
                trace_id: "2".to_string(),
                path: r"C:\Demo App\bad".to_string(),
                success: false,
                error: Some("denied".to_string()),
                bytes_freed: 0,
                backup_id: None,
            },
        ]);

        assert_eq!(report.total_size_freed, 512);
        assert!(!report.success);
        assert_eq!(report.traces_removed.len(), 2);
    }

    #[test]
    fn add_warning_appends_entries() {
        let mut report = UninstallerReport::new("Demo App".to_string());

        report.add_warning("manual review".to_string());

        assert_eq!(report.warnings, vec!["manual review".to_string()]);
    }
}
