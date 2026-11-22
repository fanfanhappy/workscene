import { useEffect, useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  Camera,
  CheckCircle2,
  Copy,
  Cpu,
  EyeOff,
  FolderOpen,
  Globe,
  HardDrive,
  Info,
  Keyboard,
  Monitor,
  Moon,
  Palette,
  Plus,
  RefreshCw,
  ShieldCheck,
  Sparkles,
  Sun,
  Terminal,
  X,
} from "lucide-react";
import { api } from "@/services/api";
import type { AppSettings } from "@/types/contract";
import { confirmAction, toast, useUiStore, type ThemeMode } from "@/stores/ui.store";
import {
  Badge,
  Button,
  Card,
  ErrorNote,
  Spinner,
  Switch,
  Tabs,
} from "@/components/ui";
import { AppIcon, useAppIcons } from "@/components/AppIcon";

type SettingsTab = "general" | "behavior" | "storage" | "adapters" | "about";

const BUILTIN_ADAPTERS = [
  {
    id: "ide",
    name: "主流 IDE / 代码编辑器",
    exes: "Code.exe · Cursor.exe · Antigravity.exe · Windsurf.exe · Trae.exe · JetBrains (IDEA / PyCharm / WebStorm 等)",
    supported: ["进程启动", "窗口定位", "自动用 IDE 打开文件夹资源"],
    limit: "不恢复未保存的编辑缓冲区。",
    icon: Cpu,
  },
  {
    id: "terminal",
    name: "Windows Terminal / Shell",
    exes: "WindowsTerminal.exe · wt.exe · pwsh.exe · powershell.exe · cmd.exe",
    supported: ["进程启动", "窗口定位", "支持 wt -d / pwsh -WorkingDirectory"],
    limit: "不恢复历史 Shell 内存态。",
    icon: Terminal,
  },
  {
    id: "browser",
    name: "主流浏览器",
    exes: "chrome.exe · msedge.exe · firefox.exe · brave.exe · arc.exe · vivaldi.exe · opera.exe",
    supported: ["最小化/正常窗口定位", "--new-window / -new-window", "本地书签夹导入"],
    limit: "不读取密码、Cookie 或私有会话库。",
    icon: Globe,
  },
  {
    id: "explorer",
    name: "Windows 资源管理器",
    exes: "explorer.exe",
    supported: ["目录路径打开", "窗口定位"],
    limit: "多标签与虚拟路径受系统外壳行为影响。",
    icon: FolderOpen,
  },
];

const ADAPTER_KIND_LABEL: Record<string, string> = {
  vscode: "L2 IDE 适配",
  jetbrains: "L2 JetBrains 适配",
  terminal: "L2 终端适配",
  browser: "L2 浏览器适配",
  explorer: "L2 资源管理器",
  generic: "L1 通用定位",
};

