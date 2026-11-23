import { useEffect, useMemo, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import {
  Boxes,
  Camera,
  CornerDownLeft,
  History,
  LayoutDashboard,
  Monitor,
  Moon,
  Play,
  RefreshCw,
  RotateCcw,
  Search,
  Settings,
  Sparkles,
  Sun,
} from "lucide-react";
import { api } from "@/services/api";
import { confirmAction, toast, useUiStore } from "@/stores/ui.store";
import { Badge } from "@/components/ui";

interface PaletteCommand {
  id: string;
  label: string;
  description: string;
  category: "恢复" | "工作空间" | "页面" | "快捷操作";
  keywords: string;
  icon: typeof Boxes;
  run: () => void;
}

export function CommandPalette() {
  const open = useUiStore((s) => s.commandPaletteOpen);
  const setOpen = useUiStore((s) => s.setCommandPaletteOpen);
  const setThemeMode = useUiStore((s) => s.setThemeMode);
  const navigate = useNavigate();
  const qc = useQueryClient();

  const [query, setQuery] = useState("");
  const [cursor, setCursor] = useState(0);
  const inputRef = useRef<HTMLInputElement>(null);
  const listRef = useRef<HTMLUListElement>(null);

  const workspaces = useQuery({
    queryKey: ["workspaces"],
    queryFn: api.workspaceList,
    staleTime: 10_000,
    enabled: open,
  });

  useEffect(() => {
    const onGlobalKey = (e: KeyboardEvent) => {
      if (!(e.ctrlKey || e.metaKey) || e.key.toLowerCase() !== "k") return;
      // 按住不放时操作系统会连续触发 keydown：忽略重复，否则面板会反复开/关
      if (e.repeat) return;
      const state = useUiStore.getState();
      // 若已有其他模态框（Dialog / 确认框 / 新手引导）打开，不叠加唤起命令面板
      if (!state.commandPaletteOpen && document.querySelector('[data-ws-modal="true"]')) {
        return;
      }
      e.preventDefault();
      // 单次按键：关闭时打开、已打开时关闭（重复按键已在上面过滤掉）
      state.setCommandPaletteOpen(!state.commandPaletteOpen);
    };
    window.addEventListener("keydown", onGlobalKey);
    return () => window.removeEventListener("keydown", onGlobalKey);
  }, []);

  useEffect(() => {
    if (open) {
      setQuery("");
      setCursor(0);
      requestAnimationFrame(() => inputRef.current?.focus());
    }
  }, [open]);

  const commands = useMemo<PaletteCommand[]>(() => {
    const go = (to: string) => {
      setOpen(false);
      if (useUiStore.getState().hasUnsavedLayout) {
        void confirmAction({
          title: "放弃未保存的窗口摆法？",
          description: "离开当前页面将丢弃尚未保存的窗口位置调整。",
          confirmLabel: "放弃并离开",
          variant: "danger",
        }).then((ok) => {
          if (ok) {
            useUiStore.getState().setHasUnsavedLayout(false);
            navigate(to);
          }
        });
        return;
      }
      navigate(to);
    };

    const wsCommands: PaletteCommand[] = [];
    for (const ws of workspaces.data ?? []) {
      wsCommands.push({
        id: `ws-restore-${ws.id}`,
        label: `一键恢复「${ws.name}」`,
        description: `${ws.itemCount} 软件 · ${ws.windowCount} 窗口`,
        category: "恢复",
        keywords: `${ws.name} restore 恢复`,
        icon: Play,
        run: () => go(`/restore?workspace=${ws.id}&auto=1`),
      });
      wsCommands.push({
        id: `ws-open-${ws.id}`,
        label: `编辑「${ws.name}」`,
        description: "打开工作空间布局画布、应用与资源配置",
        category: "工作空间",
        keywords: `${ws.name} workspace 编辑 布局`,
        icon: Boxes,
        run: () => go(`/workspaces?id=${ws.id}`),
      });
    }

    const pages: PaletteCommand[] = [
      {
        id: "p-overview",
        label: "前往总览",
        description: "当前桌面状态、最近工作空间与恢复活动",
        category: "页面",
        keywords: "overview dashboard 总览 首页",
        icon: LayoutDashboard,
        run: () => go("/"),
      },
      {
        id: "p-workspaces",
        label: "前往工作空间",
        description: "管理、收藏、导入导出与编辑工作空间",
        category: "页面",
        keywords: "workspaces 工作空间",
        icon: Boxes,
        run: () => go("/workspaces"),
      },
      {
        id: "p-capture",
        label: "捕获当前桌面",
        description: "扫描窗口并保存为工作空间",
        category: "快捷操作",
        keywords: "capture 捕获 扫描",
        icon: Camera,
        run: () => go("/capture"),
      },
      {
        id: "p-restore",
        label: "前往恢复中心",
        description: "预览环境差异、执行恢复与查看诊断报告",
        category: "页面",
        keywords: "restore 恢复中心",
        icon: RotateCcw,
        run: () => go("/restore"),
      },
      {
        id: "p-history",
        label: "前往历史快照",
        description: "时间线、快照预览、差异对比与恢复记录",
        category: "页面",
        keywords: "history snapshot 历史 快照",
        icon: History,
        run: () => go("/history"),
      },
      {
        id: "p-settings",
        label: "前往设置",
        description: "主题、自动快照、快捷键、存储与 L2 应用适配",
        category: "页面",
        keywords: "settings 设置 适配 快捷键",
        icon: Settings,
        run: () => go("/settings"),
      },
      {
        id: "t-light",
        label: "切换为浅色主题",
        description: "Apple macOS 浅色外观",
        category: "快捷操作",
        keywords: "light theme 浅色 亮色",
        icon: Sun,
        run: () => {
          setThemeMode("light");
          setOpen(false);
          toast.info("已切换为浅色主题");
        },
      },
      {
        id: "t-dark",
        label: "切换为深色主题",
        description: "Apple macOS 深色外观",
        category: "快捷操作",
        keywords: "dark theme 深色 暗色",
        icon: Moon,
        run: () => {
          setThemeMode("dark");
          setOpen(false);
          toast.info("已切换为深色主题");
        },
      },
      {
        id: "t-system",
        label: "主题跟随系统",
        description: "自动同步 Windows 深浅色设置",
        category: "快捷操作",
        keywords: "system theme 跟随 系统",
        icon: Monitor,
        run: () => {
          setThemeMode("system");
          setOpen(false);
          toast.info("主题已设为跟随系统");
        },
      },
      {
        id: "t-onboarding",
        label: "重新播放新手引导",
        description: "观看首次安装欢迎动画与 3 步上手指引",
        category: "快捷操作",
        keywords: "onboarding guide welcome 新手 引导 欢迎 动画",
        icon: Sparkles,
        run: () => {
          setOpen(false);
          useUiStore.getState().openOnboarding();
        },
      },
      {
        id: "t-refresh",
        label: "刷新本地缓存",
        description: "重新读取工作空间与桌面状态",
        category: "快捷操作",
        keywords: "refresh 刷新",
        icon: RefreshCw,
        run: () => {
          qc.invalidateQueries();
          setOpen(false);
          toast.success("已刷新缓存");
        },
      },
    ];

    return [...wsCommands, ...pages];
  }, [workspaces.data, navigate, qc, setOpen, setThemeMode]);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) {
      // 未输入关键词时：保留前 4 条工作空间与恢复命令 + 全部页面/快捷命令，避免空间多时挤掉系统命令
      const ws = commands
        .filter((c) => c.category === "工作空间" || c.category === "恢复")
        .slice(0, 6);
      const others = commands.filter(
        (c) => c.category !== "工作空间" && c.category !== "恢复",
      );
      return [...ws, ...others];
    }
    const terms = q.split(/\s+/);
    return commands.filter((cmd) => {
      const hay = `${cmd.label} ${cmd.description} ${cmd.category} ${cmd.keywords}`.toLowerCase();
      return terms.every((t) => hay.includes(t));
    });
  }, [commands, query]);

  useEffect(() => setCursor(0), [query]);

  useEffect(() => {
    const el = listRef.current?.children[cursor] as HTMLElement | undefined;
    el?.scrollIntoView({ block: "nearest" });
  }, [cursor]);

  if (!open) return null;

  const onKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Escape") {
      e.preventDefault();
      setOpen(false);
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      setCursor((c) => (filtered.length === 0 ? 0 : (c + 1) % filtered.length));
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setCursor((c) =>
        filtered.length === 0 ? 0 : (c - 1 + filtered.length) % filtered.length,
      );
    } else if (e.key === "Enter") {
      e.preventDefault();
      filtered[cursor]?.run();
    }
  };

  return (
    <div
      data-ws-modal="true"
      className="fixed inset-0 z-50 flex items-start justify-center bg-black/38 px-4 pt-[12vh] backdrop-blur-sm"
      onMouseDown={(e) => {
        if (e.target === e.currentTarget) setOpen(false);
      }}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-label="聚焦搜索"
        onKeyDown={onKeyDown}
        className="animate-dialog flex w-full max-w-[520px] flex-col overflow-hidden rounded-[14px] border border-line-strong/60 bg-surface/95 shadow-overlay backdrop-blur-xl"
      >
        <div className="flex items-center gap-2.5 border-b border-line px-4 py-3">
          <Search size={16} className="shrink-0 text-ink-3" />
          <input
            ref={inputRef}
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            placeholder="聚焦搜索工作空间、页面或快捷命令…"
            className="min-w-0 flex-1 bg-transparent text-[14px] text-ink placeholder:text-ink-3 outline-none"
          />
          <kbd className="rounded-[6px] border border-line bg-raised/70 px-1.5 py-0.5 font-mono text-[10px] text-ink-3">
            Esc
          </kbd>
        </div>

        <ul ref={listRef} className="max-h-[340px] overflow-y-auto p-1.5">
          {filtered.length === 0 ? (
            <li className="px-4 py-8 text-center text-aux text-ink-3">
              未找到匹配「{query}」的命令
            </li>
          ) : (
            filtered.map((cmd, idx) => {
              const Icon = cmd.icon;
              const active = idx === cursor;
              return (
                <li key={cmd.id}>
                  <button
                    type="button"
                    onMouseEnter={() => setCursor(idx)}
                    onClick={cmd.run}
                    className={`flex w-full cursor-pointer items-center gap-2.5 rounded-[9px] px-3 py-2 text-left transition-colors ${
                      active ? "bg-accent/14 text-ink" : "text-ink-2 hover:bg-raised/60"
                    }`}
                  >
                    <Icon
                      size={15}
                      className={`shrink-0 ${active ? "text-accent" : "text-ink-3"}`}
                    />
                    <div className="min-w-0 flex-1">
                      <p className="truncate text-body font-medium text-ink">{cmd.label}</p>
                      <p className="truncate text-[11px] text-ink-3">{cmd.description}</p>
                    </div>
                    <Badge tone={cmd.category === "恢复" ? "accent" : "neutral"}>
                      {cmd.category}
                    </Badge>
                    {active ? (
                      <CornerDownLeft size={12} className="shrink-0 text-accent" />
                    ) : null}
                  </button>
                </li>
              );
            })
          )}
        </ul>

        <div className="flex items-center justify-between border-t border-line bg-canvas/50 px-4 py-1.5 text-[11px] text-ink-3">
          <span>↑↓ 选择 · Enter 执行 · Esc 关闭</span>
          <span>Spotlight · Ctrl+K</span>
        </div>
      </div>
    </div>
  );
}
