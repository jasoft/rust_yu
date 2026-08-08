use crate::modules::lister::models::InstalledProgram;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct UninstallTargetFingerprint(pub String);

/// 只使用卸载安全边界相关字段，避免名称、版本、图标等展示元数据导致误报。
pub fn fingerprint_program(program: &InstalledProgram) -> UninstallTargetFingerprint {
    let command = program
        .preferred_uninstall_string()
        .map(normalize_command_for_fingerprint)
        .unwrap_or_default();
    let canonical = format!(
        "id={}\nkind={:?}\nregistry={}\ncommand={}\nlocation={}",
        program.id,
        program.uninstall_kind,
        program
            .uninstall_registry_key_path
            .as_deref()
            .unwrap_or_default(),
        command,
        program.install_location.as_deref().unwrap_or_default(),
    );

    UninstallTargetFingerprint(format!("fnv1a64:{:016x}", fnv1a64(canonical.as_bytes())))
}

fn normalize_command_for_fingerprint(command: &str) -> String {
    command.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn fnv1a64(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    })
}

#[cfg(test)]
mod tests {
    use super::fingerprint_program;
    use crate::modules::lister::models::{InstallSource, InstalledProgram, UninstallKind};

    fn program() -> InstalledProgram {
        let mut program =
            InstalledProgram::new("Display name".to_string(), InstallSource::Registry);
        program.id = "stable-id".to_string();
        program.install_location = Some(r"C:\Program Files\Demo".to_string());
        program.uninstall_registry_key_path =
            Some(r"HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall\stable-id".to_string());
        program.uninstall_string = Some(r#""C:\Program Files\Demo\uninstall.exe" /S"#.to_string());
        program
    }

    #[test]
    fn same_security_snapshot_has_same_fingerprint() {
        assert_eq!(
            fingerprint_program(&program()),
            fingerprint_program(&program())
        );
    }

    #[test]
    fn display_only_metadata_does_not_change_fingerprint() {
        let mut changed = program();
        changed.name = "Translated display name".to_string();
        changed.publisher = Some("Publisher".to_string());
        changed.version = Some("2.0".to_string());
        changed.icon_path = Some(r"C:\icon.ico".to_string());

        assert_eq!(
            fingerprint_program(&program()),
            fingerprint_program(&changed)
        );
    }

    #[test]
    fn security_boundary_changes_change_fingerprint() {
        let original = program();
        let mut changed = original.clone();
        changed.uninstall_kind = UninstallKind::Msi;
        assert_ne!(
            fingerprint_program(&original),
            fingerprint_program(&changed)
        );

        changed = original.clone();
        changed.install_location = Some(r"C:\Other".to_string());
        assert_ne!(
            fingerprint_program(&original),
            fingerprint_program(&changed)
        );
    }
}
