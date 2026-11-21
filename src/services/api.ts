import { invoke } from "@tauri-apps/api/core";
import type {
  AdapterSuggestion,
  AppInfo,
  AppSettings,
  CaptureResult,
  DisplayInfo,
  LayoutUpdate,
  RecoveryHint,
  RestorePreview,
  RestoreReport,
  RunSummary,
  SceneHealth,
  SnapshotDiff,
  SnapshotSummary,
  StorageStats,
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
  captureStart: () => call<CaptureResult>("capture_start"),
  sceneHealth: () => call<SceneHealth>("scene_health"),

  workspaceList: () => call<WorkspaceSummary[]>("workspace_list"),
  workspaceGet: (id: string) => call<WorkspaceDetail>("workspace_get", { id }),
  workspaceCreate: (name: string, capture: CaptureResult) =>
    call<WorkspaceSummary>("workspace_create", { name, capture }),
  workspaceRename: (id: string, name: string) =>
    call<void>("workspace_rename", { id, name }),
  workspaceDelete: (id: string) => call<void>("workspace_delete", { id }),
  workspaceItemUpdate: (
    itemId: string,
    reusePolicy: string,
    launchArgs: string | null,
    enabled: boolean,
  ) =>
    call<void>("workspace_item_update", {
      itemId,
      reusePolicy,
      launchArgs,
      enabled,
    }),
  restoreIsBusy: () => call<boolean>("restore_is_busy"),

  workspaceLayoutUpdate: (workspaceId: string, updates: LayoutUpdate[]) =>
    call<void>("workspace_layout_update", { workspaceId, updates }),
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

  snapshotList: (workspaceId: string, limit = 50) =>
    call<SnapshotSummary[]>("snapshot_list", { workspaceId, limit }),
  snapshotCreate: (workspaceId: string, name: string) =>
    call<SnapshotSummary>("snapshot_create", { workspaceId, name }),
  snapshotDelete: (id: string) => call<void>("snapshot_delete", { id }),
  snapshotDiff: (idA: string, idB: string) =>
    call<SnapshotDiff>("snapshot_diff", { idA, idB }),
  snapshotPrune: (workspaceId: string, keep: number) =>
    call<number>("snapshot_prune", { workspaceId, keep }),

  recoveryHint: () => call<RecoveryHint | null>("recovery_hint"),
  recoveryDismiss: () => call<void>("recovery_dismiss"),
  adapterSuggest: (exeName: string, target: string | null) =>
    call<AdapterSuggestion>("adapter_suggest", { exeName, target }),

  settingsGet: () => call<AppSettings>("settings_get"),
  settingsSet: (settings: AppSettings) => call<AppSettings>("settings_set", { settings }),
  storageStats: () => call<StorageStats>("storage_stats"),
  storageCleanup: () => call<number>("storage_cleanup"),
  workspacePreview: (id: string) => call<RestorePreview>("workspace_preview", { id }),
  workspaceRestore: (id: string, relaunchMissing: boolean) =>
    call<RestoreReport>("workspace_restore", { id, relaunchMissing }),

  /** 按快照恢复：撤销这次恢复、历史回退都用它 */
  snapshotRestore: (id: string, relaunchMissing: boolean) =>
    call<RestoreReport>("snapshot_restore", { id, relaunchMissing }),

  runList: (limit = 20) => call<RunSummary[]>("run_list", { limit }),
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
