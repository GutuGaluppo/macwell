// Typed wrappers around the Rust commands. The UI never touches the filesystem or runs commands
// itself: everything goes through these functions.
import { Channel, invoke } from "@tauri-apps/api/core";
import type {
  AppError,
  BackupAck,
  BackupGate,
  BackupState,
  ArtifactRemoval,
  BuildInfo,
  OperationOutcome,
  TrashSummary,
  CarePlan,
  CloudFolder,
  DevRoot,
  ExclusionPreview,
  Exported,
  HomebrewReport,
  Report,
  ReportFormat,
  TmExclusion,
  UndoneExclusion,
  PushPlan,
  PushResult,
  QuarantineList,
  RemovalMode,
  SecretScan,
  SnapshotPlan,
  SnapshotRecord,
  SnapshotResult,
  EvictOutcome,
  RulesReport,
  Risk,
  Health,
  ProcessList,
  MenuBarSettings,
  UninstallPlan,
  Volume,
  IgnoreEntry,
  JobEvent,
  Location,
  OperationRecord,
  PermissionStatus,
  ScanRecord,
  ScanResult,
  StorageNode,
} from "../types";

export function errorMessage(err: unknown): string {
  if (typeof err === "string") return err;
  if (err && typeof err === "object" && "message" in err) return String((err as AppError).message);
  return String(err);
}

/** Starts a background job; events arrive on `onEvent`. Resolves with the job id. */
export function startJob<T>(command: string, args: Record<string, unknown>, onEvent: (e: JobEvent<T>) => void) {
  const channel = new Channel<JobEvent<T>>();
  channel.onmessage = onEvent;
  return invoke<string>(command, { ...args, onEvent: channel });
}

