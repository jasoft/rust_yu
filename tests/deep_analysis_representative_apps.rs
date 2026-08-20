use rust_yu_lib::lister;
use rust_yu_lib::lister::models::{InstallSource, InstalledProgram};
use rust_yu_lib::scanner;
use rust_yu_lib::scanner::models::{confidence_for_score, ScanAreaStatus, TraceType};
use std::path::Path;
use walkdir::WalkDir;
use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
use winreg::RegKey;

#[tokio::test]
#[ignore = "read-only audit of apps installed on the current Windows system"]
async fn representative_installed_apps_have_internally_consistent_analysis() {
    let mut programs = Vec::new();
    for source in [
        InstallSource::Registry,
        InstallSource::Msi,
        InstallSource::Store,
    ] {
        programs.extend(
            lister::list_all_programs(Some(source), None).unwrap_or_else(|error| {
                panic!("list {source} apps for representative audit: {error}")
            }),
        );
    }
    let selected = select_representatives(&programs);
    assert!(
        selected.len() >= 3,
        "representative audit needs at least three installed apps; selected {:?}",
        selected
            .iter()
            .map(|program| &program.name)
            .collect::<Vec<_>>()
    );

    for program in selected {
        let report = scanner::scan_all_traces_report_for_program(program, None)
            .await
            .unwrap_or_else(|error| panic!("scan {}: {error}", program.name));
        let failed = report
            .coverage
            .iter()
            .filter(|entry| entry.status == ScanAreaStatus::Failed)
            .collect::<Vec<_>>();
        assert!(
            failed.is_empty(),
            "{} scan areas failed: {failed:?}",
            program.name
        );

        for trace in &report.traces {
            assert!(trace.confidence_score <= 100);
            assert_eq!(
                trace.confidence,
                confidence_for_score(trace.confidence_score)
            );
        }

        if let Some(root) = program
            .install_location
            .as_deref()
            .map(Path::new)
            .filter(|path| path.is_dir())
        {
            let readable_file_exists = WalkDir::new(root)
                .min_depth(1)
                .max_depth(8)
                .follow_links(false)
                .into_iter()
                .filter_map(Result::ok)
                .any(|entry| entry.file_type().is_file());
            if readable_file_exists {
                let normalized_root = normalize(root.to_string_lossy().as_ref());
                assert!(
                    report.traces.iter().any(|trace| {
                        trace.trace_type == TraceType::File
                            && normalize(&trace.path).starts_with(&format!("{normalized_root}\\"))
                    }),
                    "{} has readable install files but analysis returned none below {}",
                    program.name,
                    root.display()
                );
            }
        }

        if let Some(key_path) = program.uninstall_registry_key_path.as_deref() {
            if let Some(expected_value_count) = named_registry_value_count(key_path) {
                let normalized_key = format!("{}\\", normalize(key_path));
                let actual_value_count = report
                    .traces
                    .iter()
                    .filter(|trace| {
                        trace.trace_type == TraceType::RegistryValue
                            && normalize(&trace.path).starts_with(&normalized_key)
                    })
                    .count();
                assert_eq!(
                    actual_value_count, expected_value_count,
                    "{} did not surface every named Uninstall value",
                    program.name
                );
            }
        }

        println!(
            "representative={} source={} traces={} protected={} coverage={:?}",
            program.name,
            program.install_source,
            report.traces.len(),
            report
                .traces
                .iter()
                .filter(|trace| trace.is_critical)
                .count(),
            report.coverage
        );
    }
}

fn select_representatives(programs: &[InstalledProgram]) -> Vec<&InstalledProgram> {
    let requested = std::env::var("RUST_YU_REPRESENTATIVE_APPS").unwrap_or_default();
    if !requested.trim().is_empty() {
        let mut selected = Vec::new();
        for name in requested
            .split(',')
            .map(str::trim)
            .filter(|name| !name.is_empty())
        {
            let program = programs
                .iter()
                .find(|program| program.name.eq_ignore_ascii_case(name))
                .unwrap_or_else(|| panic!("requested representative app is not installed: {name}"));
            selected.push(program);
        }
        return selected;
    }

    let mut selected = Vec::new();
    let mut sources = Vec::new();
    for preferred_name in [
        "ChatGPT",
        "RustYu Deep Scan User",
        "RustYu Deep Scan Sparse",
    ] {
        if let Some(program) = programs
            .iter()
            .find(|program| program.name.eq_ignore_ascii_case(preferred_name))
        {
            if !sources.contains(&program.install_source) {
                sources.push(program.install_source);
            }
            selected.push(program);
        }
    }
    for source in [
        InstallSource::Msi,
        InstallSource::Registry,
        InstallSource::Store,
    ] {
        if sources.contains(&source) {
            continue;
        }
        if let Some(program) = programs.iter().find(|program| {
            program.install_source == source
                && !program.name.starts_with("RustYu Deep Scan")
                && program
                    .install_location
                    .as_deref()
                    .is_some_and(|path| Path::new(path).is_dir())
        }) {
            sources.push(source);
            selected.push(program);
        }
    }
    selected.truncate(5);
    selected
}

fn named_registry_value_count(full_path: &str) -> Option<usize> {
    let normalized = full_path.replace('/', "\\");
    let (root, subpath) = if let Some(path) = normalized.strip_prefix("HKLM\\") {
        (RegKey::predef(HKEY_LOCAL_MACHINE), path)
    } else if let Some(path) = normalized.strip_prefix("HKCU\\") {
        (RegKey::predef(HKEY_CURRENT_USER), path)
    } else {
        return None;
    };
    let key = root.open_subkey(subpath).ok()?;
    Some(
        key.enum_values()
            .filter_map(Result::ok)
            .filter(|(name, _)| !name.is_empty() && !name.contains('\\'))
            .count(),
    )
}

fn normalize(value: &str) -> String {
    value
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_ascii_lowercase()
}
