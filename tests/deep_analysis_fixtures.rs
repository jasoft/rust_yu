use rust_yu_lib::lister;
use rust_yu_lib::lister::models::InstallSource;
use rust_yu_lib::scanner;
use rust_yu_lib::scanner::models::{ScanAreaStatus, Trace};
use serde::Deserialize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
struct TruthManifest {
    fixtures: Vec<FixtureTruth>,
}

#[derive(Debug, Deserialize)]
struct FixtureTruth {
    id: String,
    display_name: String,
    expected: Vec<ExpectedTrace>,
}

#[derive(Debug, Deserialize)]
struct ExpectedTrace {
    id: String,
    trace_type: String,
    path: String,
    min_score: u8,
    reason: String,
    protected: bool,
}

#[tokio::test]
#[ignore = "requires installed controlled fixtures; performs read-only analysis only"]
async fn installed_fixtures_match_their_trace_truth_manifest() {
    let manifest_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(".resources")
        .join("nsis-random-fixtures")
        .join("expected-traces.json");
    let manifest: TruthManifest = serde_json::from_slice(
        &std::fs::read(&manifest_path)
            .unwrap_or_else(|error| panic!("read {}: {error}", manifest_path.display())),
    )
    .unwrap_or_else(|error| panic!("parse {}: {error}", manifest_path.display()));
    let selected = selected_fixture_ids();

    for fixture in manifest
        .fixtures
        .iter()
        .filter(|fixture| selected.is_empty() || selected.contains(fixture.id.as_str()))
    {
        let programs =
            lister::list_all_programs(Some(InstallSource::Registry), Some(&fixture.display_name))
                .unwrap_or_else(|error| panic!("list {}: {error}", fixture.display_name));
        let program = programs
            .into_iter()
            .find(|program| program.name == fixture.display_name)
            .unwrap_or_else(|| panic!("fixture is not installed: {}", fixture.display_name));

        let report = scanner::scan_all_traces_report_for_program(&program, None)
            .await
            .unwrap_or_else(|error| panic!("scan {}: {error}", fixture.display_name));
        assert!(
            report.coverage.iter().all(|entry| matches!(
                entry.status,
                ScanAreaStatus::Complete | ScanAreaStatus::Partial
            )),
            "{} coverage failed or was skipped: {:?}",
            fixture.display_name,
            report.coverage
        );

        for expected in &fixture.expected {
            let expected_path = expand_truth_path(&expected.path);
            let trace = find_trace(&report.traces, &expected.trace_type, &expected_path)
                .unwrap_or_else(|| {
                    panic!(
                        "{} missing truth {} ({}) at {}",
                        fixture.display_name, expected.id, expected.trace_type, expected_path
                    )
                });
            assert!(
                trace.confidence_score >= expected.min_score,
                "{} score {} < {} for {}",
                expected.id,
                trace.confidence_score,
                expected.min_score,
                trace.path
            );
            assert_eq!(
                serde_json::to_value(trace.confidence_reason)
                    .ok()
                    .and_then(|value| value.as_str().map(ToOwned::to_owned))
                    .as_deref(),
                Some(expected.reason.as_str()),
                "unexpected confidence reason for {}",
                expected.id
            );
            assert_eq!(
                trace.is_critical, expected.protected,
                "unexpected protection state for {}",
                expected.id
            );
        }

        println!(
            "fixture={} traces={} expected={} coverage={}",
            fixture.id,
            report.traces.len(),
            fixture.expected.len(),
            report.coverage.len()
        );
    }
}

fn selected_fixture_ids() -> HashSet<String> {
    std::env::var("RUST_YU_DEEP_SCAN_FIXTURES")
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn find_trace<'a>(traces: &'a [Trace], trace_type: &str, path: &str) -> Option<&'a Trace> {
    let expected_type = trace_type.to_ascii_lowercase();
    let expected_path = normalize(path);
    traces.iter().find(|trace| {
        serde_json::to_value(trace.trace_type)
            .ok()
            .and_then(|value| value.as_str().map(str::to_ascii_lowercase))
            .as_deref()
            == Some(expected_type.as_str())
            && normalize(&trace.path) == expected_path
    })
}

fn expand_truth_path(template: &str) -> String {
    let appdata = env_path("APPDATA");
    let program_data = env_path("ProgramData");
    let public = env_path("PUBLIC");
    let replacements = [
        (
            "%COMMONPROGRAMS%",
            program_data.join(r"Microsoft\Windows\Start Menu\Programs"),
        ),
        ("%COMMONDESKTOP%", public.join("Desktop")),
        ("%PROGRAMFILES%", env_path("ProgramFiles")),
        ("%LOCALAPPDATA%", env_path("LOCALAPPDATA")),
        ("%USERPROFILE%", env_path("USERPROFILE")),
        ("%PROGRAMDATA%", program_data),
        (
            "%PROGRAMS%",
            appdata.join(r"Microsoft\Windows\Start Menu\Programs"),
        ),
        ("%APPDATA%", appdata),
        (
            "%DESKTOP%",
            dirs::desktop_dir().unwrap_or_else(|| env_path("USERPROFILE").join("Desktop")),
        ),
    ];
    replacements
        .iter()
        .fold(template.to_string(), |value, (token, path)| {
            value.replace(token, &path.to_string_lossy())
        })
}

fn env_path(name: &str) -> PathBuf {
    std::env::var_os(name)
        .map(PathBuf::from)
        .unwrap_or_else(|| panic!("missing environment variable: {name}"))
}

fn normalize(value: &str) -> String {
    value
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_ascii_lowercase()
}
