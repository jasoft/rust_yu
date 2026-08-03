/** 对齐 Rust 后端 InstalledProgram */
export interface InstalledProgram {
  id: string;
  name: string;
  publisher: string | null;
  version: string | null;
  install_date: string | null;
  install_location: string | null;
  uninstall_string: string | null;
  quiet_uninstall_string: string | null;
  uninstall_registry_key_path: string | null;
  install_source: "registry" | "msi" | "store" | "unknown";
  uninstall_kind: "legacy" | "msi" | "store";
  size: number | null;
  icon_path: string | null;
  icon_cache_path_32: string | null;
  icon_cache_path_48: string | null;
  size_last_updated_at: string | null;
  icon_data_url: string | null;
  icon_data_url_32: string | null;
  icon_data_url_48: string | null;
  estimated_size: number | null;
  display_version: string | null;
  url_info_about: string | null;
  help_link: string | null;
  install_date_source: string;
  install_date_confidence: string;
  icon_source: string;
  icon_confidence: string;
  size_source: string;
  size_confidence: string;
  metadata_confidence: string;
}

export interface ProgramListCacheState {
  cache_hit: boolean;
  cache_valid: boolean;
  refreshed: boolean;
  schema_version: number;
  generated_at: string | null;
  reason: string | null;
}

export interface ProgramListResponse {
  programs: InstalledProgram[];
  cache: ProgramListCacheState;
}

export interface Trace {
  id: string;
  program_name: string;
  trace_type: "registry_key" | "registry_value" | "file" | "appdata" | "shortcut";
  path: string;
  exists: boolean;
  size: number | null;
  confidence: "high" | "medium" | "low";
  description: string | null;
}

export interface CleanResult {
  trace_id: string;
  path: string;
  success: boolean;
  error: string | null;
  bytes_freed: number;
}

export interface UninstallResult {
  success: boolean;
  message: string;
  exit_code: number | null;
  reboot_required: boolean;
  traces_found: number;
  traces_cleaned: number;
  bytes_freed: number;
}

export interface UninstallProgressTargetResolved {
  stage: "target_resolved";
  program: InstalledProgram;
  uninstall_command: string | null;
  route: string;
}

export interface UninstallProgressUninstallStarted {
  stage: "uninstall_started";
  command: string;
}

export interface UninstallProgressUninstallCompleted {
  stage: "uninstall_completed";
  exit_code: number | null;
  reboot_required: boolean;
  used_job_object: boolean;
}

export interface UninstallProgressScanCompleted {
  stage: "scan_completed";
  traces: Trace[];
}

export interface UninstallProgressCleanCompleted {
  stage: "clean_completed";
  success_count: number;
  failed_count: number;
  bytes_freed: number;
}

export interface UninstallProgressFinished {
  stage: "finished";
  success: boolean;
  message: string;
}

export type UninstallProgress =
  | UninstallProgressTargetResolved
  | UninstallProgressUninstallStarted
  | UninstallProgressUninstallCompleted
  | UninstallProgressScanCompleted
  | UninstallProgressCleanCompleted
  | UninstallProgressFinished;

export interface CommandError {
  code?: string;
  message: string;
}

export interface MetadataWarmupProgress {
  kind: "icons" | "sizes";
  stage: string;
  current: number;
  total: number;
  program_id?: string;
  program_name?: string;
  status?: string;
  message?: string;
  program?: InstalledProgram;
}
