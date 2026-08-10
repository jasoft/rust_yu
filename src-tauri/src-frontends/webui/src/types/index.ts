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

export type HealthStatus = "healthy" | "review";
export type HealthSeverity = "info" | "warning" | "critical";
export type StartupImpact = "none" | "low" | "medium" | "high" | "unknown";

export interface HealthFinding {
  code: string;
  title: string;
  detail: string;
  severity: HealthSeverity;
}

export interface UpdateHint {
  url: string;
  source: string;
  message: string;
}

export interface ProgramHealth {
  program_id: string;
  program_name: string;
  publisher: string | null;
  version: string | null;
  score: number;
  status: HealthStatus;
  findings: HealthFinding[];
  duplicate_count: number;
  last_used: string | null;
  times_used: number | null;
  startup_entry_count: number;
  startup_impact: StartupImpact;
  update_hint: UpdateHint | null;
}

export interface HealthReport {
  evaluated_at: string;
  programs: ProgramHealth[];
  total_programs: number;
  review_count: number;
  healthy_count: number;
  warnings: string[];
}

export interface Trace {
  id: string;
  program_name: string;
  trace_type: "registry_key" | "registry_value" | "file" | "appdata" | "shortcut" | "scheduled_task" | "service" | "driver";
  path: string;
  exists: boolean;
  size: number | null;
  is_critical?: boolean;
  confidence: "high" | "medium" | "low";
  description: string | null;
  related_path?: string | null;
}

export type ForceTargetKind = "directory" | "executable" | "shortcut";

export interface ForceUninstallTarget {
  input_path: string;
  resolved_path: string;
  name: string;
  kind: ForceTargetKind;
}

export interface ForceUninstallPlan {
  plan_id: string;
  target: ForceUninstallTarget;
  fingerprint: string;
  traces: Trace[];
  default_selected_ids: string[];
  warnings: string[];
}

export interface ForceCleanupSelection {
  plan_id: string;
  trace_ids: string[];
  confirm: boolean;
}

export interface ForceUninstallResult {
  plan_id: string;
  success: boolean;
  message: string;
  traces_found: number;
  traces_cleaned: number;
  failed_count: number;
  bytes_freed: number;
  outcomes: CleanResult[];
}

export interface CleanResult {
  trace_id: string;
  path: string;
  success: boolean;
  error: string | null;
  bytes_freed: number;
  backup_id?: string | null;
}

export type BackupItemKind = "file" | "directory" | "registry_key" | "registry_value";
export type BackupItemState =
  | "ready"
  | "missing"
  | "backup_failed"
  | "delete_succeeded"
  | "delete_failed"
  | "restored"
  | "restore_failed";
export type BackupSessionStatus = "prepared" | "partially_cleaned" | "restored" | "restore_failed";

export interface BackupPlanItem {
  trace_id: string;
  path: string;
  trace_type: Trace["trace_type"];
  kind: BackupItemKind | null;
  exists: boolean;
  estimated_bytes: number;
  eligible: boolean;
  reason: string | null;
}

export interface BackupPlan {
  items: BackupPlanItem[];
  total_bytes: number;
  eligible_count: number;
  unsupported_count: number;
}

export interface BackupItem {
  trace_id: string;
  original_path: string;
  trace_type: Trace["trace_type"];
  kind: BackupItemKind;
  payload: string | null;
  bytes: number;
  state: BackupItemState;
  error: string | null;
}

export interface BackupSession {
  id: string;
  created_at: string;
  reason: string;
  status: BackupSessionStatus;
  items: BackupItem[];
}

export interface BackupSessionInfo {
  id: string;
  created_at: string;
  reason: string;
  status: BackupSessionStatus;
  item_count: number;
  restorable_count: number;
  failed_count: number;
  bytes: number;
}

export interface BackupRestoreResult {
  session_id: string;
  success: boolean;
  restored_count: number;
  failed_count: number;
  session: BackupSession;
}

export type MonitorChangeKind = "added" | "removed" | "modified";
export type MonitorItemKind = "file" | "directory" | "registry_key" | "registry_value";
export type InstallMonitorStatus = "waiting" | "completed" | "failed" | "cancelled" | "expired";
export type MonitorActivityKind = "install" | "update" | "normal_run";
export type MonitorEvidenceKind = "process" | "file" | "registry" | "service" | "scheduled_task" | "driver";
export type MonitorConfidence = "high" | "medium";

