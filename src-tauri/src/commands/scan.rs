use rust_yu_lib::scanner;
use rust_yu_lib::scanner::models::Trace;
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct ScanOptions {
    pub program_name: String,
    pub trace_types: Option<Vec<String>>,
}

#[tauri::command]
pub async fn scan_traces(program_name: String, trace_types: Option<Vec<String>>) -> Result<Vec<Trace>, String> {
    use rust_yu_lib::scanner::models::TraceType;

    let types = trace_types.map(|t| {
        t.iter()
            .filter_map(|s| match s.to_lowercase().as_str() {
                "registry_key" => Some(TraceType::RegistryKey),
                "registry_value" => Some(TraceType::RegistryValue),
                "file" => Some(TraceType::File),
                "appdata" => Some(TraceType::AppData),
                "shortcut" => Some(TraceType::Shortcut),
                _ => None,
            })
            .collect()
    });

    let traces = scanner::scan_all_traces(&program_name, types)
        .await
        .map_err(|e| e.to_string())?;

    Ok(traces)
}
