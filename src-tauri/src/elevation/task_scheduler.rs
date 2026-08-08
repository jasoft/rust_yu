use super::error::{ElevationError, ElevationErrorCode};
use super::task_definition::{TaskDefinition, TASK_PATH};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskInspection {
    pub task_path: String,
    pub xml: String,
}

#[cfg(windows)]
struct ComApartment {
    initialized: bool,
}

#[cfg(windows)]
impl ComApartment {
    fn initialize() -> Result<Self, ElevationError> {
        use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};
        let result = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        if result.is_ok() {
            return Ok(Self { initialized: true });
        }
        if result.0 == windows::core::HRESULT(0x80010106u32 as i32).0 {
            return Ok(Self { initialized: false });
        }
        Err(ElevationError::new(
            ElevationErrorCode::ComInitializationFailed,
            format!("COM 初始化失败: {result:?}"),
        ))
    }
}

#[cfg(windows)]
impl Drop for ComApartment {
    fn drop(&mut self) {
        if self.initialized {
            unsafe {
                windows::Win32::System::Com::CoUninitialize();
            }
        }
    }
}

#[cfg(windows)]
fn with_service<T>(
    operation: impl FnOnce(
        &windows::Win32::System::TaskScheduler::ITaskService,
    ) -> Result<T, ElevationError>,
) -> Result<T, ElevationError> {
    use windows::Win32::System::Com::{CoCreateInstance, CLSCTX_INPROC_SERVER};
    use windows::Win32::System::TaskScheduler::{ITaskService, TaskScheduler};
    let _com = ComApartment::initialize()?;
    let service: ITaskService =
        unsafe { CoCreateInstance(&TaskScheduler, None, CLSCTX_INPROC_SERVER) }.map_err(
            |error| {
                ElevationError::new(
                    ElevationErrorCode::ElevationTaskAccessDenied,
                    error.to_string(),
                )
            },
        )?;
    let empty = windows::Win32::System::Variant::VARIANT::default();
    unsafe { service.Connect(&empty, &empty, &empty, &empty) }.map_err(|error| {
        ElevationError::new(
            ElevationErrorCode::ElevationTaskAccessDenied,
            error.to_string(),
        )
    })?;
    operation(&service)
}

#[cfg(windows)]
fn task_not_found(error: &windows::core::Error) -> bool {
    let value = error.code().0 as u32;
    value == 0x80070002 || value == 0x8004130f
}

#[cfg(windows)]
pub fn inspect_current_user_task() -> Result<Option<TaskInspection>, ElevationError> {
    use windows::core::BSTR;
    with_service(|service| unsafe {
        let folder = match service.GetFolder(&BSTR::from(r"\Rust Yu")) {
            Ok(folder) => folder,
            Err(error) if task_not_found(&error) => return Ok(None),
            Err(error) => {
                return Err(ElevationError::new(
                    ElevationErrorCode::ElevationTaskAccessDenied,
                    error.to_string(),
                ))
            }
        };
        let task = match folder.GetTask(&BSTR::from("ElevatedGui")) {
            Ok(task) => task,
            Err(error) if task_not_found(&error) => return Ok(None),
            Err(error) => {
                return Err(ElevationError::new(
                    ElevationErrorCode::ElevationTaskAccessDenied,
                    error.to_string(),
                ))
            }
        };
        let xml = task.Xml().map_err(|error| {
            ElevationError::new(
                ElevationErrorCode::ElevationTaskAccessDenied,
                error.to_string(),
            )
        })?;
        Ok(Some(TaskInspection {
            task_path: TASK_PATH.to_string(),
            xml: xml.to_string(),
        }))
    })
}

#[cfg(not(windows))]
pub fn inspect_current_user_task() -> Result<Option<TaskInspection>, ElevationError> {
    Ok(None)
}

pub fn validate_current_user_task(executable: &Path) -> Result<TaskInspection, ElevationError> {
    let expected = TaskDefinition::for_executable(executable, current_user_name())?;
    let inspection = inspect_current_user_task()?.ok_or_else(|| {
        ElevationError::new(
            ElevationErrorCode::ElevationTaskMissing,
            "管理员启动任务不存在",
        )
    })?;
    if !inspection
        .xml
        .contains(&xml_escape_for_match(&expected.executable))
        || !inspection.xml.contains("--elevated-entry")
        || !inspection.xml.contains("HighestAvailable")
        || !inspection.xml.contains("InteractiveToken")
        || !inspection
            .xml
            .contains("D:(A;;GRGX;;;CU)(A;;GA;;;BA)(A;;GA;;;SY)")
    {
        return Err(ElevationError::new(
            ElevationErrorCode::ElevationTaskInvalid,
            "管理员启动任务定义与 Rust Yu 固定定义不匹配",
        ));
    }
    Ok(inspection)
}