export interface MonitorRootInfo {
  path: string;
  source: string;
  enabled: boolean;
  reason: string | null;
}

export interface MonitorScope {
  file_roots: string[];
  registry_roots: string[];
}

export interface InstallMonitorPlan {
  program_id: string;
  program_name: string;
  scope: MonitorScope;
  file_roots: MonitorRootInfo[];
  registry_roots: MonitorRootInfo[];
  requires_admin: boolean;
  warnings: string[];
}

export interface InstallMonitorStartRequest {
  program: InstalledProgram;
  extra_file_roots: string[];
  extra_registry_roots: string[];
  activity_kind: MonitorActivityKind;
  expires_after_minutes: number | null;
}

export interface MonitorFileRecord {
  root_path: string;
  relative_path: string;
  kind: MonitorItemKind;
  size: number;
  modified_at: number | null;
  content_hash: number | null;
}

export interface MonitorRegistryRecord {
  key_path: string;
  value_name: string | null;
  value_type: number | null;
  bytes: number[];
}

export interface MonitorSnapshot {
  captured_at: string;
  files: MonitorFileRecord[];
  registry: MonitorRegistryRecord[];
  warnings: string[];
}

export interface MonitorSnapshotSummary {
  captured_at: string;
  file_count: number;
  registry_count: number;
  bytes: number;
  warning_count: number;
}

export interface MonitorChange {
  id: string;
  kind: MonitorChangeKind;
  item_kind: MonitorItemKind;
  trace_type: Trace["trace_type"];
  path: string;
  size_before: number | null;
  size_after: number | null;
  confidence: MonitorConfidence;
  description: string;
  evidence: string;
}

export interface InstallMonitorSession {
  id: string;
  program: InstalledProgram;
  scope: MonitorScope;
  created_at: string;
  completed_at: string | null;
  status: InstallMonitorStatus;
  activity_kind: MonitorActivityKind;
  expires_at: string | null;
  before: MonitorSnapshot;
  after: MonitorSnapshot | null;
  before_summary: MonitorSnapshotSummary;
  after_summary: MonitorSnapshotSummary | null;
  changes: MonitorChange[];
  evidence_events: MonitorEvidenceEvent[];
  system_before: Trace[];
  system_after: Trace[];
  warnings: string[];
}

export interface InstallMonitorSessionInfo {
  id: string;
  program_id: string;
  program_name: string;
  created_at: string;
  completed_at: string | null;
  status: InstallMonitorStatus;
  activity_kind: MonitorActivityKind;
  expires_at: string | null;
  changes_count: number;
  added_count: number;
  removed_count: number;
  modified_count: number;
  warning_count: number;
}

export interface MonitorEvidenceEvent {
  id: string;
  occurred_at: string;
  source: string;
  target: string;
  kind: MonitorEvidenceKind;
  operation: string;
  confidence: MonitorConfidence;
  parent_event_id: string | null;
  process_id: number | null;
  parent_process_id: number | null;
  note: string;
}

export type EvidenceConfidence = "high" | "medium" | "low" | "unknown";

export interface EvidenceRecord {
  id: string; category: string; target: string; source: string; observed_at: string;
  confidence: EvidenceConfidence; exists: boolean | null; result: string;
  destructive_eligible: boolean; note: string;
}

export interface ReconstructedEvidencePacket {
  id: string; schema_version: number; generated_at: string; program: InstalledProgram;
  inference_notice: string; vendor: string | null; signature_status: string;
  signature_subject: string | null; evidence: EvidenceRecord[]; warnings: string[];
  immutable_sha256: string;
}

export type CleanupPolicyKind = "audit" | "safe" | "recovery";
export interface CleanupPolicyProfile {
  kind: CleanupPolicyKind; title: string; description: string; analyze_only: boolean;
  require_confirmation: boolean; require_backup: boolean; allowed_confidence: EvidenceConfidence[];
  allowed_actions: string[]; irreversible_actions: string[];
}

export interface HibernationCandidate {
  item_id: string; name: string; source: string; command: string | null;
  association: EvidenceConfidence; reversible: boolean; prohibited: boolean; reason: string;
}
export interface HibernationPlan {
  id: string; program: InstalledProgram; created_at: string; last_used: string | null;
  times_used: number | null; candidates: HibernationCandidate[]; selected_item_ids: string[]; warnings: string[];
}
export interface HibernationResult { plan_id: string; applied: boolean; change_ids: string[]; errors: string[]; }

