#![cfg(windows)]

use rust_yu_lib::application::target::build_target_search_query;
use rust_yu_lib::application::uninstall::{
    execute_uninstall, plan_uninstall, ProductionUninstallPort,
};
use rust_yu_lib::modules::lister;
use std::error::Error;
use std::path::Path;
use std::process::Command;

#[tokio::test]
#[ignore = "requires the administrator Windows Inno fixture environment"]
async fn inno_legacy_fixture_uses_application_workflow_and_keeps_residue_review(
) -> Result<(), Box<dyn Error>> {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"));
    let installer = repository.join(".resources/inno-legacy-test/output/RustYuLegacyTestSetup.exe");
    let install_status = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-Command"])
        .arg(format!("Start-Process -FilePath '{}' -ArgumentList '/VERYSILENT','/NORESTART' -Wait -Verb RunAs", installer.display()))
        .status()?;
    if !install_status.success() {
        return Err("Inno fixture installation failed".into());
    }

    let result = async {
        let listed = lister::list_programs_with_cache(build_target_search_query())?;
        let program = listed
            .programs
            .iter()
            .find(|candidate| candidate.name == "RustYu Legacy Test App")
            .ok_or("installed Inno fixture was not listed")?;
        let port = ProductionUninstallPort;
        let mut job = plan_uninstall(&port, &program.id).await?;
        let review = execute_uninstall(&port, &mut job, 180).await?;
        if !review.default_selected_ids.is_empty() {
            return Err("residue review must not auto-select traces".into());
        }
        if !review
            .traces
            .iter()
            .any(|trace| trace.path.ends_with("leftover.log"))
        {
            return Err("install directory residue was not surfaced".into());
        }
        if !review
            .traces
            .iter()
            .any(|trace| trace.path.ends_with("leftover-user-profile.json"))
        {
            return Err("AppData residue was not surfaced".into());
        }
        Ok::<(), Box<dyn Error>>(())
    }
    .await;

    let _ = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-Command"])
        .arg("$u = 'C:\\Program Files\\RustYu Legacy Test App\\unins000.exe'; if (Test-Path $u) { Start-Process $u -ArgumentList '/VERYSILENT','/SUPPRESSMSGBOXES','/NORESTART' -Wait }")
        .status();
    result
}
