#[cfg(test)]
mod tests {
    use super::{
        build_unsuccessful_uninstall_message, classify_uninstall_exit_code,
        is_likely_interactive_uninstall,
    };

    #[test]
    fn detects_interactive_exe_uninstall_without_silent_flags() {
        assert!(is_likely_interactive_uninstall(
            r#""C:\Program Files\App\uninstall.exe""#
        ));
    }

    #[test]
    fn detects_cancelled_msi_exit_code() {
        let result = classify_uninstall_exit_code("msiexec /x {GUID}", Some(1602));

        assert_eq!(result.user_cancelled, true);
        assert_eq!(result.successful, false);
    }

    #[test]
    fn builds_cancelled_message_when_program_still_present() {
        let message = build_unsuccessful_uninstall_message(
            "Demo App",
            true,
            true,
            Some(1602),
            true,
            false,
        );

        assert!(message.contains("卸载已取消"));
        assert!(message.contains("程序仍保留"));
    }
}