pub fn create_or_repair_current_user_task(
    executable: &Path,
) -> Result<TaskInspection, ElevationError> {
    let definition = TaskDefinition::for_executable(executable, current_user_name())?;
    #[cfg(windows)]
    {
        use windows::core::BSTR;
        use windows::Win32::System::TaskScheduler::{
            TASK_CREATE_OR_UPDATE, TASK_LOGON_INTERACTIVE_TOKEN,
        };
        return with_service(|service| unsafe {
            let root = service.GetFolder(&BSTR::from("\\")).map_err(|error| {
                ElevationError::new(
                    ElevationErrorCode::ElevationTaskAccessDenied,
                    error.to_string(),
                )
            })?;
            let folder = match service.GetFolder(&BSTR::from(r"\Rust Yu")) {
                Ok(folder) => folder,
                Err(_) => root
                    .CreateFolder(
                        &BSTR::from("Rust Yu"),
                        &windows::Win32::System::Variant::VARIANT::default(),
                    )
                    .map_err(|error| {
                        ElevationError::new(
                            ElevationErrorCode::ElevationTaskAccessDenied,
                            error.to_string(),
                        )
                    })?,
            };
            let task_definition = service.NewTask(0).map_err(|error| {
                ElevationError::new(
                    ElevationErrorCode::ElevationTaskAccessDenied,
                    error.to_string(),
                )
            })?;
            task_definition
                .SetXmlText(&BSTR::from(definition.to_xml()))
                .map_err(|error| {
                    ElevationError::new(ElevationErrorCode::ElevationTaskInvalid, error.to_string())
                })?;
            let empty = windows::Win32::System::Variant::VARIANT::default();
            folder
                .RegisterTaskDefinition(
                    &BSTR::from("ElevatedGui"),
                    &task_definition,
                    TASK_CREATE_OR_UPDATE.0,
                    &empty,
                    &empty,
                    TASK_LOGON_INTERACTIVE_TOKEN,
                    &empty,
                )
                .map_err(|error| {
                    ElevationError::new(
                        ElevationErrorCode::ElevationTaskAccessDenied,
                        error.to_string(),
                    )
                })?;
            Ok(TaskInspection {
                task_path: TASK_PATH.to_string(),
                xml: definition.to_xml(),
            })
        });
    }
    #[cfg(not(windows))]
    {
        Ok(TaskInspection {
            task_path: TASK_PATH.to_string(),
            xml: definition.to_xml(),
        })
    }
}

pub fn run_current_user_task() -> Result<(), ElevationError> {
    #[cfg(windows)]
    {
        use windows::core::BSTR;
        return with_service(|service| unsafe {
            let folder = service
                .GetFolder(&BSTR::from(r"\Rust Yu"))
                .map_err(|error| {
                    ElevationError::new(ElevationErrorCode::ElevationTaskMissing, error.to_string())
                })?;
            let task = folder
                .GetTask(&BSTR::from("ElevatedGui"))
                .map_err(|error| {
                    ElevationError::new(ElevationErrorCode::ElevationTaskMissing, error.to_string())
                })?;
            task.Run(&windows::Win32::System::Variant::VARIANT::default())
                .map(|_| ())
                .map_err(|error| {
                    ElevationError::new(
                        ElevationErrorCode::ElevationLaunchFailed,
                        error.to_string(),
                    )
                })
        });
    }
    #[cfg(not(windows))]
    {
        Ok(())
    }
}

pub fn remove_current_user_task() -> Result<(), ElevationError> {
    #[cfg(windows)]
    {
        use windows::core::BSTR;
        return with_service(|service| unsafe {
            let folder = match service.GetFolder(&BSTR::from(r"\Rust Yu")) {
                Ok(folder) => folder,
                Err(_) => return Ok(()),
            };
            match folder.DeleteTask(&BSTR::from("ElevatedGui"), 0) {
                Ok(()) => Ok(()),
                Err(error) if task_not_found(&error) => Ok(()),
                Err(error) => Err(ElevationError::new(
                    ElevationErrorCode::ElevationTaskAccessDenied,
                    error.to_string(),
                )),
            }
        });
    }
    #[cfg(not(windows))]
    {
        Ok(())
    }
}

pub fn remove_all_product_tasks() -> Result<(), ElevationError> {
    remove_current_user_task()
}

fn current_user_name() -> String {
    std::env::var("USERNAME").unwrap_or_else(|_| "CURRENT_USER".to_string())
}

fn xml_escape_for_match(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::validate_current_user_task;
    use std::path::Path;

    #[test]
    fn validation_missing_task_has_stable_code() {
        let error = validate_current_user_task(Path::new(r"C:\Program Files\Rust Yu\RustYu.exe"))
            .expect_err("non-Windows test backend has no task");
        assert_eq!(error.code.as_str(), "elevation_task_missing");
    }
}
