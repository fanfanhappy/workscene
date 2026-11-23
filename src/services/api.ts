import { invoke } from "@tauri-apps/api/core";
import type {
  AdapterSuggestion,
  AppInfo,
  AppSettings,
  BookmarkFolder,
  CaptureResult,
  DisplayInfo,
  ImportAllResult,
  ImportPreview,
  LayoutUpdate,
  RecoveryHint,
  RestorePreview,
  RestoreReport,
  RunStepRow,
  RunSummary,
  SavedProfileLayout,
  SceneFile,
  SceneHealth,
  SnapshotDiff,
  SnapshotSummary,
  StorageStats,
  UrlBatchEntry,
  WorkspaceDetail,
  WorkspaceItem,
  WorkspaceSummary,
} from "@/types/contract";

/**
 * 唯一允许 invoke 的地方（UI 规范 §18.2：不在页面散落 invoke）。
 * 失败统一转成 Error，message 直接可展示。
 */
async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (raw) {
    throw new Error(normalize(raw));
  }
}

function normalize(raw: unknown): string {
  if (typeof raw === "string") return raw;
  if (raw && typeof raw === "object") {
    const anyRaw = raw as { message?: unknown };
    if (typeof anyRaw.message === "string") return anyRaw.message;
    try {
      return JSON.stringify(raw);
    } catch {
      return "未知错误";
    }
  }
  return String(raw ?? "未知错误");
}