export function SettingsPage() {
  const qc = useQueryClient();
  const [tab, setTab] = useState<SettingsTab>("general");
  const [error, setError] = useState<string | null>(null);
  const [draft, setDraft] = useState<AppSettings | null>(null);
  const [ignoreInput, setIgnoreInput] = useState("");

  const themeMode = useUiStore((s) => s.themeMode);
  const setThemeMode = useUiStore((s) => s.setThemeMode);

  const settings = useQuery({ queryKey: ["settings"], queryFn: api.settingsGet });
  const workspaces = useQuery({
    queryKey: ["workspaces"],
    queryFn: api.workspaceList,
    staleTime: 5_000,
  });
  const wsList = workspaces.data ?? [];
  const liveCapture = useQuery({
    queryKey: ["capture"],
    queryFn: api.captureStart,
    enabled: tab === "adapters",
    staleTime: 10_000,
    retry: 0,
  });
  const stats = useQuery({ queryKey: ["storage-stats"], queryFn: api.storageStats });
  const info = useQuery({
    queryKey: ["app-info"],
    queryFn: api.getAppInfo,
    staleTime: 60_000,
  });
  const [checkingUpdate, setCheckingUpdate] = useState(false);
  const [updateInfo, setUpdateInfo] = useState<{ version: string; url: string } | null>(null);

  // 汇总用户工作空间 + 当前桌面实际用到的软件，用于真实展示适配状态与一键忽略
  const knownApps = useMemo(() => {
    const map = new Map<string, { exe: string; path: string | null }>();
    const addPath = (p: string | null | undefined, fallbackName?: string) => {
      const exe = (p?.split(/[\\/]/).pop() ?? fallbackName ?? "").trim();
      if (!exe) return;
      const key = exe.toLowerCase();
      if (!map.has(key) || (p && !map.get(key)?.path)) {
        map.set(key, { exe, path: p ?? null });
      }
    };
    for (const w of workspaces.data ?? []) {
      for (const p of w.appPaths ?? []) addPath(p);
    }
    for (const win of liveCapture.data?.windows ?? []) {
      addPath(win.processPath, win.processName);
    }
    return [...map.values()].sort((a, b) => a.exe.localeCompare(b.exe, "en"));
  }, [workspaces.data, liveCapture.data]);

  const appIcons = useAppIcons(knownApps.map((a) => a.path));
  const classifyQuery = useQuery({
    queryKey: ["adapter-classify", knownApps.map((a) => a.exe).join("|")],
    queryFn: () => api.adapterClassifyBatch(knownApps.map((a) => a.exe)),
    enabled: tab === "adapters" && knownApps.length > 0,
    staleTime: 60_000,
    retry: 0,
  });

  useEffect(() => {
    if (settings.data) setDraft(settings.data);
  }, [settings.data]);

  const save = useMutation({
    mutationFn: (next: AppSettings) => api.settingsSet(next),
    onSuccess: (saved) => {
      setError(null);
      setDraft(saved);
      qc.setQueryData(["settings"], saved);
      qc.invalidateQueries({ queryKey: ["settings"] });
      qc.invalidateQueries({ queryKey: ["capture"] });
      toast.success("设置已保存");
    },
    onError: (e) => {
      const msg = String((e as Error).message);
      setError(msg);
      toast.danger("保存失败", msg);
      // 失败时以服务端为准重新取一次：开关不能停留在"看起来开了"的状态。
      // 注意不能用缓存里的 settings.data（结构共享可能返回同一引用，draft 不会被重置）。
      void (async () => {
        try {
          const fresh = await api.settingsGet();
          qc.setQueryData(["settings"], fresh);
          setDraft(fresh);
        } catch {
          if (settings.data) setDraft(settings.data);
        }
      })();
    },
  });

  const cleanup = useMutation({
    mutationFn: api.storageCleanup,
    onSuccess: (n) => {
      qc.invalidateQueries({ queryKey: ["storage-stats"] });
      qc.invalidateQueries({ queryKey: ["snapshots"] });
      toast.success(n > 0 ? `已清理 ${n} 份旧自动快照` : "暂无可清理的自动快照");
    },
    onError: (e) => toast.danger("清理失败", String((e as Error).message)),
  });

  const patch = (next: Partial<AppSettings>) => {
    setDraft((prev) => {
      if (!prev) return prev;
      const merged = { ...prev, ...next };
      save.mutate(merged);
      return merged;
    });
  };

  const normalizeExe = (raw: string) => {
    const base = raw.split(/[\\/]/).pop()?.trim().toLowerCase() ?? "";
    if (!base) return "";
    return base.endsWith(".exe") ? base : `${base}.exe`;
  };

  const toggleIgnored = (rawExe: string) => {
    if (!draft) return;
    const key = normalizeExe(rawExe);
    if (!key) return;
    const cur = draft.ignoredProcesses ?? [];
    const exists = cur.includes(key);
    const next = exists ? cur.filter((x) => x !== key) : [...cur, key];
    patch({ ignoredProcesses: next });
  };

  const addIgnoredFromInput = (e: React.FormEvent) => {
    e.preventDefault();
    const key = normalizeExe(ignoreInput);
    if (!key || !draft) return;
    const cur = draft.ignoredProcesses ?? [];
    if (cur.includes(key)) {
      toast.info(`${key} 已在忽略名单中`);
      setIgnoreInput("");
      return;
    }
    setIgnoreInput("");
    patch({ ignoredProcesses: [...cur, key] });
  };

  const setSlot = useMutation({
    mutationFn: (v: { id: string; slot: number | null }) =>
      api.workspaceSetHotkeySlot(v.id, v.slot),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["workspaces"], refetchType: "all" });
      toast.success("快捷槽已更新");
    },
    onError: (e) => toast.danger("快捷槽设置失败", String((e as Error).message)),
  });

  if (settings.isPending || !draft) {
    return (
      <Card className="flex items-center gap-2 text-aux text-ink-3">
        <Spinner /> 读取设置…
      </Card>
    );
  }

  return (
    <div className="mx-auto flex max-w-[1040px] flex-col gap-3.5">
      {error ? <ErrorNote message={error} onDismiss={() => setError(null)} /> : null}

      <Tabs
        value={tab}
        onChange={setTab}
        items={[
          { id: "general", label: "外观与热键", icon: <Palette size={13} /> },
          { id: "behavior", label: "快照与恢复", icon: <Camera size={13} /> },
          { id: "storage", label: "存储与目录", icon: <HardDrive size={13} /> },
          { id: "adapters", label: "应用适配 (L2)", icon: <Cpu size={13} /> },
          { id: "about", label: "关于", icon: <Info size={13} /> },
        ]}
      />

      {/* Tab 1: 外观、开机启动与热键 */}
      {tab === "general" ? (
        <div className="flex flex-col gap-3.5">
          <Card className="flex flex-col gap-3">
            <p className="text-body font-medium text-ink">主题外观</p>
            <div className="grid grid-cols-3 gap-2">
              {(
                [
                  { id: "dark", label: "深色主题", desc: "低饱和深石墨色", icon: Moon },
                  { id: "light", label: "浅色主题", desc: "明亮办公环境", icon: Sun },
                  { id: "system", label: "跟随系统", desc: "同步 Windows 设置", icon: Monitor },
                ] as { id: ThemeMode; label: string; desc: string; icon: typeof Moon }[]
              ).map(({ id, label, desc, icon: Icon }) => {
                const selected = themeMode === id;
                return (
                  <button
                    key={id}
                    type="button"
                    onClick={() => {
                      setThemeMode(id);
                      toast.info(`已切换为${label}`);
                    }}
                    className={`flex cursor-pointer flex-col gap-1 rounded-[10px] border p-2.5 text-left transition-colors ${
                      selected
                        ? "border-accent bg-accent/10"
                        : "border-line bg-canvas/50 hover:border-line-strong"
                    }`}
                  >
                    <div className="flex items-center justify-between">
                      <Icon size={14} className={selected ? "text-accent" : "text-ink-3"} />
                      {selected ? <Badge tone="accent">当前</Badge> : null}
                    </div>
                    <p className="text-body font-medium text-ink">{label}</p>
                    <p className="text-[11px] text-ink-3">{desc}</p>
                  </button>
                );
              })}
            </div>
          </Card>

          <Card className="flex flex-col gap-3.5">
            <Row
              label="随 Windows 开机启动"
              hint="仅对当前用户生效（无需管理员）；若被 Windows 安全中心拦截，可点右侧打开启动文件夹手动放入快捷方式"
            >
              <div className="flex items-center gap-2">
                <Button
                  size="xs"
                  variant={error?.includes("开机启动") ? "secondary" : "ghost"}
                  onClick={() =>
                    api
                      .systemOpenStartupDir()
                      .catch((e) => toast.danger("打开启动文件夹失败", String((e as Error).message)))
                  }
                  title="打开 Windows 当前用户启动文件夹"
                >
                  <FolderOpen size={12} /> 启动文件夹
                </Button>
                <Switch
                  checked={draft.autostart}
                  onChange={(v) => patch({ autostart: v })}
                  ariaLabel="开机启动"
                />
              </div>
            </Row>

            <Row
              label="全局唤起主窗口热键"
              hint="修改后立即生效；若被其他程序占用会提示冲突"
            >
              <div className="flex items-center gap-2">
                <Keyboard size={14} className="text-ink-3" />
                <select
                  value={draft.hotkey}
                  onChange={(e) => patch({ hotkey: e.target.value })}
                  className="h-8 rounded-[8px] border border-line-strong/80 bg-canvas px-2 text-aux text-ink outline-none focus:border-accent"
                >
                  <option value="Ctrl+Alt+W">Ctrl + Alt + W</option>
                  <option value="Ctrl+Shift+W">Ctrl + Shift + W</option>
                  <option value="Ctrl+Alt+Space">Ctrl + Alt + Space</option>
                  <option value="">关闭</option>
                </select>
              </div>
            </Row>
            <Row
              label="工作空间快捷槽（Ctrl + Alt + 1 ~ 9）"
              hint="按下后直接恢复该槽绑定的工作空间；下方可直接为每个数字键绑定固定空间"
            >
              <Switch
                checked={draft.workspaceHotkeysEnabled}
                onChange={(v) => patch({ workspaceHotkeysEnabled: v })}
                ariaLabel="工作空间全局切屏热键"
              />
            </Row>
          </Card>

          <Card
            className={`flex flex-col gap-3 transition-opacity ${
              draft.workspaceHotkeysEnabled ? "" : "opacity-60"
            }`}
          >
            <div className="flex flex-wrap items-center justify-between gap-2">
              <div>
                <p className="flex items-center gap-1.5 text-body font-semibold text-ink">
                  <Keyboard size={14} className="text-ink-3" /> 工作空间快捷槽绑定
                </p>
                <p className="mt-0.5 text-[11px] text-ink-3">
                  {draft.workspaceHotkeysEnabled
                    ? "Ctrl+Alt+1..9 直接恢复对应工作空间；绑定跟着工作空间走，删除别的空间不会串号"
                    : "上方「工作空间快捷槽」总开关已关闭，开启后以下快捷键即可生效"}
                </p>
              </div>
              <Badge tone={draft.workspaceHotkeysEnabled ? "accent" : "neutral"}>
                {draft.workspaceHotkeysEnabled
                  ? `${wsList.filter((w) => w.hotkeySlot).length} 个已绑定`
                  : "已关闭"}
              </Badge>
            </div>

            <div className="grid grid-cols-1 gap-1.5 sm:grid-cols-3">
              {Array.from({ length: 9 }, (_, i) => i + 1).map((slot) => (
                <div
                  key={slot}
                  className="flex items-center gap-2 rounded-[8px] border border-line/70 bg-canvas/60 px-2 py-1.5"
                >
                  <kbd className="shrink-0 rounded-[5px] border border-line-strong bg-surface px-1.5 py-0.5 font-mono text-[10px] text-ink-2">
                    Ctrl+Alt+{slot}
                  </kbd>
                  <select
                    value={wsList.find((w) => w.hotkeySlot === slot)?.id ?? ""}
                    disabled={!draft.workspaceHotkeysEnabled || setSlot.isPending}
                    onChange={(e) => {
                      const nextId = e.target.value;
                      const current = wsList.find((w) => w.hotkeySlot === slot);
                      if (nextId) {
                        setSlot.mutate({ id: nextId, slot });
                      } else if (current) {
                        setSlot.mutate({ id: current.id, slot: null });
                      }
                    }}
                    className="h-7 min-w-0 flex-1 rounded-[6px] border border-line-strong/70 bg-canvas px-1.5 text-[11px] text-ink-2 outline-none focus:border-accent disabled:cursor-not-allowed"
                  >
                    <option value="">未绑定</option>
                    {wsList.map((w) => (
                      <option key={w.id} value={w.id}>
                        {w.name}
                      </option>
                    ))}
                  </select>
                </div>
              ))}
            </div>
          </Card>

          <Card className="flex flex-col gap-2">
            <p className="text-body font-medium text-ink">应用内快捷键</p>
            <div className="grid gap-1.5 text-aux sm:grid-cols-2">
              {[
                ["Ctrl + Alt + 1~9", "恢复该槽绑定的工作空间"],
                ["Ctrl + K", "打开快捷命令面板"],
                ["Ctrl + S", "保存布局画布修改"],
                ["方向键 / Shift+方向键", "布局画布微调窗口位置 (1% / 5%)"],
                ["Ctrl + Z / Ctrl + Y", "布局画布撤销 / 重做"],
                ["Esc", "关闭弹窗或取消当前选中"],
              ].map(([k, desc]) => (
                <div
                  key={k}
                  className="flex items-center justify-between gap-2 rounded-[8px] border border-line/70 bg-canvas/45 px-2.5 py-1.5"
                >
                  <span className="text-ink-2">{desc}</span>
                  <kbd className="shrink-0 rounded-[5px] border border-line bg-surface px-1.5 py-0.5 font-mono text-[10px] text-ink">
                    {k}
                  </kbd>
                </div>
              ))}
            </div>
          </Card>
        </div>
      ) : null}

      {/* Tab 2: 自动快照与恢复行为 */}
      {tab === "behavior" ? (
        <div className="flex flex-col gap-3.5">
          <Card className="flex flex-col gap-3.5">
            <p className="text-body font-medium text-ink">自动快照策略</p>
            <Row
              label="启用后台自动快照"
              hint="按桌面指纹去重与节流；手动快照与恢复前保护快照不受此开关影响"
            >
              <Switch
                checked={draft.autoSnapshotEnabled}
                onChange={(v) => patch({ autoSnapshotEnabled: v })}
                ariaLabel="启用自动快照"
              />
            </Row>
            <Row label="每空间保留份数" hint="仅淘汰最旧自动快照，手动与保护快照永不删除">
              <NumberInput
                value={draft.autoKeep}
                min={1}
                max={500}
                suffix="份"
                onCommit={(v) => patch({ autoKeep: v })}
              />
            </Row>
            <Row label="最小采样间隔" hint="避免拖动窗口时连续落盘">
              <NumberInput
                value={draft.autoMinIntervalSecs}
                min={10}
                max={86400}
                suffix="秒"
                onCommit={(v) => patch({ autoMinIntervalSecs: v })}
              />
            </Row>
          </Card>

          <Card className="flex flex-col gap-3">
            <p className="text-body font-medium text-ink">恢复默认行为</p>
            <Row
              label="默认自动启动未运行的应用"
              hint="恢复预览页的默认勾选状态，每次恢复前仍可临时切换"
            >
              <Switch
                checked={draft.restoreRelaunchDefault}
                onChange={(v) => patch({ restoreRelaunchDefault: v })}
                ariaLabel="默认自动启动未运行的应用"
              />
            </Row>
            <Row
              label="默认开启专注模式（自动最小化无关窗口）"
              hint="切换工作空间时自动把非本空间的窗口收进任务栏，绝不关闭软件"
            >
              <Switch
                checked={draft.restoreFocusModeDefault}
                onChange={(v) => patch({ restoreFocusModeDefault: v })}
                ariaLabel="默认开启专注模式"
              />
            </Row>
            <Row
              label="启动 WorkScene 时自动恢复工作空间"
              hint="注意：启动后会自动打开该空间的软件并摆好窗口；搭配「开机启动」时，登录 Windows 就会开始恢复"
            >
              <select
                value={draft.startupWorkspaceId ?? ""}
                onChange={(e) =>
                  patch({ startupWorkspaceId: e.target.value ? e.target.value : null })
                }
                className="h-8 max-w-[200px] rounded-[8px] border border-line-strong/80 bg-canvas px-2 text-aux text-ink outline-none focus:border-accent"
              >
                <option value="">不自动恢复</option>
                {(workspaces.data ?? []).map((w) => (
                  <option key={w.id} value={w.id}>
                    {w.isFavorite ? "★ " : ""}
                    {w.name}
                  </option>
                ))}
              </select>
            </Row>
            <div className="flex items-start gap-2 rounded-[10px] border border-line bg-canvas/55 px-3 py-2 text-aux text-ink-3">
              <ShieldCheck size={14} className="mt-0.5 shrink-0 text-success" />
              恢复前始终自动生成保护快照；绝不静默关闭已打开应用；窗口匹配歧义时宁可跳过不误移。
            </div>
          </Card>
        </div>
      ) : null}

      {/* Tab 3: 存储与目录 */}
      {tab === "storage" ? (
        <Card className="flex flex-col gap-4">
          <div className="grid grid-cols-2 gap-2.5 sm:grid-cols-4">
            <Stat label="数据库" value={formatBytes(stats.data?.dbBytes ?? 0)} />
            <Stat label="快照文件" value={`${stats.data?.snapshotCount ?? 0} 个`} />
            <Stat label="快照占用" value={formatBytes(stats.data?.snapshotBytes ?? 0)} />
            <Stat label="恢复报告" value={`${stats.data?.runCount ?? 0} 条`} />
          </div>

          <div className="flex flex-col gap-1.5 border-t border-line pt-3">
            <span className="text-aux text-ink-3">本机数据目录</span>
            <div className="flex flex-wrap items-center gap-2">
              <code className="selectable min-w-0 flex-1 truncate rounded-[8px] border border-line bg-canvas px-2.5 py-1.5 font-mono text-[11px] text-ink-2">
                {stats.data?.dataDir ?? info.data?.dataDir ?? "—"}
              </code>
              <Button
                size="sm"
                variant="secondary"
                onClick={() =>
                  api
                    .systemOpenDataDir()
                    .catch((e) => toast.danger("打开目录失败", String((e as Error).message)))
                }
              >
                <FolderOpen size={13} /> 打开目录
              </Button>
              <Button
                size="sm"
                variant="ghost"
                onClick={async () => {
                  const dir = stats.data?.dataDir ?? info.data?.dataDir ?? "";
                  if (!dir) return;
                  await navigator.clipboard.writeText(dir).catch(() => undefined);
                  toast.success("已复制目录路径");
                }}
              >
                <Copy size={13} /> 复制
              </Button>
            </div>
          </div>

          <div className="flex flex-wrap items-center justify-between gap-3 border-t border-line pt-3">
            <div>
              <p className="text-body font-medium text-ink">全量工作空间备份与迁移</p>
              <p className="text-aux text-ink-3">
                一键导出全部工作空间为 JSON 文件，换电脑或重装系统后可一键批量恢复
              </p>
            </div>
            <div className="flex items-center gap-2">
              <Button
                size="sm"
                variant="secondary"
                onClick={async () => {
                  try {
                    const json = await api.workspaceExportAll();
                    const blob = new Blob([json], { type: "application/json;charset=utf-8" });
                    const url = URL.createObjectURL(blob);
                    const a = document.createElement("a");
                    const date = new Date().toISOString().slice(0, 10);
                    a.href = url;
                    a.download = `WorkScene-backup-${date}.json`;
                    document.body.appendChild(a);
                    a.click();
                    a.remove();
                    window.setTimeout(() => URL.revokeObjectURL(url), 1000);
                    toast.success("全量备份已导出", `已下载 WorkScene-backup-${date}.json`);
                  } catch (e) {
                    toast.danger("导出备份失败", String((e as Error).message));
                  }
                }}
              >
                导出全部备份
              </Button>
              <label className="inline-flex h-8 cursor-pointer items-center gap-1.5 rounded-[8px] border border-line bg-surface px-3 text-aux font-medium text-ink transition-colors hover:bg-raised">
                导入备份
                <input
                  type="file"
                  accept=".json,application/json"
                  className="hidden"
                  onChange={async (e) => {
                    const file = e.target.files?.[0];
                    e.target.value = "";
                    if (!file) return;
                    try {
                      const text = await file.text();
                      const ok = await confirmAction({
                        title: `导入全量备份「${file.name}」？`,
                        description:
                          "将批量新增备份中的工作空间（不会覆盖或删除你现有的空间）。外部导入的生命周期钩子与启动参数会出于安全默认停用/剥离。",
                        confirmLabel: "确认导入",
                      });
                      if (!ok) return;
                      const count = await api.workspaceImportAll(text);
                      qc.invalidateQueries({ queryKey: ["workspaces"], refetchType: "all" });
                      qc.invalidateQueries({ queryKey: ["storage-stats"] });
                      toast.success(`已批量导入 ${count} 个工作空间`);
                    } catch (err) {
                      toast.danger("导入备份失败", String((err as Error).message));
                    }
                  }}
                />
              </label>
            </div>
          </div>

          <div className="flex items-center justify-between gap-3 border-t border-line pt-3">
            <div>
              <p className="text-body font-medium text-ink">按保留策略清理旧自动快照</p>
              <p className="text-aux text-ink-3">
                每个工作空间保留最近 {draft.autoKeep} 份自动快照
              </p>
            </div>
            <Button
              size="sm"
              variant="secondary"
              disabled={cleanup.isPending}
              onClick={async () => {
                const ok = await confirmAction({
                  title: "清理全部工作空间的旧自动快照？",
                  description: `每空间保留最近 ${draft.autoKeep} 份自动快照，手动与保护快照不动。`,
                  confirmLabel: "开始清理",
                });
                if (ok) cleanup.mutate();
              }}
            >
              <Sparkles size={13} /> 立即清理
            </Button>
          </div>
        </Card>
      ) : null}

      {/* Tab 4: 内置 L2 应用适配（不再单独占一个一级导航） */}
      {tab === "adapters" ? (
        <div className="flex flex-col gap-3.5">
          <div className="grid gap-2.5 md:grid-cols-2">
            {BUILTIN_ADAPTERS.map((ad) => {
              const Icon = ad.icon;
              return (
                <Card key={ad.id} className="flex flex-col gap-2">
                  <div className="flex items-center justify-between gap-2">
                    <span className="flex items-center gap-2 text-body font-semibold text-ink">
                      <Icon size={14} className="text-accent" /> {ad.name}
                    </span>
                    <Badge tone="accent">L2 参数适配</Badge>
                  </div>
                  <p className="font-mono text-[11px] text-ink-3">{ad.exes}</p>
                  <div className="flex flex-wrap gap-1">
                    {ad.supported.map((s) => (
                      <Badge key={s} tone="success">
                        <CheckCircle2 size={10} /> {s}
                      </Badge>
                    ))}
                  </div>
                  <p className="text-[11px] text-ink-3">边界：{ad.limit}</p>
                </Card>
              );
            })}
          </div>

          <Card className="flex flex-col gap-3">
            <div className="flex flex-wrap items-center justify-between gap-2">
              <div>
                <p className="flex items-center gap-1.5 text-body font-semibold text-ink">
                  <EyeOff size={14} className="text-ink-3" /> 全局捕获忽略名单
                </p>
                <p className="mt-0.5 text-[11px] text-ink-3">
                  加入此名单的软件在扫描桌面、自动备份时会被自动跳过（不会关闭软件，随时可移出）
                </p>
              </div>
              <Badge>{(draft.ignoredProcesses ?? []).length} 项</Badge>
            </div>

            <form onSubmit={addIgnoredFromInput} className="flex items-center gap-2">
              <input
                value={ignoreInput}
                onChange={(e) => setIgnoreInput(e.target.value)}
                placeholder="输入要忽略的进程名并回车，例如 WeChat.exe 或 FlClash"
                className="h-8 flex-1 rounded-[8px] border border-line-strong/80 bg-canvas px-2.5 font-mono text-[11px] text-ink placeholder:text-ink-3 outline-none focus:border-accent"
              />
              <Button
                type="submit"
                size="sm"
                variant="secondary"
                disabled={save.isPending || !ignoreInput.trim()}
              >
                <Plus size={13} /> 添加忽略
              </Button>
            </form>

            {(draft.ignoredProcesses ?? []).length === 0 ? (
              <p className="rounded-[8px] border border-dashed border-line py-3 text-center text-[11px] text-ink-3">
                当前没有忽略任何软件；也可以在下方「我常用软件的适配状态」里一键加入忽略。
              </p>
            ) : (
              <div className="flex flex-wrap gap-1.5">
                {(draft.ignoredProcesses ?? []).map((proc) => (
                  <span
                    key={proc}
                    className="inline-flex items-center gap-1.5 rounded-full border border-line bg-canvas/80 px-2.5 py-1 font-mono text-[11px] text-ink-2"
                  >
                    {proc}
                    <button
                      type="button"
                      title={`移出忽略：${proc}`}
                      onClick={() => toggleIgnored(proc)}
                      className="cursor-pointer rounded-full p-0.5 text-ink-3 hover:bg-raised hover:text-danger"
                    >
                      <X size={11} />
                    </button>
                  </span>
                ))}
              </div>
            )}
          </Card>

          <Card padded={false} className="overflow-hidden">
            <div className="flex items-center justify-between border-b border-line px-4 py-2.5">
              <div>
                <p className="text-body font-semibold text-ink">我常用软件的适配状态</p>
                <p className="text-[11px] text-ink-3">
                  来自当前桌面与已有工作空间；所有软件均默认支持 L1 启动与窗口定位
                </p>
              </div>
              <Button
                size="xs"
                variant="ghost"
                disabled={liveCapture.isFetching}
                onClick={() => liveCapture.refetch()}
              >
                <RefreshCw
                  size={12}
                  className={liveCapture.isFetching ? "animate-spin" : ""}
                />
                刷新
              </Button>
            </div>

            {knownApps.length === 0 ? (
              <p className="px-4 py-6 text-center text-aux text-ink-3">
                暂未检测到已捕获软件，捕获一次桌面后这里会自动展示。
              </p>
            ) : (
              <ul className="divide-y divide-line">
                {knownApps.map((app) => {
                  const kind = classifyQuery.data?.[app.exe] ?? "generic";
                  const isL2 = kind !== "generic";
                  const key = normalizeExe(app.exe);
                  const ignored = (draft.ignoredProcesses ?? []).includes(key);
                  return (
                    <li
                      key={key}
                      className="flex items-center gap-2.5 px-4 py-2"
                    >
                      <AppIcon path={app.path} name={app.exe} icons={appIcons} size={18} />
                      <div className="min-w-0 flex-1">
                        <p className="truncate text-aux font-medium text-ink">
                          {app.exe.replace(/\.exe$/i, "")}
                        </p>
                        <p className="truncate font-mono text-[10px] text-ink-3">
                          {app.path ?? app.exe}
                        </p>
                      </div>
                      <Badge tone={isL2 ? "accent" : "neutral"}>
                        {ADAPTER_KIND_LABEL[kind] ?? "L1 通用定位"}
                      </Badge>
                      <Button
                        size="xs"
                        variant={ignored ? "tinted" : "ghost"}
                        onClick={() => toggleIgnored(app.exe)}
                      >
                        <EyeOff size={11} />
                        {ignored ? "已忽略" : "忽略捕获"}
                      </Button>
                    </li>
                  );
                })}
              </ul>
            )}
          </Card>
        </div>
      ) : null}

      {/* Tab 5: 关于 */}
      {tab === "about" ? (
        <Card className="flex flex-col gap-3">
          <div className="flex flex-wrap items-center gap-2">
            <span className="text-card font-semibold text-ink">
              WorkScene v{info.data?.version ?? "1.1.2"}
            </span>
            <Badge tone="accent">Tauri {info.data?.tauriVersion ?? "2"}</Badge>
            <Badge>{info.data?.dpiAwareness ?? "Per-Monitor V2"}</Badge>
          </div>
          <ul className="flex flex-col gap-1 text-aux text-ink-3">
            <li>· 本地优先、离线可用：所有数据保存在本机，不上传任何桌面信息。</li>
            <li>· 权限边界：受 Windows UIPI 限制，普通权限下无法移动管理员提权窗口。</li>
            <li>· 能力边界：不支持 UWP 打包应用窗口恢复；不读取浏览器密码/Cookie 或应用内存缓冲区。</li>
          </ul>
          <div className="flex flex-wrap items-center gap-2">
            <Button
              size="sm"
              variant="secondary"
              disabled={checkingUpdate}
              onClick={async () => {
                setCheckingUpdate(true);
                try {
                  const res = await fetch(
                    "https://api.github.com/repos/fanfanhappy/workscene/releases/latest",
                  );
                  if (!res.ok) throw new Error(`HTTP ${res.status}`);
                  const data = (await res.json()) as { tag_name?: string; html_url?: string };
                  const latest = (data.tag_name ?? "").replace(/^v/i, "");
                  const current = (info.data?.version ?? "1.1.1").replace(/^v/i, "");
                  const toParts = (v: string) =>
                    v.split(".").map((n) => Number.parseInt(n, 10) || 0);
                  const a = toParts(latest);
                  const b = toParts(current);
                  let cmp = 0;
                  for (let i = 0; i < Math.max(a.length, b.length); i++) {
                    if ((a[i] ?? 0) !== (b[i] ?? 0)) {
                      cmp = (a[i] ?? 0) - (b[i] ?? 0);
                      break;
                    }
                  }
                  if (!latest || cmp <= 0) {
                    setUpdateInfo(null);
                    toast.success("已是最新版本", `当前版本 v${current}`);
                  } else {
                    const url =
                      data.html_url || "https://github.com/fanfanhappy/workscene/releases/latest";
                    setUpdateInfo({ version: latest, url });
                    toast.info(`发现新版本 v${latest}`, "点击下方链接可前往 GitHub 下载");
                  }
                } catch {
                  toast.warning(
                    "检查更新失败",
                    "请检查网络或直接访问 github.com/fanfanhappy/workscene/releases",
                  );
                } finally {
                  setCheckingUpdate(false);
                }
              }}
            >
              <RefreshCw size={13} className={checkingUpdate ? "animate-spin" : ""} />
              {checkingUpdate ? "检查中…" : "检查新版本"}
            </Button>
            {updateInfo ? (
              <a
                href={updateInfo.url}
                target="_blank"
                rel="noreferrer"
                className="inline-flex items-center gap-1 rounded-[7px] border border-accent/40 bg-accent/10 px-2.5 py-1 text-aux font-medium text-accent hover:bg-accent/15"
              >
                下载 v{updateInfo.version} ↗
              </a>
            ) : null}
            <Button
              size="sm"
              variant="secondary"
              onClick={() => useUiStore.getState().openOnboarding()}
            >
              <Sparkles size={13} /> 重新查看新手引导
            </Button>
            <Button
              size="sm"
              variant="ghost"
              onClick={() => {
                qc.invalidateQueries();
                toast.success("已刷新缓存");
              }}
            >
              <RefreshCw size={13} /> 刷新本地缓存
            </Button>
          </div>
        </Card>
      ) : null}
    </div>
  );
}