export interface InventoryEntry {
  program_id: string; name: string; publisher: string | null; version: string | null;
  source: string; install_location: string | null; uninstall_capability: string; signature_status: string;
}
export interface InventoryBaseline {
  id: string; schema_version: number; captured_at: string; machine_label: string;
  entries: InventoryEntry[]; sha256: string;
}
export interface InventoryDifference {
  key: string; name: string; status: "missing" | "added" | "version_changed";
  baseline_version: string | null; current_version: string | null; note: string;
}
export interface InventoryComparison {
  compared_at: string; baseline_id: string; baseline_captured_at: string;
  differences: InventoryDifference[]; read_only_notice: string;
}

export interface EvidenceBundleExport {
  report_id: string; path: string; generated_at: string; file_count: number;
  immutable_snapshot_sha256: string;
}

export interface MonitorExport {
  path: string;
  format: "json" | "csv";
  changes_count: number;
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

export type UninstallPhase =
  | "planned"
  | "running_uninstaller"
  | "verifying_removal"
  | "scanning_residues"
  | "awaiting_cleanup_confirmation"
  | "cleaning_residues"
  | "completed"
  | "cancelled"
  | "failed";

export interface UninstallJobSnapshot {
  job_id: string;
  program: InstalledProgram;
  fingerprint: string;
  route: string;
  traces: Trace[];
  selected_trace_ids: string[];
}

export interface UninstallResidueReview {
  traces: Trace[];
  default_selected_ids: string[];
}

export type UninstallEventPayload =
  | { kind: "planned" }
  | { kind: "uninstaller_started"; command_summary: string }
  | { kind: "uninstaller_completed"; exit_code: number | null; reboot_required: boolean }
  | { kind: "removal_verified"; removed: boolean }
  | { kind: "residues_scanned"; count: number }
  | { kind: "cleanup_started"; count: number }
  | { kind: "cleanup_completed"; success_count: number; failed_count: number }
  | { kind: "finished"; success: boolean; message: string };

export interface UninstallJobEvent {
  job_id: string;
  sequence: number;
  phase: UninstallPhase;
  payload: UninstallEventPayload;
}

export interface UninstallOutcome {
  success: boolean;
  message: string;
  exit_code: number | null;
  reboot_required: boolean;
  traces_found: number;
  traces_cleaned: number;
  bytes_freed: number;
}

export interface UninstallJob {
  snapshot: UninstallJobSnapshot;
  phase: UninstallPhase;
  next_sequence: number;
  events: UninstallJobEvent[];
  residue_review: UninstallResidueReview;
  outcome: UninstallOutcome | null;
}

export interface UninstallJobResponse {
  job: UninstallJob;
}

export type ReportExportFormat = "json" | "html" | "text";

export interface ReportInfo {
  id: string;
  name: string;
  created_at: string;
  path: string;
  success: boolean;
  traces_count: number;
  cleaned_count: number;
  failed_count: number;
  warning_count: number;
  formats: string[];
}

export interface ReportExport {
  path: string;
  format: ReportExportFormat;
  report_id: string;
}

export interface UninstallerReport {
  id: string;
  program_name: string;
  generated_at: string;
  traces_found: Trace[];
  traces_removed: CleanResult[];
  total_size_freed: number;
  success: boolean;
  warnings: string[];
  job: UninstallJob | null;
}

export type BatchUninstallItemStatus =
  | "queued"
  | "planning"
  | "running"
  | "completed"
  | "failed"
  | "cancelled";

export interface BatchUninstallItem {
  program: InstalledProgram;
  status: BatchUninstallItemStatus;
  job_id: string | null;
  job: UninstallJob | null;
  message: string | null;
  error: string | null;
  traces_found: number;
  traces: Trace[];
  bytes_freed: number;
}

export interface CleanupSelection {
  trace_ids: string[];
  confirm: boolean;
}

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

export type StartupSource =
  | "registry_run"
  | "registry_run_once"
  | "registry_policy_run"
  | "startup_folder"
  | "scheduled_task"
  | "service";

export type StartupScope = "user" | "machine";
export type StartupState = "enabled" | "disabled" | "broken";
export type StartupAction = "enable" | "disable" | "delete" | "rollback";

export interface StartupCapabilities {
  can_enable: boolean;
  can_disable: boolean;
  can_delete: boolean;
  can_rollback: boolean;
}

export interface StartupLocator {
  location: string;
  bucket: string | null;
}

export interface StartupItem {
  id: string;
  name: string;
  source: StartupSource;
  scope: StartupScope;
  state: StartupState;
  command: string | null;
  executable_path: string | null;
  arguments: string[];
  working_dir: string | null;
  target_exists: boolean | null;
  requires_admin: boolean;
  capabilities: StartupCapabilities;
  locator: StartupLocator;
  warnings: string[];
  description: string | null;
  raw: unknown | null;
}

export interface StartupListResponse {
  items: StartupItem[];
  total: number;
  applied_limit: number | null;
  applied_offset: number;
}

export interface StartupErrorDetail {
  code: string;
  message: string;
}

export interface StartupEnvelope<T> {
  ok: boolean;
  data: T | null;
  warnings: string[];
  error: StartupErrorDetail | null;
}

export interface StartupActionPlan {
  item_id: string;
  action: StartupAction;
  apply_requested: boolean;
  will_apply: boolean;
  requires_admin: boolean;
  change_id: string | null;
  warnings: string[];
  operations: string[];
  snapshot_available: boolean;
}

export interface StartupActionResult {
  item_id: string | null;
  action: StartupAction;
  applied: boolean;
  change_id: string | null;
  warnings: string[];
  operations: string[];
}

export interface CleanerEntrySummary {
  id: string;
  name: string;
  category: string;
  warning: string | null;
  default_enabled: boolean;
  file_rule_count: number;
  registry_rule_count: number;
}

export interface CleanerCatalog {
  entries: CleanerEntrySummary[];
  database_version: string;
  total_rule_count: number;
  detected_rule_count: number;
}

export type CleanerTargetKind = "file" | "registry_key" | "registry_value";

export interface CleanerTarget {
  id: string;
  entry_id: string;
  entry_name: string;
  kind: CleanerTargetKind;
  path: string;
  value_name: string | null;
  size: number;
  requires_admin: boolean;
  blocked_reason: string | null;
}

export interface CleanerScanResult {
  targets: CleanerTarget[];
  total_bytes: number;
  truncated: boolean;
}

export interface CleanerCleanItemResult {
  target_id: string;
  path: string;
  success: boolean;
  error: string | null;
  bytes_freed: number;
  dry_run: boolean;
}

export interface CleanerCleanResult {
  items: CleanerCleanItemResult[];
  bytes_freed: number;
}

export type ShredMethod = "quick" | "standard" | "thorough";

export interface ShredTargetPlan {
  path: string;
  kind: "file" | "directory" | "missing";
  file_count: number;
  size: number;
  blocked_reason: string | null;
}

export interface ShredPlan {
  method: ShredMethod;
  targets: ShredTargetPlan[];
  total_files: number;
  total_bytes: number;
  overwrite_bytes: number;
  confirmation_token: string;
  confirmation_text: string;
  warnings: string[];
}

export interface ShredProgress {
  stage: "overwriting" | "completed";
  current_path: string;
  processed_bytes: number;
  total_bytes: number;
  pass: number;
  total_passes: number;
  message: string;
}

export interface ShredResult {
  dry_run: boolean;
  shredded_files: number;
  deleted_directories: number;
  bytes_overwritten: number;
  failures: Array<{ path: string; error: string }>;
}

export type BrowserCleanupKind = "cache" | "extension";

export interface BrowserInfo {
  id: string;
  name: string;
  profile_count: number;
  running: boolean;
}

export interface BrowserCleanupItem {
  id: string;
  browser_id: string;
  browser_name: string;
  profile: string;
  kind: BrowserCleanupKind;
  name: string;
  description: string;
  path: string;
  size: number;
  selected_by_default: boolean;
  confidence: "high" | "medium" | "low";
}

export interface BrowserScanResult {
  browsers: BrowserInfo[];
  items: BrowserCleanupItem[];
  total_size: number;
}

export interface BrowserCleanupResult {
  dry_run: boolean;
  outcomes: Array<{
    item_id: string;
    name: string;
    success: boolean;
    bytes_freed: number;
    error: string | null;
  }>;
  bytes_freed: number;
}
