import { invoke } from "@tauri-apps/api/core";
import type {
  AppInfo,
  CaptureResult,
  DisplayInfo,
  RestorePreview,
  RestoreReport,
  SceneFile,
  SceneHealth,
  SceneSummary,
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
    const anyRaw = raw as { message?: unknown; code?: unknown };
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

  sceneList: () => call<SceneSummary[]>("scene_list"),
  sceneHealth: () => call<SceneHealth>("scene_health"),
  sceneSave: (name: string, capture: CaptureResult) =>
    call<SceneSummary>("scene_save", { name, capture }),
  sceneGet: (id: string) => call<SceneFile>("scene_get", { id }),
  sceneDelete: (id: string) => call<void>("scene_delete", { id }),

  restorePreview: (id: string) => call<RestorePreview>("restore_preview", { id }),
  restoreApply: (id: string, relaunchMissing: boolean) =>
    call<RestoreReport>("restore_apply", { id, relaunchMissing }),
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
