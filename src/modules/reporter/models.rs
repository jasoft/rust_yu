use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use crate::modules::cleaner::models::CleanResult;
use crate::modules::scanner::models::Trace;

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
}

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
