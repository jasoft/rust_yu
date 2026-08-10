//! F-17 专业证据导出包。导出只读取已持久化报告，先在内存生成不可变快照，
//! 再写入 ZIP；不会上传数据，也不会重新扫描当前系统来改写历史结果。

use super::history::{exports_dir, load_report};
use super::models::UninstallerReport;
use crate::modules::common::error::UninstallerError;
use crate::modules::scanner::models::TraceType;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Write;
use uuid::Uuid;
use zip::write::SimpleFileOptions;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceBundleFile {
    pub name: String,
    pub source: String,
    pub result: String,
    pub sha256: String,
    pub bytes: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceBundleManifest {
    pub schema_version: u32,
    pub report_id: String,
    pub generated_at: DateTime<Utc>,
    pub local_only: bool,
    pub immutable_snapshot_sha256: String,
    pub files: Vec<EvidenceBundleFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceBundleExport {
    pub report_id: String,
    pub path: String,
    pub generated_at: DateTime<Utc>,
    pub file_count: usize,
    pub immutable_snapshot_sha256: String,
}

pub fn export_evidence_bundle(report_id: &str) -> Result<EvidenceBundleExport, UninstallerError> {
    let report = load_report(report_id)?;
    let snapshot = serde_json::to_vec_pretty(&report)
        .map_err(|error| UninstallerError::Serde(error.to_string()))?;
    let snapshot_hash = sha256(&snapshot);
    let generated_at = Utc::now();
    let mut contents = BTreeMap::<String, (String, String, Vec<u8>)>::new();
    contents.insert(
        "report.json".to_string(),
        (
            "persisted_uninstall_report".to_string(),
            "historical_snapshot".to_string(),
            snapshot,
        ),
    );
    contents.insert(
        "traces.csv".to_string(),
        (
            "report.traces_found".to_string(),
            "observed_candidates".to_string(),
            traces_csv(&report).into_bytes(),
        ),
    );
    contents.insert(
        "files.csv".to_string(),
        (
            "report.traces_found+traces_removed".to_string(),
            "per_target_cleanup_result".to_string(),
            files_csv(&report).into_bytes(),
        ),
    );
    contents.insert(
        "registry.reg".to_string(),
        (
            "report.registry_traces".to_string(),
            "evidence_comments_only_not_importable_changes".to_string(),
            registry_reg(&report).into_bytes(),
        ),
    );
    contents.insert(
        "process-tree.csv".to_string(),
        (
            "report.job.events".to_string(),
            "timeline_without_unobserved_pid_inference".to_string(),
            process_tree_csv(&report).into_bytes(),
        ),
    );
    contents.insert(
        "services-tasks.csv".to_string(),
        (
            "report.system_integration_traces".to_string(),
            "observed_and_cleanup_result".to_string(),
            services_tasks_csv(&report).into_bytes(),
        ),
    );

    let mut files = contents
        .iter()
        .map(|(name, (source, result, bytes))| EvidenceBundleFile {
            name: name.clone(),
            source: source.clone(),
            result: result.clone(),
            sha256: sha256(bytes),
            bytes: bytes.len(),
        })
        .collect::<Vec<_>>();
    files.sort_by(|left, right| left.name.cmp(&right.name));
    let manifest = EvidenceBundleManifest {
        schema_version: 1,
        report_id: report.id.clone(),
        generated_at,
        local_only: true,
        immutable_snapshot_sha256: snapshot_hash.clone(),
        files: files.clone(),
    };
    let manifest_bytes = serde_json::to_vec_pretty(&manifest)
        .map_err(|error| UninstallerError::Serde(error.to_string()))?;
    let mut sums = files
        .iter()
        .map(|file| {
            format!(
                "{}  {}  source={}  result={}",
                file.sha256, file.name, file.source, file.result
            )
        })
        .collect::<Vec<_>>();
    sums.push(format!(
        "{}  manifest.json  source=bundle_builder  result=file_provenance",
        sha256(&manifest_bytes)
    ));
    contents.insert(
        "manifest.json".to_string(),
        (
            "bundle_builder".to_string(),
            "file_provenance".to_string(),
            manifest_bytes,
        ),
    );
    contents.insert(
        "SHA256SUMS.txt".to_string(),
        (
            "bundle_builder".to_string(),
            "integrity_summary".to_string(),
            format!("{}\n", sums.join("\n")).into_bytes(),
        ),
    );

    let root = exports_dir()?;
    fs::create_dir_all(&root)?;
    let path = root.join(format!(
        "rust-yu-evidence-{}-{}.zip",
        report.id,
        Uuid::new_v4()
    ));
    let file = File::create(&path)?;
    let mut zip = zip::ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for (name, (_, _, bytes)) in &contents {
        zip.start_file(name, options)
            .map_err(|error| UninstallerError::Other(format!("创建证据包文件失败: {error}")))?;
        zip.write_all(bytes)?;
    }
    zip.finish()
        .map_err(|error| UninstallerError::Other(format!("完成证据包失败: {error}")))?;
    Ok(EvidenceBundleExport {
        report_id: report.id,
        path: path.to_string_lossy().to_string(),
        generated_at,
        file_count: contents.len(),
        immutable_snapshot_sha256: snapshot_hash,
    })
}

fn traces_csv(report: &UninstallerReport) -> String {
    let mut output =
        String::from("source,result,trace_id,type,path,exists,confidence,critical,description\n");
    for trace in &report.traces_found {
        output.push_str(&csv_row(&[
            "report.traces_found".to_string(),
            "observed".to_string(),
            trace.id.clone(),
            trace.trace_type.to_string(),
            trace.path.clone(),
            trace.exists.to_string(),
            format!("{:?}", trace.confidence).to_ascii_lowercase(),
            trace.is_critical.to_string(),
            trace.description.clone(),
        ]));
    }
    output
}

fn files_csv(report: &UninstallerReport) -> String {
    let results = report
        .traces_removed
        .iter()
        .map(|item| (item.trace_id.as_str(), item))
        .collect::<BTreeMap<_, _>>();
    let mut output = String::from("source,result,trace_id,path,bytes,cleanup_error,backup_id\n");
    for trace in report.traces_found.iter().filter(|trace| {
        matches!(
            trace.trace_type,
            TraceType::File | TraceType::AppData | TraceType::Shortcut
        )
    }) {
        let result = results.get(trace.id.as_str());
        output.push_str(&csv_row(&[
            "report.file_trace".to_string(),
            result
                .map(|item| if item.success { "removed" } else { "failed" })
                .unwrap_or("retained")
                .to_string(),
            trace.id.clone(),
            trace.path.clone(),
            trace.size.unwrap_or_default().to_string(),
            result
                .and_then(|item| item.error.clone())
                .unwrap_or_default(),
            result
                .and_then(|item| item.backup_id.clone())
                .unwrap_or_default(),
        ]));
    }
    output
}

fn registry_reg(report: &UninstallerReport) -> String {
    let mut output = String::from("Windows Registry Editor Version 5.00\r\n\r\n; Rust Yu evidence only. No values or deletion directives are included.\r\n");
    for trace in report.traces_found.iter().filter(|trace| {
        matches!(
            trace.trace_type,
            TraceType::RegistryKey | TraceType::RegistryValue
        )
    }) {
        output.push_str(&format!(
            "; source=report.registry_trace result=observed path={}\r\n",
            trace.path.replace(['\r', '\n'], " ")
        ));
    }
    output
}

fn process_tree_csv(report: &UninstallerReport) -> String {
    let mut output = String::from("source,result,sequence,phase,event\n");
    if let Some(job) = &report.job {
        for event in &job.events {
            output.push_str(&csv_row(&[
                "report.job.events".to_string(),
                "timeline_event_no_pid_claim".to_string(),
                event.sequence.to_string(),
                format!("{:?}", event.phase).to_ascii_lowercase(),
                format!("{:?}", event.payload),
            ]));
        }
    }
    output
}

fn services_tasks_csv(report: &UninstallerReport) -> String {
    let results = report
        .traces_removed
        .iter()
        .map(|item| (item.trace_id.as_str(), item))
        .collect::<BTreeMap<_, _>>();
    let mut output = String::from("source,result,trace_id,type,path,related_path,confidence\n");
    for trace in report.traces_found.iter().filter(|trace| {
        matches!(
            trace.trace_type,
            TraceType::Service | TraceType::ScheduledTask | TraceType::Driver
        )
    }) {
        let result = results.get(trace.id.as_str());
        output.push_str(&csv_row(&[
            "report.system_integration_trace".to_string(),
            result
                .map(|item| if item.success { "removed" } else { "failed" })
                .unwrap_or("retained")
                .to_string(),
            trace.id.clone(),
            trace.trace_type.to_string(),
            trace.path.clone(),
            trace.related_path.clone().unwrap_or_default(),
            format!("{:?}", trace.confidence).to_ascii_lowercase(),
        ]));
    }
    output
}

fn csv_row(values: &[String]) -> String {
    let escaped = values
        .iter()
        .map(|value| format!("\"{}\"", value.replace('"', "\"\"")))
        .collect::<Vec<_>>();
    format!("{}\n", escaped.join(","))
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::reporter::models::UninstallerReport;
    use crate::modules::scanner::models::{Confidence, Trace};

    #[test]
    fn registry_export_is_evidence_only() {
        let report = UninstallerReport::new("Demo".to_string()).with_traces(vec![Trace::new(
            "Demo".to_string(),
            TraceType::RegistryKey,
            r"HKCU\Software\Demo".to_string(),
        )
        .with_confidence(Confidence::High)]);
        let output = registry_reg(&report);
        assert!(output.contains("evidence only"));
        assert!(!output.contains("[-HKCU"));
    }

    #[test]
    fn csv_escapes_quotes_and_newlines_as_fields() {
        let row = csv_row(&["a\"b".to_string(), "result".to_string()]);
        assert_eq!(row, "\"a\"\"b\",\"result\"\n");
    }
}
