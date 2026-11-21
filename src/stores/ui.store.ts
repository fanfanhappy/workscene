import { create } from "zustand";
import { api } from "@/services/api";

export type ThemeMode = "dark" | "light" | "system";
export type ResolvedTheme = "dark" | "light";
export type ToastTone = "info" | "success" | "warning" | "danger";

export interface ToastItem {
  id: string;
  tone: ToastTone;
  title: string;
  detail?: string;
}

export interface ConfirmOptions {
  title: string;
  description: string;
  detail?: string;
  confirmLabel?: string;
  cancelLabel?: string;
  variant?: "primary" | "danger";
}

interface ConfirmRequest extends ConfirmOptions {
  resolve: (confirmed: boolean) => void;
}

interface UiState {
  themeMode: ThemeMode;
  resolvedTheme: ResolvedTheme;
  sidebarCollapsed: boolean;
  commandPaletteOpen: boolean;
  onboardingOpen: boolean;
  captureOpen: boolean;
  restoreWorkspaceId: string | null;
  toasts: ToastItem[];
  confirmRequest: ConfirmRequest | null;

  setThemeMode: (mode: ThemeMode) => void;
  cycleThemeMode: () => void;
  syncSystemTheme: () => void;
  setSidebarCollapsed: (collapsed: boolean) => void;
  toggleSidebar: () => void;
  setCommandPaletteOpen: (open: boolean) => void;
  toggleCommandPalette: () => void;
  openOnboarding: () => void;
  completeOnboarding: () => void;
  openCapture: () => void;
  closeCapture: () => void;
  openRestore: (workspaceId: string) => void;
  closeRestore: () => void;

  pushToast: (toast: Omit<ToastItem, "id">, durationMs?: number) => void;
  dismissToast: (id: string) => void;

  confirm: (options: ConfirmOptions) => Promise<boolean>;
  resolveConfirm: (confirmed: boolean) => void;
}

const THEME_KEY = "workscene.theme";
const SIDEBAR_KEY = "workscene.sidebarCollapsed";
const ONBOARDING_KEY = "workscene.onboarding.v1";

function getSystemTheme(): ResolvedTheme {
  if (typeof window === "undefined") return "dark";
  return window.matchMedia("(prefers-color-scheme: light)").matches ? "light" : "dark";
}

function resolveTheme(mode: ThemeMode): ResolvedTheme {
  return mode === "system" ? getSystemTheme() : mode;
}

function applyDomTheme(resolved: ResolvedTheme) {
  if (typeof document === "undefined") return;
  document.documentElement.dataset.theme = resolved;
  document.documentElement.classList.toggle("dark", resolved === "dark");
  // 同步 Windows 原生标题栏深浅色（浏览器预览环境下自动忽略）
  if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
    api.windowSetTheme(resolved).catch(() => undefined);
  }
}

function loadInitialTheme(): ThemeMode {
  if (typeof localStorage === "undefined") return "dark";
  const saved = localStorage.getItem(THEME_KEY);
  if (saved === "dark" || saved === "light" || saved === "system") return saved;
  return "dark";
}

function loadInitialSidebar(): boolean {
  if (typeof localStorage === "undefined") return false;
  return localStorage.getItem(SIDEBAR_KEY) === "1";
}

function loadInitialOnboardingOpen(): boolean {
  if (typeof localStorage === "undefined") return false;
  return localStorage.getItem(ONBOARDING_KEY) !== "1";
}

const initialMode = loadInitialTheme();
const initialResolved = resolveTheme(initialMode);
applyDomTheme(initialResolved);

let toastSeq = 0;

export const useUiStore = create<UiState>((set, get) => ({
  themeMode: initialMode,
  resolvedTheme: initialResolved,
  sidebarCollapsed: loadInitialSidebar(),
  commandPaletteOpen: false,
  onboardingOpen: loadInitialOnboardingOpen(),
  captureOpen: false,
  restoreWorkspaceId: null,
  toasts: [],
  confirmRequest: null,

  setThemeMode: (mode) => {
    const resolved = resolveTheme(mode);
    applyDomTheme(resolved);
    try {
      localStorage.setItem(THEME_KEY, mode);
    } catch {
      // ignore storage errors
    }
    set({ themeMode: mode, resolvedTheme: resolved });
  },

  cycleThemeMode: () => {
    const cur = get().themeMode;
    const next: ThemeMode = cur === "dark" ? "light" : cur === "light" ? "system" : "dark";
    get().setThemeMode(next);
  },

  syncSystemTheme: () => {
    const mode = get().themeMode;
    if (mode !== "system") return;
    const resolved = getSystemTheme();
    applyDomTheme(resolved);
    set({ resolvedTheme: resolved });
  },

  setSidebarCollapsed: (collapsed) => {
    try {
      localStorage.setItem(SIDEBAR_KEY, collapsed ? "1" : "0");
    } catch {
      // ignore
    }
    set({ sidebarCollapsed: collapsed });
  },

  toggleSidebar: () => {
    get().setSidebarCollapsed(!get().sidebarCollapsed);
  },

  setCommandPaletteOpen: (open) => set({ commandPaletteOpen: open }),
  toggleCommandPalette: () => set((s) => ({ commandPaletteOpen: !s.commandPaletteOpen })),
  openOnboarding: () => set({ onboardingOpen: true, commandPaletteOpen: false }),
  completeOnboarding: () => {
    try {
      localStorage.setItem(ONBOARDING_KEY, "1");
    } catch {
      // ignore
    }
    set({ onboardingOpen: false });
  },
  openCapture: () => set({ captureOpen: true, commandPaletteOpen: false }),
  closeCapture: () => set({ captureOpen: false }),
  openRestore: (workspaceId) =>
    set({ restoreWorkspaceId: workspaceId, commandPaletteOpen: false }),
  closeRestore: () => set({ restoreWorkspaceId: null }),

  pushToast: (toast, durationMs = 3600) => {
    const id = `t-${++toastSeq}`;
    set((s) => ({ toasts: [...s.toasts.slice(-3), { ...toast, id }] }));
    if (durationMs > 0) {
      window.setTimeout(() => {
        get().dismissToast(id);
      }, durationMs);
    }
  },

  dismissToast: (id) => {
    set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) }));
  },

  confirm: (options) =>
    new Promise<boolean>((resolve) => {
      const prev = get().confirmRequest;
      if (prev) prev.resolve(false);
      set({ confirmRequest: { ...options, resolve } });
    }),

  resolveConfirm: (confirmed) => {
    const req = get().confirmRequest;
    if (req) {
      set({ confirmRequest: null });
      req.resolve(confirmed);
    }
  },
}));

/** 快捷 Toast 工具，可在任意事件回调里调用 */
export const toast = {
  info: (title: string, detail?: string) =>
    useUiStore.getState().pushToast({ tone: "info", title, detail }),
  success: (title: string, detail?: string) =>
    useUiStore.getState().pushToast({ tone: "success", title, detail }),
  warning: (title: string, detail?: string) =>
    useUiStore.getState().pushToast({ tone: "warning", title, detail }, 4600),
  danger: (title: string, detail?: string) =>
    useUiStore.getState().pushToast({ tone: "danger", title, detail }, 5600),
};

/** 替代原生 window.confirm() 的主题化异步确认框 */
export function confirmAction(options: ConfirmOptions): Promise<boolean> {
  return useUiStore.getState().confirm(options);
}