export const api = {
  getAppInfo: () => call<AppInfo>("system_get_info"),
  getDisplays: () => call<DisplayInfo[]>("system_get_displays"),
  getHotkeyStatus: () =>
    call<import("@/types/contract").HotkeyStatus>("system_hotkey_status"),
  openLogDir: () => call<void>("system_open_log_dir"),
  exportDiagnostics: () => call<string>("system_export_diagnostics"),
  browserBridgeStatus: () =>
    call<import("@/types/contract").BrowserBridgeStatus>("browser_bridge_status"),
  browserBridgeInstall: () =>
    call<import("@/types/contract").BrowserBridgeStatus>("browser_bridge_install"),
  openBrowserExtensionDir: () => call<void>("browser_bridge_open_extension_dir"),
  captureStart: () => call<CaptureResult>("capture_start"),
  sceneHealth: () => call<SceneHealth>("scene_health"),
  windowSetTheme: (theme: "light" | "dark") => call<void>("window_set_theme", { theme }),

  workspaceList: () => call<WorkspaceSummary[]>("workspace_list"),
  workspaceGet: (id: string) => call<WorkspaceDetail>("workspace_get", { id }),
  workspaceCreate: (name: string, capture: CaptureResult) =>
    call<WorkspaceSummary>("workspace_create", { name, capture }),
  workspaceSetHotkeySlot: (id: string, slot: number | null) =>
    call<void>("workspace_set_hotkey_slot", { id, slot }),
  workspaceSetFavorite: (id: string, isFavorite: boolean) =>
    call<void>("workspace_set_favorite", { id, isFavorite }),
  workspaceUpdateMeta: (
    id: string,
    name: string,
    description: string | null,
    autoSnapshotEnabled: boolean,
    color: string | null = null,
  ) =>
    call<void>("workspace_update_meta", {
      id,
      name,
      description,
      autoSnapshotEnabled,
      color,
    }),
  workspaceDuplicate: (id: string, newName: string) =>
    call<WorkspaceSummary>("workspace_duplicate", { id, newName }),
  workspaceExport: (id: string) => call<string>("workspace_export", { id }),
  workspaceExportAll: () => call<string>("workspace_export_all"),
  workspaceImportAll: (json: string) =>
    call<ImportAllResult>("workspace_import_all", { json }),
  workspaceImportPreview: (json: string) =>
    call<ImportPreview>("workspace_import_preview", { rawJson: json }),
  workspaceImport: (json: string) => call<WorkspaceSummary>("workspace_import", { json }),
  workspaceRecapture: (id: string) =>
    call<WorkspaceDetail>("workspace_recapture", { id }),
  workspaceUpdateFromCapture: (id: string, capture: CaptureResult) =>
    call<WorkspaceDetail>("workspace_update_from_capture", { id, capture }),
  workspaceDelete: (id: string) => call<void>("workspace_delete", { id }),
  workspaceCloseWindows: (id: string) => call<number>("workspace_close_windows", { id }),
  workspaceItemUpdate: (
    itemId: string,
    reusePolicy: string,
    launchArgs: string | null,
    enabled: boolean,
    launchPriority?: number,
    launchDelayMs?: number,
  ) =>
    call<void>("workspace_item_update", {
      itemId,
      reusePolicy,
      launchArgs,
      enabled,
      launchPriority,
      launchDelayMs,
    }),
  /** 只提交改动字段，避免失焦保存与点击开关互相覆盖 */
  workspaceItemPatch: (
    itemId: string,
    patch: {
      reusePolicy?: string;
      launchArgs?: string | null;
      enabled?: boolean;
      launchPriority?: number;
      launchDelayMs?: number;
    },
  ) =>
    call<void>("workspace_item_patch", {
      itemId,
      reusePolicy: patch.reusePolicy ?? null,
      launchArgs: patch.launchArgs ?? null,
      enabled: patch.enabled ?? null,
      launchPriority: patch.launchPriority ?? null,
      launchDelayMs: patch.launchDelayMs ?? null,
    }),
  workspaceItemRelocate: (workspaceId: string, itemId: string, newPath: string) =>
    call<WorkspaceItem>("workspace_item_relocate", {
      workspaceId,
      itemId,
      newPath,
    }),
  appIconsBatch: (paths: string[]) =>
    call<Record<string, string>>("app_icons_batch", { paths }),
  bookmarksListFolders: () => call<BookmarkFolder[]>("bookmarks_list_folders"),
  workspaceItemsAddBatch: (workspaceId: string, entries: UrlBatchEntry[]) =>
    call<number>("workspace_items_add_batch", { workspaceId, entries }),
  restoreIsBusy: () => call<boolean>("restore_is_busy"),
  restoreCancel: () => call<boolean>("restore_cancel"),
  systemOpenDataDir: () => call<void>("system_open_data_dir"),
  systemOpenStartupDir: () => call<void>("system_open_startup_dir"),
  dialogPickFile: (exeOnly = false) =>
    call<string | null>("dialog_pick_file", { exeOnly }),
  dialogPickFolder: () => call<string | null>("dialog_pick_folder"),
  dialogSaveJson: (defaultName: string, content: string) =>
    call<string | null>("dialog_save_json", { defaultName, content }),

  workspaceLayoutUpdate: (workspaceId: string, updates: LayoutUpdate[]) =>
    call<void>("workspace_layout_update", { workspaceId, updates }),
  /** 删除 + 更新一次事务提交，画布保存不会只成功一半 */
  workspaceLayoutSave: (
    workspaceId: string,
    removedWindowIds: string[],
    updates: LayoutUpdate[],
  ) =>
    call<void>("workspace_layout_save", {
      workspaceId,
      removedWindowIds,
      updates,
    }),
  workspaceWindowDelete: (workspaceId: string, windowId: string) =>
    call<void>("workspace_window_delete", { workspaceId, windowId }),
  workspaceItemDelete: (workspaceId: string, itemId: string) =>
    call<void>("workspace_item_delete", { workspaceId, itemId }),
  workspaceItemAdd: (
    workspaceId: string,
    itemType: string,
    target: string,
    displayName: string | null = null,
  ) =>
    call<WorkspaceItem>("workspace_item_add", {
      workspaceId,
      itemType,
      target,
      displayName,
    }),
  workspaceAddApp: (
    workspaceId: string,
    exePath: string,
    displayName: string | null = null,
    launchArgs: string | null = null,
  ) =>
    call<WorkspaceDetail>("workspace_add_app", {
      workspaceId,
      exePath,
      displayName,
      launchArgs,
    }),

  snapshotList: (workspaceId: string, limit = 50) =>
    call<SnapshotSummary[]>("snapshot_list", { workspaceId, limit }),
  snapshotGet: (id: string) => call<SceneFile>("snapshot_get", { id }),
  snapshotCreate: (workspaceId: string, name: string) =>
    call<SnapshotSummary>("snapshot_create", { workspaceId, name }),
  snapshotSaveAsWorkspace: (snapshotId: string, name: string) =>
    call<WorkspaceSummary>("snapshot_save_as_workspace", { snapshotId, name }),
  snapshotDelete: (id: string) => call<void>("snapshot_delete", { id }),
  snapshotDiff: (idA: string, idB: string) =>
    call<SnapshotDiff>("snapshot_diff", { idA, idB }),
  snapshotPrune: (workspaceId: string, keep: number) =>
    call<number>("snapshot_prune", { workspaceId, keep }),

  recoveryHint: () => call<RecoveryHint | null>("recovery_hint"),
  recoveryDismiss: () => call<void>("recovery_dismiss"),
  adapterSuggest: (exeName: string, target: string | null) =>
    call<AdapterSuggestion>("adapter_suggest", { exeName, target }),
  adapterClassifyBatch: (exeNames: string[]) =>
    call<Record<string, string>>("adapter_classify_batch", { exeNames }),

  settingsGet: () => call<AppSettings>("settings_get"),
  settingsSet: (settings: AppSettings) => call<AppSettings>("settings_set", { settings }),
  storageStats: () => call<StorageStats>("storage_stats"),
  storageCleanup: () => call<number>("storage_cleanup"),
  workspacePreview: (id: string) => call<RestorePreview>("workspace_preview", { id }),
  workspaceRestore: (id: string, relaunchMissing: boolean, focusMode = false) =>
    call<RestoreReport>("workspace_restore", { id, relaunchMissing, focusMode }),
  workspaceRestoreRetry: (
    id: string,
    relaunchMissing: boolean,
    targetNames: string[],
  ) =>
    call<RestoreReport>("workspace_restore_retry", {
      id,
      relaunchMissing,
      targetNames,
    }),

  /** 按快照恢复：撤销这次恢复、历史回退都用它 */
  snapshotRestore: (id: string, relaunchMissing: boolean) =>
    call<RestoreReport>("snapshot_restore", { id, relaunchMissing }),

  runList: (limit = 20) => call<RunSummary[]>("run_list", { limit }),
  runSteps: (runId: string) => call<RunStepRow[]>("run_steps", { runId }),
  runErrorStats: () => call<[string, number][]>("run_error_stats"),
  runClear: () => call<number>("run_clear"),

  captureDetectContext: (windows: import("@/types/contract").WindowInfo[]) =>
    call<import("@/types/contract").DetectedContext[]>("capture_detect_context", { windows }),
  workspaceProfileList: (workspaceId: string) =>
    call<import("@/types/contract").DisplayProfileSummary[]>("workspace_profile_list", {
      workspaceId,
    }),
  workspaceProfileGet: (workspaceId: string, displaySignature: string) =>
    call<SavedProfileLayout[]>("workspace_profile_get", {
      workspaceId,
      displaySignature,
    }),
  workspaceProfileSave: (workspaceId: string) =>
    call<import("@/types/contract").DisplayProfileSummary>("workspace_profile_save", {
      workspaceId,
    }),
  workspaceProfileDelete: (workspaceId: string, displaySignature: string) =>
    call<void>("workspace_profile_delete", { workspaceId, displaySignature }),
  workspaceHooksGet: (workspaceId: string) =>
    call<import("@/types/contract").WorkspaceHooks>("workspace_hooks_get", { workspaceId }),
  workspaceHooksSet: (
    workspaceId: string,
    hooks: import("@/types/contract").WorkspaceHooks,
  ) => call<void>("workspace_hooks_set", { workspaceId, hooks }),
  workspaceHookTest: (cmd: string) => call<string>("workspace_hook_test", { cmd }),
  workspaceResourceUpdate: (
    workspaceId: string,
    itemId: string,
    displayName: string,
    target: string,
  ) =>
    call<void>("workspace_resource_update", {
      workspaceId,
      itemId,
      displayName,
      target,
    }),
};

/** 统一的时间展示，避免各页面自己格式化 */
export function formatTime(epochMs: number): string {
  const d = new Date(epochMs);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(
    d.getHours(),
  )}:${pad(d.getMinutes())}`;
}

export function formatRelative(epochMs: number): string {
  const diff = Date.now() - epochMs;
  const min = Math.floor(diff / 60000);
  if (min < 1) return "刚刚";
  if (min < 60) return `${min} 分钟前`;
  const hour = Math.floor(min / 60);
  if (hour < 24) return `${hour} 小时前`;
  const day = Math.floor(hour / 24);
  if (day < 30) return `${day} 天前`;
  return formatTime(epochMs);
}
