use rust_yu_lib::lister::models::InstalledProgram;
use rust_yu_lib::scanner;
use rust_yu_lib::scanner::models::Trace;
use serde::{Deserialize, Serialize};

use super::CommandError;

#[derive(Debug, Serialize, Deserialize)]
pub struct ScanOptions {
    pub program_name: String,
    pub trace_types: Option<Vec<String>>,
}

#[tauri::command]
pub async fn scan_traces(
    program_name: String,
    trace_types: Option<Vec<String>>,
    program: Option<InstalledProgram>,
) -> Result<Vec<Trace>, CommandError> {
    use rust_yu_lib::scanner::models::TraceType;

    let types = trace_types.map(|t| {
        t.iter()
            .filter_map(|s| match s.to_lowercase().as_str() {
                "registry_key" => Some(TraceType::RegistryKey),
                "registry_value" => Some(TraceType::RegistryValue),
                "file" => Some(TraceType::File),
                "appdata" => Some(TraceType::AppData),
                "shortcut" => Some(TraceType::Shortcut),
                "scheduled_task" => Some(TraceType::ScheduledTask),
                "service" => Some(TraceType::Service),
                "driver" => Some(TraceType::Driver),
                _ => None,
            })
            .collect()
    });

    let traces = if let Some(program) = program.as_ref() {
        scanner::scan_all_traces_for_program(program, types).await
    } else {
        scanner::scan_all_traces(&program_name, types).await
    }
    .map_err(CommandError::from)?;

    Ok(traces)
}
