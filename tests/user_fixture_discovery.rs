//! 只读核验用户级夹具的真实扫描覆盖；安装、权限门禁与清场由外部夹具驱动。
#![cfg(windows)]

use rust_yu_lib::modules::lister::models::InstallSource;
use rust_yu_lib::modules::lister::models::InstalledProgram;
use rust_yu_lib::modules::scanner::scope::ScanIdentity;
use rust_yu_lib::modules::{lister, scanner};
use std::path::PathBuf;

fn correlated_program(name: &str, identifier: &str) -> InstalledProgram {
    let mut program = InstalledProgram::new(name.to_string(), InstallSource::Registry);
    program.install_location = Some(format!(r"C:\FixtureApps\{identifier}"));
    program.uninstall_registry_key_path = Some(format!(
        r"HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\{identifier}"
    ));
    program.icon_path = Some(format!(r"C:\FixtureApps\{identifier}\{identifier}.exe"));
    program
}

#[test]
fn correlated_private_identifier_matches_exactly_without_live_install_directory() {
    let program = correlated_program("Acme Studio Editor Extended", "AcmeStudioEditor");
    assert!(!ScanIdentity::from_name(&program.name).matches_component("AcmeStudioEditor"));
    let identity = ScanIdentity::from_program(&program);
    assert!(identity.matches_component("AcmeStudioEditor"));
    assert!(!identity.matches_component("AcmeStudioEditorOther"));
    assert!(!identity.matches_component("StudioEditor"));
}

#[test]
fn private_identifier_requires_all_correlations_and_rejects_generic_or_shared_paths() {
    let original = correlated_program("Acme Studio Editor Extended", "AcmeStudioEditor");
    for change in [
        "arp",
        "icon",
        "missing-icon",
        "unrelated-name",
        "relative",
        "parent",
        "shared",
    ] {
        let mut program = original.clone();
        match change {
            "arp" => {
                program.uninstall_registry_key_path = Some(r"HKCU\Software\OtherEditor".to_string())
            }
            "icon" => {
                program.icon_path =
                    Some(r"C:\FixtureApps\AcmeStudioEditor\OtherEditor.exe".to_string())
            }
            "missing-icon" => program.icon_path = None,
            "unrelated-name" => program.name = "Unrelated Editor Extended".to_string(),
            "relative" => {
                program.install_location = Some(r"FixtureApps\AcmeStudioEditor".to_string())
            }
            "parent" => {
                program.install_location = Some(r"C:\FixtureApps\..\AcmeStudioEditor".to_string())
            }
            _ => program.install_location = Some(r"C:\Windows\AcmeStudioEditor".to_string()),
        }
        assert!(
            !ScanIdentity::from_program(&program).matches_component("AcmeStudioEditor"),
            "{change}"
        );
    }
    for generic in ["Helper", "Runtime", "Client", "Components", "Programs"] {
        let program = correlated_program(&format!("{generic} Extended"), generic);
        assert!(
            !ScanIdentity::from_program(&program).matches_component(generic),
            "{generic}"
        );
    }
}

fn normalized(path: &str) -> String {
    path.replace('/', "\\")
        .trim_end_matches('\\')
        .to_lowercase()
}

#[tokio::test]
#[ignore = "requires installed leftover-user and leftover-sparse fixtures; read-only discovery only"]
async fn installed_user_fixtures_surface_recorded_files() {
    let output = PathBuf::from(
        std::env::var_os("RUST_YU_USER_FIXTURE_EVIDENCE_DIR").expect("必须指定独立证据目录"),
    );
    std::fs::create_dir_all(&output).expect("创建只读扫描证据目录");
    let local = PathBuf::from(std::env::var_os("LOCALAPPDATA").expect("LOCALAPPDATA"));
    let roaming = PathBuf::from(std::env::var_os("APPDATA").expect("APPDATA"));
    let profiles = [
        (
            "leftover-user",
            "RustYu Leftover User Fixture",
            vec![
                local.join("Programs/RustYuLeftoverUser/RustYuLeftoverUser.exe"),
                local.join("Programs/RustYuLeftoverUser/helper.dll"),
                local.join("Programs/RustYuLeftoverUser/config/app.json"),
                local.join("Programs/RustYuLeftoverUser/logs/session.log"),
                roaming.join("RustYuLeftoverUser/config/profile.json"),
                local.join("RustYuLeftoverUser/Cache/state.db"),
            ],
        ),
        (
            "leftover-sparse",
            "RustYu Shared Helper",
            vec![
                local.join("RustYuSharedTools/Helper/SharedHelper.exe"),
                local.join("RustYuSharedTools/Helper/helper.dat"),
            ],
        ),
    ];
    let mut missing = Vec::new();
    for (id, name, expected) in profiles {
        let candidates = lister::list_all_programs(Some(InstallSource::Registry), Some(name))
            .expect("列出受控夹具")
            .into_iter()
            .filter(|p| p.name == name)
            .collect::<Vec<_>>();
        assert_eq!(candidates.len(), 1, "必须唯一命中受控夹具 {name}");
        let program = &candidates[0];
        for path in &expected {
            assert!(std::fs::symlink_metadata(path)
                .expect("已安装夹具文件可读")
                .is_file());
        }
        let report = scanner::scan_all_traces_report_for_program(program, None)
            .await
            .expect("按真实 InstalledProgram 上下文扫描");
        let path = output.join(format!("{id}-core-report.json"));
        std::fs::write(
            &path,
            serde_json::to_vec_pretty(&report).expect("序列化只读报告"),
        )
        .expect("保留完整扫描覆盖和候选证据");
        for path in expected {
            let key = normalized(&path.to_string_lossy());
            let found = report
                .traces
                .iter()
                .find(|trace| trace.exists && normalized(&trace.path) == key);
            if found.is_none() {
                missing.push(format!("{id}: {}", path.display()));
            } else if path.starts_with(roaming.join("RustYuLeftoverUser"))
                || path.starts_with(local.join("RustYuLeftoverUser"))
            {
                assert!(
                    found.is_some_and(|trace| trace.confidence_score < 80),
                    "新增名称候选必须保持人工确认等级"
                );
            }
        }
        println!(
            "fixture={id} core-traces={} coverage={}",
            report.traces.len(),
            report.coverage.len()
        );
    }
    assert!(missing.is_empty(), "真实夹具文件漏检: {missing:#?}");
}
