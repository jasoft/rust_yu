use super::error::{ElevationError, ElevationErrorCode};
use std::path::Path;

pub const ELEVATED_ENTRY_ARGUMENT: &str = "--elevated-entry";
pub const TASK_PATH: &str = r"\Rust Yu\ElevatedGui";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskDefinition {
    pub task_path: String,
    pub executable: String,
    pub arguments: String,
    pub principal_user: String,
    pub logon_type: String,
    pub run_level: String,
    pub multiple_instances: String,
    pub allow_demand_start: bool,
    pub disallow_start_if_on_batteries: bool,
    pub stop_if_goes_on_batteries: bool,
    pub execution_time_limit: String,
    pub trigger: String,
    pub security_descriptor: String,
}

impl TaskDefinition {
    pub fn for_executable(
        executable: &Path,
        principal_user: impl Into<String>,
    ) -> Result<Self, ElevationError> {
        if !executable.is_absolute() {
            return Err(ElevationError::new(
                ElevationErrorCode::UnsafeInstallLocation,
                "计划任务动作必须是绝对路径",
            ));
        }
        Ok(Self {
            task_path: TASK_PATH.to_string(),
            executable: executable.to_string_lossy().to_string(),
            arguments: ELEVATED_ENTRY_ARGUMENT.to_string(),
            principal_user: principal_user.into(),
            logon_type: "InteractiveToken".to_string(),
            run_level: "HighestAvailable".to_string(),
            multiple_instances: "IgnoreNew".to_string(),
            allow_demand_start: true,
            disallow_start_if_on_batteries: false,
            stop_if_goes_on_batteries: false,
            execution_time_limit: "PT2H".to_string(),
            trigger: "none".to_string(),
            security_descriptor: "D:(A;;GRGX;;;CU)(A;;GA;;;BA)(A;;GA;;;SY)".to_string(),
        })
    }

    pub fn to_xml(&self) -> String {
        let executable = xml_escape(&self.executable);
        let user = xml_escape(&self.principal_user);
        let sddl = xml_escape(&self.security_descriptor);
        format!(
            r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.4" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo><Description>Rust Yu protected elevated GUI entry</Description><SecurityDescriptor>{sddl}</SecurityDescriptor></RegistrationInfo>
  <Triggers />
  <Principals><Principal id="Author"><UserId>{user}</UserId><LogonType>InteractiveToken</LogonType><RunLevel>HighestAvailable</RunLevel></Principal></Principals>
  <Settings><MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy><DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries><StopIfGoingOnBatteries>false</StopIfGoingOnBatteries><AllowHardTerminate>false</AllowHardTerminate><AllowStartOnDemand>true</AllowStartOnDemand><ExecutionTimeLimit>PT2H</ExecutionTimeLimit><Hidden>true</Hidden></Settings>
  <Actions Context="Author"><Exec><Command>{executable}</Command><Arguments>--elevated-entry</Arguments></Exec></Actions>
</Task>"#
        )
    }
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::{TaskDefinition, ELEVATED_ENTRY_ARGUMENT, TASK_PATH};
    use std::path::Path;

    #[test]
    fn definition_snapshot_contains_fixed_safe_action_and_no_trigger() {
        let definition = TaskDefinition::for_executable(
            Path::new(r"C:\Program Files\Rust Yu\RustYu.exe"),
            "user",
        )
        .expect("绝对路径应能生成任务定义");
        let xml = definition.to_xml();
        assert_eq!(definition.task_path, TASK_PATH);
        assert_eq!(definition.arguments, ELEVATED_ENTRY_ARGUMENT);
        assert!(xml.contains("<Triggers />"));
        assert!(xml.contains("<RunLevel>HighestAvailable</RunLevel>"));
        assert!(xml.contains("<MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>"));
        assert!(!xml.contains("--confirm"));
        assert!(!xml.contains("--clean"));
    }
}