export const api = {
  cancelJob: (jobId: string) => invoke<boolean>("cancel_job", { jobId }),
  storageNode: (scanId: string, path?: string, depth = 1, limit = 200) =>
    invoke<StorageNode>("storage_node", { scanId, path, depth, limit }),
  permissionStatus: () => invoke<PermissionStatus>("permission_status"),
  openSystemSettings: (pane: "fullDiskAccess" | "loginItems" | "storage" | "photos") =>
    invoke<void>("open_system_settings", { pane }),
  suggestedLocations: () => invoke<Location[]>("suggested_locations"),
  appInfo: () => invoke<BuildInfo>("app_info"),
  recentScans: (module?: string, limit?: number) => invoke<ScanRecord[]>("recent_scans", { module, limit }),
  operationLog: (limit?: number) => invoke<OperationRecord[]>("operation_log", { limit }),
  localMetrics: () => invoke<Record<string, number>>("local_metrics"),
  ignoreList: () => invoke<IgnoreEntry[]>("ignore_list"),
  addToIgnoreList: (path: string, reason?: string) => invoke<void>("add_to_ignore_list", { path, reason }),
  removeFromIgnoreList: (path: string) => invoke<void>("remove_from_ignore_list", { path }),
  diskOverview: () => invoke<Volume[]>("disk_overview"),
  trashSummary: () => invoke<TrashSummary>("trash_summary"),
  emptyTrash: () => invoke<OperationOutcome[]>("empty_trash"),
  emptyTrashWithFinder: () => invoke<void>("empty_trash_with_finder"),
  revealInFinder: (path: string) => invoke<void>("reveal_in_finder", { path }),
  quickLook: (path: string) => invoke<void>("quick_look", { path }),
  moveToTrash: (request: { items: { path: string; size: number }[]; scanRoot?: string }) =>
    invoke<OperationOutcome[]>("move_to_trash", { request }),
  pathExists: (path: string) => invoke<boolean>("path_exists", { path }),
  imageThumbnail: (path: string) => invoke<string | null>("image_thumbnail", { path }),
  photosStatus: (request = false) => invoke<string>("photos_status", { request }),
  photoThumbnail: (id: string) => invoke<string | null>("photo_thumbnail", { id }),
  deletePhotos: (ids: string[]) => invoke<boolean>("delete_photos", { ids }),
  cloudFolders: () => invoke<CloudFolder[]>("cloud_folders"),
  evictICloud: (paths: string[]) => invoke<EvictOutcome[]>("evict_icloud", { paths }),
  openAppStorePage: (url: string) => invoke<void>("open_app_store_page", { url }),
  openApplication: (path: string) => invoke<void>("open_application", { path }),
  cleanupRules: () => invoke<RulesReport>("cleanup_rules"),
  addCustomRule: (title: string, folder: string, risk: Risk) => invoke<void>("add_custom_rule", { title, folder, risk }),
  removeCustomRule: (id: string) => invoke<void>("remove_custom_rule", { id }),
  runCleanupRules: (items: { ruleId: string; path: string; size: number }[], backupAck?: BackupAck) => invoke<OperationOutcome[]>("run_cleanup_rules", { items, backupAck }),
  runSmartCare: (plan: CarePlan, backupAck?: BackupAck) => invoke<OperationOutcome[]>("run_smart_care", { plan, backupAck }),
  health: () => invoke<Health>("health"),
  processList: () => invoke<ProcessList>("process_list"),
  quitApplication: (appPath: string) => invoke<number>("quit_application", { appPath }),
  menuBarSettings: () => invoke<MenuBarSettings>("menu_bar_settings"),
  setMenuBarSettings: (settings: MenuBarSettings) => invoke<void>("set_menu_bar_settings", { settings }),
  appIcon: (path: string) => invoke<string | null>("app_icon", { path }),
  uninstallPlan: (path: string) => invoke<UninstallPlan>("uninstall_plan", { path }),
  uninstallApp: (path: string, leftovers: string[]) => invoke<OperationOutcome[]>("uninstall_app", { path, leftovers }),
  removeOrphans: (items: { path: string; size: number }[], backupAck?: BackupAck) => invoke<OperationOutcome[]>("remove_orphans", { items, backupAck }),
  devRoots: () => invoke<DevRoot[]>("list_dev_roots"),
  addDevRoot: (path: string) => invoke<void>("add_dev_root", { path }),
  removeDevRoot: (path: string) => invoke<void>("remove_dev_root", { path }),
  scanRepoSecrets: (repo: string) => invoke<SecretScan>("scan_repo_secrets", { repo }),
  planWipSnapshot: (repo: string, excludes: string[]) => invoke<SnapshotPlan>("plan_wip_snapshot", { repo, excludes }),
  createWipSnapshot: (repo: string, excludes: string[]) => invoke<SnapshotResult>("create_wip_snapshot", { repo, excludes }),
  devSnapshots: (repo?: string) => invoke<SnapshotRecord[]>("list_dev_snapshots", { repo }),
  planPush: (repo: string, refName: string, remote: string) => invoke<PushPlan>("plan_push_snapshot", { repo, refName, remote }),
  /** Network: one anonymous request to api.github.com, only when the user asks. */
  checkRemoteVisibility: (repo: string, remote: string, refName: string) => invoke<PushPlan>("check_remote_visibility", { repo, remote, refName }),
  pushSnapshot: (repo: string, refName: string, remote: string, confirmed: boolean, acceptExposure: boolean) =>
    invoke<PushResult>("push_snapshot", { repo, refName, remote, confirmed, acceptExposure }),
  removeDevArtifacts: (items: ArtifactRemoval[], backupAck?: BackupAck) => invoke<OperationOutcome[]>("remove_dev_artifacts", { items, backupAck }),
  getBackupState: () => invoke<BackupState>("get_backup_state"),
  /** The gate before a batch cleanup: backup state plus repositories with work only on this Mac. */
  getBackupGate: () => invoke<BackupGate>("get_backup_gate"),
  previewTmExclusions: (items: ArtifactRemoval[]) => invoke<ExclusionPreview>("preview_tm_exclusions", { items }),
  /** Changes a Time Machine setting for exactly the approved folders; each is checked again first. */
  applyTmExclusions: (items: ArtifactRemoval[]) => invoke<OperationOutcome[]>("apply_tm_exclusions", { items }),
  listTmExclusions: () => invoke<TmExclusion[]>("list_tm_exclusions"),
  undoTmExclusions: (paths: string[]) => invoke<UndoneExclusion[]>("undo_tm_exclusions", { paths }),
  /** Read only: `brew outdated` plus a look at your project folders. Never upgrades anything. */
  analyzeHomebrew: () => invoke<HomebrewReport>("analyze_homebrew"),
  generateReport: (periodDays: number) => invoke<Report>("generate_report", { periodDays }),
  /** Rebuilds the report on the Rust side and writes it where the user chose. Fails with kind "exists" unless `overwrite`. */
  exportReport: (format: ReportFormat, path: string, overwrite: boolean, periodDays: number) =>
    invoke<Exported>("export_report", { format, path, overwrite, periodDays }),
  getRemovalMode: () => invoke<RemovalMode>("get_removal_mode"),
  setRemovalMode: (mode: RemovalMode) => invoke<void>("set_removal_mode", { mode }),
  listQuarantine: () => invoke<QuarantineList>("list_quarantine"),
  /** Needs the user's confirmation. Never overwrites: `to` picks another folder when the original place is taken. */
  restoreQuarantine: (ids: string[], to?: string) => invoke<OperationOutcome[]>("restore_quarantine", { ids, to, confirmed: true }),
  /** Sends the items to the Trash (not a permanent deletion). Needs the user's confirmation. */
  emptyQuarantine: (ids: string[]) => invoke<OperationOutcome[]>("empty_quarantine", { ids, confirmed: true }),
  setBackupStaleDays: (days: number) => invoke<void>("set_backup_stale_days", { days }),
  moveDuplicatesToTrash: (
    groups: { all: string[]; remove: { path: string; size: number; modifiedAt: number | null }[] }[],
    scanRoot?: string,
    backupAck?: BackupAck,
  ) => invoke<OperationOutcome[]>("move_duplicates_to_trash", { groups, scanRoot, backupAck }),
};

export type { ScanResult };