function Row({
  label,
  hint,
  children,
}: {
  label: string;
  hint?: string;
  children: React.ReactNode;
}) {
  return (
    <div className="flex items-start justify-between gap-6 border-t border-line/60 pt-3 first:border-0 first:pt-0">
      <div className="min-w-0">
        <p className="text-body font-medium text-ink">{label}</p>
        {hint ? <p className="mt-0.5 text-[11px] text-ink-3">{hint}</p> : null}
      </div>
      <div className="shrink-0">{children}</div>
    </div>
  );
}

function NumberInput({
  value,
  min,
  max,
  suffix,
  onCommit,
}: {
  value: number;
  min: number;
  max: number;
  suffix: string;
  onCommit: (v: number) => void;
}) {
  const [text, setText] = useState(String(value));
  useEffect(() => setText(String(value)), [value]);

  const commit = () => {
    const n = Number(text);
    if (!Number.isFinite(n)) {
      setText(String(value));
      return;
    }
    const next = Math.min(Math.max(Math.round(n), min), max);
    setText(String(next));
    if (next !== value) onCommit(next);
  };

  return (
    <span className="flex items-center gap-1.5">
      <input
        value={text}
        onChange={(e) => setText(e.target.value)}
        onBlur={commit}
        onKeyDown={(e) => {
          if (e.key === "Enter") e.currentTarget.blur();
        }}
        inputMode="numeric"
        className="tabnum h-8 w-18 rounded-[8px] border border-line-strong/80 bg-canvas px-2 text-right font-mono text-body text-ink outline-none focus:border-accent"
      />
      <span className="text-aux text-ink-3">{suffix}</span>
    </span>
  );
}

function Stat({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-[9px] border border-line bg-canvas/55 px-3 py-2">
      <p className="text-[11px] text-ink-3">{label}</p>
      <p className="tabnum mt-0.5 text-card font-semibold text-ink">{value}</p>
    </div>
  );
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}
