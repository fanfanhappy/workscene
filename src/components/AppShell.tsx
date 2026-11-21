import { useEffect } from "react";
import { NavLink, useLocation, useNavigate } from "react-router-dom";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { listen } from "@tauri-apps/api/event";
import {
  Boxes,
  Camera,
  History,
  LayoutDashboard,
  Monitor,
  Moon,
  PanelLeftClose,
  PanelLeftOpen,
  RotateCcw,
  Search,
  Settings,
  Sun,
} from "lucide-react";
import { api } from "@/services/api";
import type { RestoreReport, SnapshotSummary } from "@/types/contract";
import { toast, useUiStore, type ThemeMode } from "@/stores/ui.store";
import { Button, ConfirmDialogHost, ToastHost } from "@/components/ui";
import { CommandPalette } from "@/components/CommandPalette";

const NAV = [
  { to: "/", label: "总览", icon: LayoutDashboard, end: true },
  { to: "/workspaces", label: "工作空间", icon: Boxes },
  { to: "/capture", label: "现场捕获", icon: Camera },
  { to: "/restore", label: "恢复中心", icon: RotateCcw },
  { to: "/history", label: "历史快照", icon: History },
];

export function AppShell({ children }: { children: React.ReactNode }) {
  const location = useLocation();
  const navigate = useNavigate();
  const qc = useQueryClient();

  // 监听托盘操作与后台自动快照事件：实时刷新所有相关视图缓存
  useEffect(() => {
    const unlistenRestore = listen<RestoreReport>("tray:restore_done", (e) => {
      const r = e.payload;
      qc.invalidateQueries({ queryKey: ["workspaces"], refetchType: "all" });
      qc.invalidateQueries({ queryKey: ["runs"] });
      qc.invalidateQueries({ queryKey: ["snapshots"] });
      qc.invalidateQueries({ queryKey: ["storage-stats"] });
      if (r.failed === 0) {
        toast.success(`已从托盘恢复「${r.sceneName}」`, `完成 ${r.succeeded} 项`);
      } else {
        toast.warning(
          `托盘恢复「${r.sceneName}」部分完成`,
          `成功 ${r.succeeded} · 未完成 ${r.failed + r.skipped}`,
        );
      }
    });

    const unlistenSnap = listen<SnapshotSummary>("tray:snapshot_done", (e) => {
      const s = e.payload;
      qc.invalidateQueries({ queryKey: ["snapshots"] });
      qc.invalidateQueries({ queryKey: ["storage-stats"] });
      toast.success(`托盘备份完成「${s.name}」`, `记录 ${s.windowCount} 个窗口`);
    });

    const unlistenAuto = listen<string>("snapshot:auto_created", () => {
      qc.invalidateQueries({ queryKey: ["snapshots"] });
      qc.invalidateQueries({ queryKey: ["storage-stats"] });
    });

    return () => {
      unlistenRestore.then((fn) => fn()).catch(() => undefined);
      unlistenSnap.then((fn) => fn()).catch(() => undefined);
      unlistenAuto.then((fn) => fn()).catch(() => undefined);
    };
  }, [qc]);

  const themeMode = useUiStore((s) => s.themeMode);
  const setThemeMode = useUiStore((s) => s.setThemeMode);
  const syncSystemTheme = useUiStore((s) => s.syncSystemTheme);
  const collapsed = useUiStore((s) => s.sidebarCollapsed);
  const toggleSidebar = useUiStore((s) => s.toggleSidebar);
  const setCommandPaletteOpen = useUiStore((s) => s.setCommandPaletteOpen);

  useEffect(() => {
    const mq = window.matchMedia("(prefers-color-scheme: light)");
    const listener = () => syncSystemTheme();
    mq.addEventListener("change", listener);
    return () => mq.removeEventListener("change", listener);
  }, [syncSystemTheme]);

  useEffect(() => {
    const onResize = () => {
      if (window.innerWidth < 940 && !useUiStore.getState().sidebarCollapsed) {
        useUiStore.getState().setSidebarCollapsed(true);
      }
    };
    window.addEventListener("resize", onResize);
    return () => window.removeEventListener("resize", onResize);
  }, []);

  const info = useQuery({
    queryKey: ["app-info"],
    queryFn: api.getAppInfo,
    staleTime: 60_000,
    retry: 0,
  });

  const busy = useQuery({
    queryKey: ["restore-busy"],
    queryFn: api.restoreIsBusy,
    refetchInterval: 2_500,
    retry: 0,
  });

  const allNav = [...NAV, { to: "/settings", label: "设置", icon: Settings }];
  const current = allNav.find((n) =>
    "end" in n && n.end
      ? location.pathname === n.to
      : location.pathname.startsWith(n.to),
  );

  return (
    <div className="flex h-full bg-canvas text-ink">
      {/* Apple macOS 紧凑左侧边栏 */}
      <aside
        className={`flex shrink-0 flex-col border-r border-line bg-sidebar transition-[width] duration-200 ${
          collapsed ? "w-[58px]" : "w-[188px]"
        }`}
      >
        {/* 顶部品牌栏 */}
        <div
          className={`flex h-[46px] items-center ${
            collapsed ? "justify-center px-1" : "justify-between px-3"
          }`}
        >
          <button
            type="button"
            onClick={() => navigate("/")}
            title="WorkScene"
            className="flex cursor-pointer items-center gap-2 overflow-hidden text-left"
          >
            <span className="grid size-7 shrink-0 place-items-center rounded-[8px] bg-gradient-to-b from-[#2f9bff] to-[#0071e3] text-white shadow-[0_1px_3px_rgba(0,113,227,0.35)]">
              <Boxes size={15} strokeWidth={2.2} />
            </span>
            {!collapsed ? (
              <span className="truncate text-[14px] font-semibold tracking-tight text-ink">
                WorkScene
              </span>
            ) : null}
          </button>
          {!collapsed ? (
            <button
              type="button"
              onClick={toggleSidebar}
              title="折叠侧栏"
              aria-label="折叠侧栏"
              className="grid size-6.5 cursor-pointer place-items-center rounded-[7px] text-ink-3 transition-colors hover:bg-raised/80 hover:text-ink"
            >
              <PanelLeftClose size={14} />
            </button>
          ) : null}
        </div>

        {/* 主导航 */}
        <nav className="flex flex-1 flex-col gap-0.5 px-2 pt-1" aria-label="主导航">
          {!collapsed ? (
            <p className="px-2 pb-1 pt-0.5 text-[10px] font-semibold uppercase tracking-wider text-ink-3">
              工作区
            </p>
          ) : null}
          {NAV.map(({ to, label, icon: Icon, end }) => {
            const runningDot = to === "/restore" && busy.data === true;
            return (
              <NavLink
                key={to}
                to={to}
                end={end}
                title={collapsed ? label : undefined}
                aria-label={label}
                className={({ isActive }) =>
                  `relative flex h-[32px] items-center rounded-[8px] text-[13px] transition-colors duration-150 ${
                    collapsed ? "justify-center px-0" : "gap-2.5 px-2.5"
                  } ${
                    isActive
                      ? "bg-accent/14 font-medium text-accent"
                      : "text-ink-2 hover:bg-raised/65 hover:text-ink"
                  }`
                }
              >
                {({ isActive }) => (
                  <>
                    <Icon
                      size={15}
                      strokeWidth={isActive ? 2.2 : 1.85}
                      className="shrink-0"
                    />
                    {!collapsed ? <span className="truncate">{label}</span> : null}
                    {runningDot ? (
                      <span
                        title="恢复任务执行中"
                        className={`size-2 rounded-full bg-accent ${
                          collapsed ? "absolute right-1.5 top-1.5" : "ml-auto"
                        }`}
                      />
                    ) : null}
                  </>
                )}
              </NavLink>
            );
          })}
        </nav>

        {/* 底部设置 + Apple 分段主题切换 */}
        <div className="flex flex-col gap-2 border-t border-line p-2">
          <NavLink
            to="/settings"
            title={collapsed ? "设置" : undefined}
            aria-label="设置"
            className={({ isActive }) =>
              `flex h-[32px] items-center rounded-[8px] text-[13px] transition-colors duration-150 ${
                collapsed ? "justify-center px-0" : "gap-2.5 px-2.5"
              } ${
                isActive
                  ? "bg-accent/14 font-medium text-accent"
                  : "text-ink-2 hover:bg-raised/65 hover:text-ink"
              }`
            }
          >
            <Settings size={15} strokeWidth={1.9} className="shrink-0" />
            {!collapsed ? <span className="truncate">设置</span> : null}
          </NavLink>

          {collapsed ? (
            <button
              type="button"
              onClick={toggleSidebar}
              title="展开侧栏"
              aria-label="展开侧栏"
              className="mx-auto grid size-7 cursor-pointer place-items-center rounded-[7px] text-ink-3 hover:bg-raised hover:text-ink"
            >
              <PanelLeftOpen size={14} />
            </button>
          ) : (
            <div className="flex flex-col gap-1.5 px-0.5">
              <div
                role="group"
                aria-label="切换主题"
                className="grid grid-cols-3 rounded-[8px] bg-raised/70 p-0.5"
              >
                {(
                  [
                    { id: "light", label: "浅色", icon: Sun },
                    { id: "dark", label: "深色", icon: Moon },
                    { id: "system", label: "自动", icon: Monitor },
                  ] as { id: ThemeMode; label: string; icon: typeof Sun }[]
                ).map(({ id, label, icon: Icon }) => {
                  const active = themeMode === id;
                  return (
                    <button
                      key={id}
                      type="button"
                      title={label}
                      onClick={() => setThemeMode(id)}
                      className={`flex h-6 cursor-pointer items-center justify-center gap-1 rounded-[6px] text-[11px] font-medium transition-all ${
                        active
                          ? "apple-segment-active"
                          : "text-ink-3 hover:text-ink-2"
                      }`}
                    >
                      <Icon size={11} />
                      <span>{label}</span>
                    </button>
                  );
                })}
              </div>
              <p className="px-1 font-mono text-[10px] text-ink-3">
                v{info.data?.version ?? "0.1.0"} · 本地离线优先
              </p>
            </div>
          )}
        </div>
      </aside>

      {/* 右侧主容器：macOS 毛玻璃工具条 + 紧凑内容区 */}
      <main className="flex min-w-0 flex-1 flex-col">
        <header className="flex h-[46px] shrink-0 items-center justify-between gap-3 border-b border-line bg-canvas/85 px-5 backdrop-blur-md">
          <h1 className="truncate text-[15px] font-semibold tracking-tight text-ink">
            {current?.label ?? "WorkScene"}
          </h1>

          <div className="flex items-center gap-2">
            <button
              type="button"
              onClick={() => setCommandPaletteOpen(true)}
              aria-label="打开快捷搜索"
              className="flex h-[30px] w-[210px] cursor-pointer items-center gap-2 rounded-[8px] border border-line bg-raised/55 px-2.5 text-aux text-ink-3 transition-colors hover:border-line-strong hover:text-ink-2 sm:w-[240px]"
            >
              <Search size={13} className="shrink-0" />
              <span className="flex-1 truncate text-left">聚焦搜索…</span>
              <kbd className="rounded-[5px] border border-line bg-surface px-1.5 py-0.2 font-mono text-[10px] text-ink-3">
                Ctrl K
              </kbd>
            </button>

            {location.pathname !== "/capture" ? (
              <Button size="sm" variant="primary" onClick={() => navigate("/capture")}>
                <Camera size={13} />
                <span>捕获现场</span>
              </Button>
            ) : null}
          </div>
        </header>

        <div className="min-h-0 flex-1 overflow-y-auto overflow-x-hidden px-5 py-4">
          <div key={location.pathname} className="animate-page">
            {children}
          </div>
        </div>
      </main>

      <CommandPalette />
      <ConfirmDialogHost />
      <ToastHost />
    </div>
  );
}
