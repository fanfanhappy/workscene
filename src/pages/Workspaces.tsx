import { useEffect, useMemo, useRef, useState } from "react";
import { useNavigate, useSearchParams } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  AppWindow,
  ArrowLeft,
  Bookmark,
  Camera,
  ChevronDown,
  Copy,
  Cpu,
  Download,
  FilePlus2,
  FileText,
  FolderOpen,
  FolderPlus,
  Globe,
  History,
  LayoutGrid,
  List,
  MonitorSmartphone,
  MoreHorizontal,
  Pencil,
  Play,
  Plus,
  Power,
  RefreshCw,
  Rows3,
  SlidersHorizontal,
  Sparkles,
  Star,
  Trash2,
  Upload,
} from "lucide-react";
import { api, formatRelative } from "@/services/api";
import type {
  WorkspaceItem,
  WorkspaceSummary,
  WorkspaceWindow,
} from "@/types/contract";
import { confirmAction, toast } from "@/stores/ui.store";
import { LayoutEditor, toLayoutUpdates } from "@/components/LayoutEditor";
import {
  AppIcon,
  AppIconStrip,
  useAppIcons,
  WORKSPACE_COLORS,
  WorkspaceColorDot,
} from "@/components/AppIcon";
import {
  Badge,
  Button,
  Card,
  Dialog,
  EmptyState,
  ErrorNote,
  SearchInput,
  Spinner,
  Switch,
  Tabs,
} from "@/components/ui";

type ViewMode = "grid" | "list" | "compact";
type SortKey = "updated" | "restored" | "name" | "windows";
type EditorTab = "layout" | "apps" | "resources";

function SuggestArgs({
  exeName,
  target,
  current,
  onApply,
}: {
  exeName: string;
  target: string | null;
  current: string | null;
  onApply: (args: string) => void;
}) {
  const q = useQuery({
    queryKey: ["adapter", exeName, target],
    queryFn: () => api.adapterSuggest(exeName, target),
    staleTime: Infinity,
    retry: 0,
    enabled: Boolean(exeName),
  });
  const suggestion = q.data?.args ?? null;
  if (!suggestion || suggestion === (current ?? "")) return null;
  return (
    <Button
      size="xs"
      variant="tinted"
      onClick={() => onApply(suggestion)}
      title={`填入推荐参数：${suggestion}`}
    >
      <Sparkles size={11} /> 使用推荐参数
    </Button>
  );
}

export function WorkspacesPage() {
  const navigate = useNavigate();
  const qc = useQueryClient();
  const [params, setParams] = useSearchParams();
  const activeId = params.get("id");
  const importInputRef = useRef<HTMLInputElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);

  const [viewMode, setViewMode] = useState<ViewMode>("grid");
  const [sortBy, setSortBy] = useState<SortKey>("updated");
  const [onlyFavorites, setOnlyFavorites] = useState(false);
  const [search, setSearch] = useState("");
  const [tab, setTab] = useState<EditorTab>("layout");
  const [menuOpen, setMenuOpen] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const [metaOpen, setMetaOpen] = useState(false);
  const [metaName, setMetaName] = useState("");
  const [metaDesc, setMetaDesc] = useState("");
  const [metaAutoSnap, setMetaAutoSnap] = useState(true);
  const [metaColor, setMetaColor] = useState<string | null>(null);

  const [dupOpen, setDupOpen] = useState(false);
  const [dupName, setDupName] = useState("");

  useEffect(() => {
    if (!menuOpen) return;
    const onDoc = (e: MouseEvent) => {
      if (!menuRef.current?.contains(e.target as Node)) setMenuOpen(false);
    };
    window.addEventListener("mousedown", onDoc);
    return () => window.removeEventListener("mousedown", onDoc);
  }, [menuOpen]);

  const list = useQuery({
    queryKey: ["workspaces"],
    queryFn: api.workspaceList,
    staleTime: 2_000,
  });

  const detail = useQuery({
    queryKey: ["workspace", activeId],
    queryFn: () => api.workspaceGet(activeId!),
    enabled: Boolean(activeId),
    retry: 0,
  });

  const displays = useQuery({
    queryKey: ["displays"],
    queryFn: api.getDisplays,
    staleTime: 30_000,
    retry: 0,
  });

  const allIconPaths = useMemo(() => {
    const out: (string | null | undefined)[] = [];
    for (const w of list.data ?? []) {
      for (const p of w.appPaths ?? []) out.push(p);
    }
    for (const it of detail.data?.items ?? []) {
      if (it.itemType === "application") out.push(it.target);
    }
    return out;
  }, [list.data, detail.data]);
  const appIcons = useAppIcons(allIconPaths);

  const [draft, setDraft] = useState<WorkspaceWindow[]>([]);
  useEffect(() => {
    if (detail.data) setDraft(detail.data.windows);
  }, [detail.data]);

  const layoutDirty = useMemo(() => {
    if (!detail.data) return false;
    const orig = detail.data.windows;
    if (orig.length !== draft.length) return true;
    return draft.some((d) => {
      const o = orig.find((x) => x.id === d.id);
      if (!o) return true;
      return (
        Math.abs(o.xRatio - d.xRatio) > 0.0001 ||
        Math.abs(o.yRatio - d.yRatio) > 0.0001 ||
        Math.abs(o.widthRatio - d.widthRatio) > 0.0001 ||
        Math.abs(o.heightRatio - d.heightRatio) > 0.0001 ||
        o.monitorKey !== d.monitorKey ||
        o.showState !== d.showState
      );
    });
  }, [detail.data, draft]);

  const filteredList = useMemo(() => {
    const q = search.trim().toLowerCase();
    return (list.data ?? [])
      .filter((w) => {
        if (onlyFavorites && !w.isFavorite) return false;
        if (!q) return true;
        return (
          w.name.toLowerCase().includes(q) ||
          (w.description ?? "").toLowerCase().includes(q)
        );
      })
      .sort((a, b) => {
        if (a.isFavorite !== b.isFavorite) return a.isFavorite ? -1 : 1;
        if (sortBy === "name") return a.name.localeCompare(b.name, "zh-CN");
        if (sortBy === "windows") return b.windowCount - a.windowCount;
        if (sortBy === "restored") return (b.lastRestoredAt ?? 0) - (a.lastRestoredAt ?? 0);
        return b.updatedAt - a.updatedAt;
      });
  }, [list.data, search, sortBy, onlyFavorites]);

  const saveLayout = useMutation({
    mutationFn: async () => {
      const ws = detail.data!;
      let changes = 0;
      const removed = ws.windows.filter((w) => !draft.some((d) => d.id === w.id));
      for (const w of removed) {
        await api.workspaceWindowDelete(ws.id, w.id);
        changes += 1;
      }
      const modified = draft.filter((d) => {
        const o = ws.windows.find((w) => w.id === d.id);
        return (
          o &&
          (Math.abs(o.xRatio - d.xRatio) > 0.0001 ||
            Math.abs(o.yRatio - d.yRatio) > 0.0001 ||
            Math.abs(o.widthRatio - d.widthRatio) > 0.0001 ||
            Math.abs(o.heightRatio - d.heightRatio) > 0.0001 ||
            o.monitorKey !== d.monitorKey ||
            o.showState !== d.showState)
        );
      });
      if (modified.length > 0) {
        await api.workspaceLayoutUpdate(ws.id, toLayoutUpdates(modified));
        changes += modified.length;
      }
      return changes;
    },
    onSuccess: (changes) => {
      qc.invalidateQueries({ queryKey: ["workspace", activeId] });
      qc.invalidateQueries({ queryKey: ["workspaces"] });
      toast.success("窗口摆法已保存", changes > 0 ? `已保存 ${changes} 处修改` : undefined);
    },
    onError: (e) => setError(String((e as Error).message)),
  });

  const toggleFavorite = useMutation({
    mutationFn: ({ id, isFavorite }: { id: string; isFavorite: boolean }) =>
      api.workspaceSetFavorite(id, isFavorite),
    onSuccess: (_, vars) => {
      qc.setQueryData<WorkspaceSummary[]>(["workspaces"], (prev) =>
        prev?.map((w) => (w.id === vars.id ? { ...w, isFavorite: vars.isFavorite } : w)),
      );
      qc.invalidateQueries({ queryKey: ["workspaces"], refetchType: "all" });
      qc.invalidateQueries({ queryKey: ["workspace", vars.id] });
      toast.info(vars.isFavorite ? "已收藏置顶（托盘菜单也会同步置顶）" : "已取消收藏");
    },
    onError: (e) => {
      const msg = String((e as Error).message);
      setError(msg);
      toast.danger("收藏操作失败", msg);
    },
  });

  const updateMeta = useMutation({
    mutationFn: () =>
      api.workspaceUpdateMeta(
        detail.data!.id,
        metaName.trim(),
        metaDesc.trim() || null,
        metaAutoSnap,
        metaColor,
      ),
    onSuccess: () => {
      const id = detail.data?.id;
      const nextName = metaName.trim();
      const nextDesc = metaDesc.trim() || null;
      const nextColor = metaColor;
      setMetaOpen(false);
      if (id) {
        qc.setQueryData<WorkspaceSummary[]>(["workspaces"], (prev) =>
          prev?.map((w) =>
            w.id === id
              ? {
                  ...w,
                  name: nextName,
                  description: nextDesc,
                  color: nextColor,
                  updatedAt: Date.now(),
                }
              : w,
          ),
        );
      }
      qc.invalidateQueries({ queryKey: ["workspaces"], refetchType: "all" });
      qc.invalidateQueries({ queryKey: ["workspace", activeId] });
      toast.success("已更新工作空间信息");
    },
    onError: (e) => {
      const msg = String((e as Error).message);
      setError(msg);
      toast.danger("更新失败", msg);
    },
  });

  const duplicateWs = useMutation({
    mutationFn: () => api.workspaceDuplicate(detail.data!.id, dupName.trim()),
    onSuccess: (created) => {
      setDupOpen(false);
      qc.setQueryData<WorkspaceSummary[]>(["workspaces"], (prev) =>
        prev ? [created, ...prev] : [created],
      );
      qc.invalidateQueries({ queryKey: ["workspaces"], refetchType: "all" });
      toast.success(`已复制「${created.name}」`);
      setParams({ id: created.id });
    },
    onError: (e) => {
      const msg = String((e as Error).message);
      setError(msg);
      toast.danger("复制失败", msg);
    },
  });

  const importWs = useMutation({
    mutationFn: (json: string) => api.workspaceImport(json),
    onSuccess: (created) => {
      qc.setQueryData<WorkspaceSummary[]>(["workspaces"], (prev) =>
        prev ? [created, ...prev] : [created],
      );
      qc.invalidateQueries({ queryKey: ["workspaces"], refetchType: "all" });
      // 导入的文件可能来自外部：后端会停用不可信的应用并剥离启动参数，
      // 这里必须把"动了什么"告诉用户，否则他会以为导入是原样复原。
      const note = created.description?.includes("[导入提示]")
        ? created.description.split("[导入提示]").pop()?.trim()
        : null;
      if (note) {
        toast.warning(`已导入「${created.name}」，但有调整`, note);
      } else {
        toast.success(`已导入「${created.name}」`);
      }
      setParams({ id: created.id });
    },
    onError: (e) => toast.danger("导入失败", String((e as Error).message)),
  });

  const recaptureWs = useMutation({
    mutationFn: () => api.workspaceRecapture(detail.data!.id),
    onSuccess: (updated) => {
      qc.invalidateQueries({ queryKey: ["workspaces"], refetchType: "all" });
      qc.invalidateQueries({ queryKey: ["workspace", activeId] });
      qc.invalidateQueries({ queryKey: ["snapshots", activeId] });
      qc.invalidateQueries({ queryKey: ["storage-stats"] });
      toast.success("已用当前桌面更新", `同步 ${updated.windows.length} 个窗口，旧版本已自动备份`);
    },
    onError: (e) => toast.danger("重新捕获失败", String((e as Error).message)),
  });

  const remove = useMutation({
    mutationFn: (id: string) => api.workspaceDelete(id),
    onSuccess: (_, deletedId) => {
      qc.setQueryData<WorkspaceSummary[]>(["workspaces"], (prev) =>
        prev ? prev.filter((w) => w.id !== deletedId) : prev,
      );
      qc.removeQueries({ queryKey: ["workspace", deletedId] });
      if (activeId === deletedId) setParams({});
      qc.invalidateQueries({ queryKey: ["workspaces"], refetchType: "all" });
      qc.invalidateQueries({ queryKey: ["storage-stats"] });
      qc.invalidateQueries({ queryKey: ["runs"] });
      toast.success("已删除工作空间");
    },
    onError: (e) => {
      const msg = String((e as Error).message);
      setError(msg);
      toast.danger("删除失败", msg);
    },
  });

  // 详情页支持 Ctrl+S 快速保存未提交的窗口摆法
  useEffect(() => {
    if (!activeId || !layoutDirty) return;
    const onKey = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "s") {
        e.preventDefault();
        if (!saveLayout.isPending) saveLayout.mutate();
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [activeId, layoutDirty, saveLayout]);

  const saveItem = useMutation({
    mutationFn: (v: {
      itemId: string;
      reusePolicy: string;
      launchArgs: string | null;
      enabled: boolean;
      launchPriority?: number;
      launchDelayMs?: number;
    }) =>
      api.workspaceItemUpdate(
        v.itemId,
        v.reusePolicy,
        v.launchArgs,
        v.enabled,
        v.launchPriority,
        v.launchDelayMs,
      ),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["workspace", activeId] });
      toast.success("设置已保存");
    },
    onError: (e) => {
      const msg = String((e as Error).message);
      setError(msg);
      toast.danger("保存失败", msg);
    },
  });

  const closeWsWindows = useMutation({
    mutationFn: (id: string) => api.workspaceCloseWindows(id),
    onSuccess: (count) => {
      toast.info(
        count > 0 ? `已向 ${count} 个窗口发送关闭请求` : "当前没有已打开的本空间窗口",
        count > 0 ? "有未保存内容的软件会正常弹出保存提示" : undefined,
      );
    },
    onError: (e) => toast.danger("收工失败", String((e as Error).message)),
  });

  const relocateItem = useMutation({
    mutationFn: ({ itemId, newPath }: { itemId: string; newPath: string }) =>
      api.workspaceItemRelocate(detail.data!.id, itemId, newPath),
    onSuccess: (updated) => {
      qc.invalidateQueries({ queryKey: ["workspace", activeId] });
      toast.success(`已更新程序为 ${updated.displayName}`);
    },
    onError: (e) => toast.danger("更换程序失败", String((e as Error).message)),
  });

  const removeItem = useMutation({
    mutationFn: (itemId: string) => api.workspaceItemDelete(detail.data!.id, itemId),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["workspace", activeId] });
      qc.invalidateQueries({ queryKey: ["workspaces"], refetchType: "all" });
      toast.info("已移除该软件");
    },
    onError: (e) => {
      const msg = String((e as Error).message);
      setError(msg);
      toast.danger("移除失败", msg);
    },
  });

  const guardNavigate = async (next: () => void) => {
    if (!layoutDirty) {
      next();
      return;
    }
    const discard = await confirmAction({
      title: "放弃未保存的窗口摆法？",
      description: "继续离开将丢弃尚未保存的窗口位置调整。",
      confirmLabel: "放弃改动",
      variant: "danger",
    });
    if (discard) {
      if (detail.data) setDraft(detail.data.windows);
      next();
    }
  };

  const handleDelete = async (ws: Pick<WorkspaceSummary, "id" | "name">) => {
    setMenuOpen(false);
    const ok = await confirmAction({
      title: `删除工作空间「${ws.name}」？`,
      description: "将永久删除该工作空间及其历史备份。",
      detail: "不可撤销；不会关闭你当前已经打开的任何软件。",
      confirmLabel: "永久删除",
      variant: "danger",
    });
    if (ok) remove.mutate(ws.id);
  };

  const handleExport = async () => {
    if (!detail.data) return;
    setMenuOpen(false);
    const ok = await confirmAction({
      title: `导出「${detail.data.name}」？`,
      description: "导出为 JSON 文件，可备份或分享给其他电脑导入。",
      detail: "提示：导出文件包含本机软件路径、窗口标题和文件路径，分享前请确认不含隐私内容。",
      confirmLabel: "确认并导出",
    });
    if (!ok) return;
    try {
      const json = await api.workspaceExport(detail.data.id);
      const blob = new Blob([json], { type: "application/json;charset=utf-8" });
      const url = URL.createObjectURL(blob);
      const a = document.createElement("a");
      const safe = detail.data.name.replace(/[\\/:*?"<>|]/g, "_");
      a.href = url;
      a.download = `${safe}.workscene.json`;
      document.body.appendChild(a);
      a.click();
      document.body.removeChild(a);
      URL.revokeObjectURL(url);
      toast.success("已导出配置文件", `${safe}.workscene.json`);
    } catch (e) {
      toast.danger("导出失败", String((e as Error).message));
    }
  };

  const handleRecapture = async () => {
    if (!detail.data) return;
    setMenuOpen(false);
    const ok = await confirmAction({
      title: `用当前桌面更新「${detail.data.name}」？`,
      description: "重新记录你现在打开的软件与窗口摆法，更新前会自动备份旧版本。",
      detail: "已添加的文件、文件夹和网址会自动保留。",
      confirmLabel: "立即更新",
    });
    if (ok) recaptureWs.mutate();
  };

  const handleImportFile = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    e.target.value = "";
    if (!file) return;
    try {
      const json = await file.text();
      // 导入的文件可能是别人给的：先预检"它会启动什么"，再让用户决定
      const preview = await api.workspaceImportPreview(json);
      const risky: string[] = [];
      if (preview.disabledApps.length > 0) {
        risky.push(
          `${preview.disabledApps.length} 个应用的路径不存在或位于网络位置，导入后会被停用：${preview.disabledApps.join("、")}`,
        );
      }
      if (preview.appsWithArgs.length > 0) {
        risky.push(
          `${preview.appsWithArgs.length} 个应用的启动参数会被剥离：${preview.appsWithArgs.join("、")}`,
        );
      }
      const ok = await confirmAction({
        title: `导入「${preview.name}」？`,
        description: [
          `这份文件包含 ${preview.appCount} 个应用、${preview.resourceCount} 个资源。`,
          ...risky,
          risky.length > 0
            ? "导入后仍可手动启用应用或重新填写启动参数；请确认文件来源可信。"
            : "请确认文件来源可信。",
        ].join("\n"),
        confirmLabel: "导入",
      });
      if (!ok) return;
      importWs.mutate(json);
    } catch (err) {
      toast.danger("读取文件失败", String((err as Error).message));
    }
  };

  if (list.isPending) {
    return (
      <Card className="flex items-center gap-2 text-aux text-ink-3">
        <Spinner /> 读取工作空间…
      </Card>
    );
  }

  const hiddenImport = (
    <input
      ref={importInputRef}
      type="file"
      accept=".json,application/json"
      onChange={handleImportFile}
      className="hidden"
    />
  );

  if ((list.data ?? []).length === 0) {
    return (
      <>
        {hiddenImport}
        <EmptyState
          icon={<AppWindow size={22} />}
          title="还没有工作空间"
          description="先捕获当前桌面，或导入已备份的 JSON 文件。"
          action={
            <div className="flex items-center gap-2">
              <Button variant="primary" onClick={() => navigate("/capture")}>
                <Camera size={14} /> 捕获当前桌面
              </Button>
              <Button variant="secondary" onClick={() => importInputRef.current?.click()}>
                <Upload size={14} /> 导入配置
              </Button>
            </div>
          }
        />
      </>
    );
  }

  // ---- 视图 1：工作空间目录页 ----
  if (!activeId) {
    const favCount = (list.data ?? []).filter((w) => w.isFavorite).length;
    return (
      <div className="mx-auto flex max-w-[1040px] flex-col gap-3.5">
        {hiddenImport}
        {error ? <ErrorNote message={error} onDismiss={() => setError(null)} /> : null}

        <div className="flex flex-wrap items-center justify-between gap-2">
          <div className="flex flex-1 flex-wrap items-center gap-2">
            <SearchInput
              value={search}
              onChange={setSearch}
              placeholder="筛选工作空间…"
              className="w-full max-w-[220px]"
            />
            <button
              type="button"
              onClick={() => setOnlyFavorites((v) => !v)}
              className={`inline-flex h-[30px] cursor-pointer items-center gap-1.5 rounded-[8px] border px-2.5 text-aux transition-colors ${
                onlyFavorites
                  ? "border-warning/40 bg-warning/14 text-warning"
                  : "border-line bg-surface text-ink-2 hover:border-line-strong"
              }`}
            >
              <Star size={12} className={onlyFavorites ? "fill-warning" : ""} />
              收藏 ({favCount})
            </button>
            <select
              value={sortBy}
              onChange={(e) => setSortBy(e.target.value as SortKey)}
              aria-label="排序"
              className="h-[30px] rounded-[8px] border border-line bg-surface px-2.5 text-aux text-ink-2 outline-none focus:border-accent"
            >
              <option value="updated">最近更新</option>
              <option value="restored">最近使用</option>
              <option value="windows">窗口数量</option>
              <option value="name">名称排序</option>
            </select>
          </div>

          <div className="flex items-center gap-2">
            <div
              role="group"
              aria-label="切换视图"
              className="inline-flex rounded-[8px] bg-raised/70 p-0.5"
            >
              {(
                [
                  { id: "grid", icon: LayoutGrid, label: "网格" },
                  { id: "list", icon: List, label: "列表" },
                  { id: "compact", icon: Rows3, label: "紧凑" },
                ] as const
              ).map(({ id, icon: Icon, label }) => (
                <button
                  key={id}
                  type="button"
                  title={label}
                  onClick={() => setViewMode(id)}
                  className={`grid size-[26px] cursor-pointer place-items-center rounded-[6px] transition-all ${
                    viewMode === id
                      ? "apple-segment-active"
                      : "text-ink-3 hover:text-ink-2"
                  }`}
                >
                  <Icon size={13} />
                </button>
              ))}
            </div>
            <Button
              size="sm"
              variant="secondary"
              disabled={importWs.isPending}
              onClick={() => importInputRef.current?.click()}
            >
              <Upload size={13} /> 导入
            </Button>
          </div>
        </div>

        {filteredList.length === 0 ? (
          <EmptyState
            title="没有匹配的工作空间"
            action={
              <Button
                size="sm"
                variant="secondary"
                onClick={() => {
                  setSearch("");
                  setOnlyFavorites(false);
                }}
              >
                清除筛选
              </Button>
            }
          />
        ) : viewMode === "grid" ? (
          <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
            {filteredList.map((w) => (
              <Card
                key={w.id}
                interactive
                onClick={() => setParams({ id: w.id })}
                className="flex flex-col justify-between gap-3 p-3.5"
              >
                <div className="min-w-0">
                  <div className="flex items-start justify-between gap-2">
                    <h3 className="flex min-w-0 items-center gap-1.5 text-[14px] font-semibold text-ink">
                      <WorkspaceColorDot color={w.color} />
                      <span className="truncate">{w.name}</span>
                    </h3>
                    <button
                      type="button"
                      title={w.isFavorite ? "取消收藏" : "收藏置顶"}
                      onClick={(e) => {
                        e.stopPropagation();
                        toggleFavorite.mutate({ id: w.id, isFavorite: !w.isFavorite });
                      }}
                      className="shrink-0 cursor-pointer rounded-full p-1 text-ink-3 hover:bg-raised hover:text-warning"
                    >
                      <Star
                        size={13}
                        className={w.isFavorite ? "fill-warning text-warning" : ""}
                      />
                    </button>
                  </div>
                  {w.description ? (
                    <p className="mt-0.5 line-clamp-1 text-[11px] text-ink-3">
                      {w.description}
                    </p>
                  ) : null}
                  {(w.appPaths ?? []).length > 0 ? (
                    <div className="mt-2">
                      <AppIconStrip
                        paths={w.appPaths}
                        icons={appIcons}
                        totalApps={w.itemCount}
                      />
                    </div>
                  ) : null}
                  <p className="mt-2 text-aux text-ink-2">
                    {w.itemCount} 软件 · {w.resourceCount} 文件/网址 · {w.windowCount} 窗口
                  </p>
                  <p className="mt-0.5 text-[11px] text-ink-3">
                    {w.lastRestoredAt
                      ? `上次使用 ${formatRelative(w.lastRestoredAt)}`
                      : `创建于 ${formatRelative(w.createdAt)}`}
                  </p>
                </div>

                <div
                  className="flex items-center justify-between gap-2 border-t border-line pt-2.5"
                  onClick={(e) => e.stopPropagation()}
                >
                  <Button
                    size="sm"
                    variant="primary"
                    onClick={() => navigate(`/restore?workspace=${w.id}&auto=1`)}
                  >
                    <Play size={12} className="fill-current" /> 一键恢复
                  </Button>
                  <div className="flex items-center gap-1">
                    <Button
                      size="sm"
                      variant="secondary"
                      onClick={() => setParams({ id: w.id })}
                    >
                      <SlidersHorizontal size={12} /> 编辑
                    </Button>
                    <Button
                      size="sm"
                      variant="ghost"
                      title="删除"
                      onClick={() => handleDelete(w)}
                      className="px-2 text-ink-3 hover:text-danger"
                    >
                      <Trash2 size={13} />
                    </Button>
                  </div>
                </div>
              </Card>
            ))}
          </div>
        ) : viewMode === "list" ? (
          <Card padded={false} className="overflow-hidden">
            <ul className="divide-y divide-line">
              {filteredList.map((w) => (
                <li
                  key={w.id}
                  onClick={() => setParams({ id: w.id })}
                  className="flex cursor-pointer items-center gap-3 px-4 py-2.5 transition-colors hover:bg-raised/45"
                >
                  <button
                    type="button"
                    onClick={(e) => {
                      e.stopPropagation();
                      toggleFavorite.mutate({ id: w.id, isFavorite: !w.isFavorite });
                    }}
                    className="shrink-0 cursor-pointer rounded-full p-1 text-ink-3 hover:text-warning"
                  >
                    <Star
                      size={13}
                      className={w.isFavorite ? "fill-warning text-warning" : ""}
                    />
                  </button>
                  <div className="min-w-0 flex-1">
                    <p className="flex items-center gap-1.5 truncate text-body font-semibold text-ink">
                      <WorkspaceColorDot color={w.color} />
                      <span className="truncate">{w.name}</span>
                    </p>
                    <p className="text-[11px] text-ink-3">
                      {w.itemCount} 软件 · {w.resourceCount} 文件/网址 · {w.windowCount} 窗口
                    </p>
                  </div>
                  <div
                    className="flex shrink-0 items-center gap-1.5"
                    onClick={(e) => e.stopPropagation()}
                  >
                    <Button
                      size="sm"
                      variant="primary"
                      onClick={() => navigate(`/restore?workspace=${w.id}&auto=1`)}
                    >
                      <Play size={12} className="fill-current" /> 一键恢复
                    </Button>
                    <Button size="sm" variant="secondary" onClick={() => setParams({ id: w.id })}>
                      编辑
                    </Button>
                    <Button
                      size="sm"
                      variant="ghost"
                      onClick={() => handleDelete(w)}
                      className="px-2 text-ink-3 hover:text-danger"
                    >
                      <Trash2 size={13} />
                    </Button>
                  </div>
                </li>
              ))}
            </ul>
          </Card>
        ) : (
          <Card padded={false} className="overflow-hidden">
            <ul className="divide-y divide-line">
              {filteredList.map((w) => (
                <li
                  key={w.id}
                  onClick={() => setParams({ id: w.id })}
                  className="flex cursor-pointer items-center gap-3 px-4 py-2 transition-colors hover:bg-raised/45"
                >
                  <WorkspaceColorDot color={w.color} />
                  {w.isFavorite ? (
                    <Star size={12} className="shrink-0 fill-warning text-warning" />
                  ) : null}
                  <span className="min-w-0 flex-1 truncate text-body font-medium text-ink">
                    {w.name}
                  </span>
                  <span className="tabnum shrink-0 font-mono text-[11px] text-ink-3">
                    {w.itemCount} 软件 · {w.windowCount} 窗口
                  </span>
                  <div
                    className="flex shrink-0 items-center gap-1.5"
                    onClick={(e) => e.stopPropagation()}
                  >
                    <Button
                      size="xs"
                      variant="primary"
                      onClick={() => navigate(`/restore?workspace=${w.id}&auto=1`)}
                    >
                      <Play size={10} className="fill-current" /> 恢复
                    </Button>
                  </div>
                </li>
              ))}
            </ul>
          </Card>
        )}
      </div>
    );
  }

  // ---- 视图 2：工作空间详情与编辑器（精简 3 Tab + 原生文件选择 + 高级项折叠） ----
  const appItems = (detail.data?.items ?? []).filter((i) => i.itemType === "application");
  const resourceItems = (detail.data?.items ?? []).filter((i) =>
    ["file", "folder", "url"].includes(i.itemType),
  );

  return (
    <div className="mx-auto flex max-w-[1080px] flex-col gap-3.5">
      <div className="flex flex-wrap items-center justify-between gap-2.5">
        <div className="flex items-center gap-2">
          <Button
            size="sm"
            variant="ghost"
            onClick={() => guardNavigate(() => setParams({}))}
          >
            <ArrowLeft size={14} /> 返回列表
          </Button>
          <span className="text-ink-3">/</span>
          <select
            value={activeId}
            aria-label="切换工作空间"
            onChange={(e) => guardNavigate(() => setParams({ id: e.target.value }))}
            className="h-[30px] max-w-[200px] rounded-[8px] border border-line bg-surface px-2.5 text-body font-semibold text-ink outline-none focus:border-accent"
          >
            {(list.data ?? []).map((w) => (
              <option key={w.id} value={w.id}>
                {w.isFavorite ? "★ " : ""}
                {w.name}
              </option>
            ))}
          </select>
          {detail.data ? (
            <>
              <button
                type="button"
                title={detail.data.isFavorite ? "取消收藏" : "收藏置顶"}
                onClick={() =>
                  toggleFavorite.mutate({
                    id: detail.data!.id,
                    isFavorite: !detail.data!.isFavorite,
                  })
                }
                className="cursor-pointer rounded-full p-1 text-ink-3 hover:bg-raised hover:text-warning"
              >
                <Star
                  size={14}
                  className={detail.data.isFavorite ? "fill-warning text-warning" : ""}
                />
              </button>
              <button
                type="button"
                title="编辑名称、备注与色标"
                onClick={() => {
                  setMetaName(detail.data!.name);
                  setMetaDesc(detail.data!.description ?? "");
                  setMetaAutoSnap(detail.data!.autoSnapshotEnabled);
                  setMetaColor(detail.data!.color ?? null);
                  setMetaOpen(true);
                }}
                className="cursor-pointer rounded-full p-1 text-ink-3 hover:bg-raised hover:text-ink"
              >
                <Pencil size={13} />
              </button>
              {layoutDirty ? <Badge tone="warning">摆法未保存</Badge> : null}
            </>
          ) : null}
        </div>

        {detail.data ? (
          <div className="flex items-center gap-2">
            <Button
              size="sm"
              variant="secondary"
              onClick={() => navigate(`/history?workspace=${detail.data!.id}`)}
            >
              <History size={13} /> 历史版本
            </Button>
            <Button
              size="sm"
              variant="primary"
              onClick={() =>
                guardNavigate(() => navigate(`/restore?workspace=${detail.data!.id}&auto=1`))
              }
            >
              <Play size={12} className="fill-current" /> 一键恢复
            </Button>
            <div ref={menuRef} className="relative">
              <Button
                size="sm"
                variant="secondary"
                ariaLabel="更多操作"
                onClick={() => setMenuOpen((v) => !v)}
                className="px-2"
              >
                <MoreHorizontal size={15} />
              </Button>
              {menuOpen ? (
                <div className="animate-dialog absolute right-0 top-9 z-30 w-44 rounded-[11px] border border-line-strong/60 bg-surface/95 p-1 shadow-overlay backdrop-blur-xl">
                  <button
                    type="button"
                    onClick={handleRecapture}
                    className="flex w-full cursor-pointer items-center gap-2 rounded-[7px] px-2.5 py-1.5 text-left text-aux text-ink hover:bg-raised"
                  >
                    <RefreshCw size={13} className="text-ink-3" /> 用当前桌面更新
                  </button>
                  <button
                    type="button"
                    onClick={() => {
                      setMenuOpen(false);
                      setDupName(`${detail.data!.name}（副本）`);
                      setDupOpen(true);
                    }}
                    className="flex w-full cursor-pointer items-center gap-2 rounded-[7px] px-2.5 py-1.5 text-left text-aux text-ink hover:bg-raised"
                  >
                    <Copy size={13} className="text-ink-3" /> 复制工作空间
                  </button>
                  <button
                    type="button"
                    onClick={handleExport}
                    className="flex w-full cursor-pointer items-center gap-2 rounded-[7px] px-2.5 py-1.5 text-left text-aux text-ink hover:bg-raised"
                  >
                    <Download size={13} className="text-ink-3" /> 导出备份文件
                  </button>
                  <button
                    type="button"
                    onClick={async () => {
                      setMenuOpen(false);
                      const ok = await confirmAction({
                        title: `收工：关闭「${detail.data!.name}」的窗口？`,
                        description: "仅向本空间已打开的窗口发送标准关闭请求，绝不强杀进程。",
                        detail: "如有未保存文档，软件会正常弹出保存提示。",
                        confirmLabel: "发送关闭请求",
                      });
                      if (ok) closeWsWindows.mutate(detail.data!.id);
                    }}
                    className="flex w-full cursor-pointer items-center gap-2 rounded-[7px] px-2.5 py-1.5 text-left text-aux text-ink hover:bg-raised"
                  >
                    <Power size={13} className="text-ink-3" /> 收工（关闭窗口）
                  </button>
                  <div className="my-1 border-t border-line" />
                  <button
                    type="button"
                    onClick={() => handleDelete(detail.data!)}
                    className="flex w-full cursor-pointer items-center gap-2 rounded-[7px] px-2.5 py-1.5 text-left text-aux text-danger hover:bg-danger/12"
                  >
                    <Trash2 size={13} /> 删除工作空间
                  </button>
                </div>
              ) : null}
            </div>
          </div>
        ) : null}
      </div>

      {error ? <ErrorNote message={error} onDismiss={() => setError(null)} /> : null}
      {detail.isError ? <ErrorNote message={String(detail.error.message)} /> : null}

      {detail.isPending ? (
        <Card className="flex items-center gap-2 text-aux text-ink-3">
          <Spinner /> 读取工作空间详情…
        </Card>
      ) : detail.data ? (
        <>
          <div className="flex flex-wrap items-center justify-between gap-2">
            <Tabs
              value={tab}
              onChange={setTab}
              items={[
                {
                  id: "layout",
                  label: "窗口摆法",
                  count: draft.length,
                  icon: <MonitorSmartphone size={13} />,
                },
                {
                  id: "apps",
                  label: "软件列表",
                  count: appItems.length,
                  icon: <Cpu size={13} />,
                },
                {
                  id: "resources",
                  label: "文件与网址",
                  count: resourceItems.length,
                  icon: <FolderOpen size={13} />,
                },
              ]}
            />
            <span className="text-[11px] text-ink-3">
              {detail.data.description ? `${detail.data.description} · ` : ""}
              {appItems.length} 软件 · {resourceItems.length} 文件/网址 · {draft.length} 窗口
            </span>
          </div>

          {tab === "layout" ? (
            displays.data ? (
              <LayoutEditor
                displays={displays.data}
                windows={draft}
                dirty={layoutDirty}
                onChange={setDraft}
                onReset={() => setDraft(detail.data!.windows)}
                onSave={() => saveLayout.mutate()}
                saving={saveLayout.isPending}
              />
            ) : (
              <Card className="flex items-center gap-2 text-aux text-ink-3">
                <Spinner /> 读取显示器…
              </Card>
            )
          ) : null}

          {tab === "apps" ? (
            <Card padded={false} className="overflow-hidden">
              <div className="flex items-center justify-between border-b border-line px-4 py-2.5 text-aux">
                <span className="font-medium text-ink">包含的软件</span>
                <span className="text-[11px] text-ink-3">
                  开关决定恢复时是否处理该软件；换了安装目录可点「更换程序」
                </span>
              </div>
              <ul className="divide-y divide-line">
                {appItems.map((it) => {
                  const exeLower = (it.target?.split(/[\\/]/).pop() ?? it.displayName).toLowerCase();
                  const isBrowser = [
                    "chrome.exe",
                    "msedge.exe",
                    "firefox.exe",
                    "brave.exe",
                    "arc.exe",
                    "vivaldi.exe",
                    "opera.exe",
                  ].includes(exeLower);
                  const matchedResource = isBrowser
                    ? resourceItems.find((r) => r.itemType === "url")?.target ?? null
                    : resourceItems.find((r) => r.itemType === "folder")?.target ??
                      resourceItems.find((r) => r.itemType === "file")?.target ??
                      null;
                  return (
                    <AppItemRow
                      key={it.id}
                      item={it}
                      icons={appIcons}
                      suggestTarget={matchedResource}
                      disabled={
                        saveItem.isPending || removeItem.isPending || relocateItem.isPending
                      }
                      onUpdate={(patch) =>
                        saveItem.mutate({
                          itemId: it.id,
                          reusePolicy: patch.reusePolicy ?? it.reusePolicy,
                          launchArgs:
                            patch.launchArgs !== undefined ? patch.launchArgs : it.launchArgs,
                          enabled: patch.enabled ?? it.enabled,
                          launchPriority: patch.launchPriority ?? it.launchPriority,
                          launchDelayMs: patch.launchDelayMs ?? it.launchDelayMs,
                        })
                      }
                      onRelocate={async () => {
                        try {
                          const picked = await api.dialogPickFile(true);
                          if (picked) {
                            relocateItem.mutate({ itemId: it.id, newPath: picked });
                          }
                        } catch (e) {
                          toast.danger("打开文件选择框失败", String((e as Error).message));
                        }
                      }}
                      onDelete={async () => {
                        const ok = await confirmAction({
                          title: `移除软件「${it.displayName}」？`,
                          description: "将一并移除该软件记录的窗口位置。",
                          confirmLabel: "移除",
                          variant: "danger",
                        });
                        if (ok) removeItem.mutate(it.id);
                      }}
                    />
                  );
                })}
              </ul>
            </Card>
          ) : null}

          {tab === "resources" ? (
            <ResourcesCard
              workspaceId={detail.data.id}
              items={resourceItems}
              onChanged={() => {
                qc.invalidateQueries({ queryKey: ["workspace", activeId] });
                qc.invalidateQueries({ queryKey: ["workspaces"] });
              }}
              onError={setError}
            />
          ) : null}
        </>
      ) : null}

      <Dialog
        open={metaOpen}
        onClose={() => setMetaOpen(false)}
        title="编辑工作空间信息"
        maxWidth="max-w-md"
        footer={
          <>
            <Button variant="secondary" onClick={() => setMetaOpen(false)}>
              取消
            </Button>
            <Button
              variant="primary"
              disabled={updateMeta.isPending || metaName.trim() === ""}
              onClick={() => updateMeta.mutate()}
            >
              保存
            </Button>
          </>
        }
      >
        <div className="flex flex-col gap-3">
          <label className="flex flex-col gap-1">
            <span className="text-aux font-medium text-ink">名称</span>
            <input
              autoFocus
              value={metaName}
              onChange={(e) => setMetaName(e.target.value)}
              maxLength={80}
              className="h-9 rounded-[8px] border border-line-strong bg-canvas px-3 text-body text-ink outline-none focus:border-accent"
            />
          </label>
          <label className="flex flex-col gap-1">
            <span className="text-aux font-medium text-ink">备注说明（可选）</span>
            <textarea
              value={metaDesc}
              onChange={(e) => setMetaDesc(e.target.value)}
              rows={2}
              maxLength={240}
              className="rounded-[8px] border border-line-strong bg-canvas p-2.5 text-body text-ink outline-none focus:border-accent"
            />
          </label>
          <div className="flex flex-col gap-1.5">
            <span className="text-aux font-medium text-ink">色标分组（可选）</span>
            <div className="flex flex-wrap items-center gap-2">
              {WORKSPACE_COLORS.map((c) => {
                const active = (metaColor ?? null) === c.value;
                return (
                  <button
                    key={c.label}
                    type="button"
                    onClick={() => setMetaColor(c.value)}
                    className={`inline-flex cursor-pointer items-center gap-1.5 rounded-full border px-2.5 py-1 text-[11px] transition-all ${
                      active
                        ? "border-accent bg-accent/12 font-medium text-accent"
                        : "border-line bg-canvas/70 text-ink-2 hover:border-line-strong"
                    }`}
                  >
                    {c.hex ? (
                      <span
                        className="size-2.5 rounded-full"
                        style={{ backgroundColor: c.hex }}
                      />
                    ) : null}
                    {c.label}
                  </button>
                );
              })}
            </div>
          </div>
          <div className="flex items-center justify-between border-t border-line pt-3">
            <div>
              <p className="text-body font-medium text-ink">后台自动备份</p>
              <p className="text-[11px] text-ink-3">关闭后此工作空间不会自动保存历史版本</p>
            </div>
            <Switch
              checked={metaAutoSnap}
              onChange={setMetaAutoSnap}
              ariaLabel="后台自动备份"
            />
          </div>
        </div>
      </Dialog>

      <Dialog
        open={dupOpen}
        onClose={() => setDupOpen(false)}
        title="复制工作空间"
        maxWidth="max-w-md"
        footer={
          <>
            <Button variant="secondary" onClick={() => setDupOpen(false)}>
              取消
            </Button>
            <Button
              variant="primary"
              disabled={duplicateWs.isPending || dupName.trim() === ""}
              onClick={() => duplicateWs.mutate()}
            >
              创建副本
            </Button>
          </>
        }
      >
        <input
          autoFocus
          value={dupName}
          onChange={(e) => setDupName(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && dupName.trim()) duplicateWs.mutate();
          }}
          maxLength={80}
          className="h-9 w-full rounded-[8px] border border-line-strong bg-canvas px-3 text-body text-ink outline-none focus:border-accent"
        />
      </Dialog>
    </div>
  );
}

/** 简洁软件行：默认只显示开关、图标、名称、更换程序、移除；高级参数折叠按需展开 */
function AppItemRow({
  item,
  icons,
  suggestTarget,
  disabled,
  onUpdate,
  onRelocate,
  onDelete,
}: {
  item: WorkspaceItem;
  icons: Record<string, string>;
  suggestTarget: string | null;
  disabled: boolean;
  onUpdate: (patch: {
    reusePolicy?: string;
    launchArgs?: string | null;
    enabled?: boolean;
    launchPriority?: number;
    launchDelayMs?: number;
  }) => void;
  onRelocate: () => void;
  onDelete: () => void;
}) {
  const [expanded, setExpanded] = useState(false);
  const [argsText, setArgsText] = useState(item.launchArgs ?? "");
  useEffect(() => setArgsText(item.launchArgs ?? ""), [item.launchArgs]);

  const commit = () => {
    const trimmed = argsText.trim();
    if (trimmed === (item.launchArgs ?? "")) return;
    onUpdate({ launchArgs: trimmed === "" ? null : trimmed });
  };

  const isPriority = item.launchPriority < 0;

  return (
    <li className={`flex flex-col px-4 py-2.5 transition-opacity ${item.enabled ? "" : "opacity-55"}`}>
      <div className="flex items-center gap-3">
        <Switch
          checked={item.enabled}
          disabled={disabled}
          ariaLabel={`启用 ${item.displayName}`}
          onChange={(v) => onUpdate({ enabled: v })}
        />
        <AppIcon path={item.target} name={item.displayName} icons={icons} size={22} />
        <div className="min-w-0 flex-1">
          <div className="flex items-center gap-1.5">
            <p className="truncate text-body font-medium text-ink">
              {item.displayName.replace(/\.exe$/i, "")}
            </p>
            {isPriority ? <Badge tone="accent">优先启动</Badge> : null}
            {item.launchDelayMs > 0 ? (
              <Badge tone="warning">等 {(item.launchDelayMs / 1000).toFixed(item.launchDelayMs % 1000 === 0 ? 0 : 1)}s</Badge>
            ) : null}
          </div>
          <p
            className="selectable truncate font-mono text-[11px] text-ink-3"
            title={item.target ?? undefined}
          >
            {item.target ?? "未读取到安装路径（可点击右侧「更换程序」手动指定）"}
          </p>
        </div>

        <Button
          size="xs"
          variant="secondary"
          disabled={disabled}
          onClick={onRelocate}
          title="重新选择该软件的 .exe 程序位置"
        >
          <FolderOpen size={12} /> 更换程序
        </Button>

        <Button
          size="xs"
          variant="ghost"
          onClick={() => setExpanded((v) => !v)}
          title="展开高级启动选项（启动顺序、延迟、参数）"
        >
          高级
          <ChevronDown
            size={12}
            className={`transition-transform ${expanded ? "rotate-180" : ""}`}
          />
        </Button>

        <Button
          size="xs"
          variant="ghost"
          disabled={disabled}
          onClick={onDelete}
          title="移除"
          className="px-1.5 text-ink-3 hover:text-danger"
        >
          <Trash2 size={13} />
        </Button>
      </div>

      {expanded ? (
        <div className="mt-2.5 flex flex-col gap-2 rounded-[9px] bg-canvas/70 px-3 py-2.5 pl-[48px]">
          <div className="flex flex-wrap items-center gap-3">
            <label className="flex items-center gap-1.5 text-[11px] text-ink-2">
              <span>打开方式</span>
              <select
                value={item.reusePolicy}
                disabled={disabled || !item.enabled}
                onChange={(e) => onUpdate({ reusePolicy: e.target.value })}
                className="h-7 rounded-[7px] border border-line bg-surface px-2 text-[11px] text-ink outline-none focus:border-accent"
              >
                <option value="reuse_or_launch">已打开就直接用（推荐）</option>
                <option value="reuse_if_matched">只整理已打开的，不自动启动</option>
                <option value="always_launch">总是新开窗口</option>
              </select>
            </label>

            <label className="flex cursor-pointer items-center gap-1.5 text-[11px] text-ink-2">
              <input
                type="checkbox"
                checked={isPriority}
                disabled={disabled || !item.enabled}
                onChange={(e) => onUpdate({ launchPriority: e.target.checked ? -10 : 0 })}
                className="accent-accent"
              />
              <span>优先启动（如代理/依赖服务）</span>
            </label>

            <label className="flex items-center gap-1.5 text-[11px] text-ink-2">
              <span>启动后等待</span>
              <select
                value={item.launchDelayMs}
                disabled={disabled || !item.enabled}
                onChange={(e) => onUpdate({ launchDelayMs: Number(e.target.value) })}
                className="h-7 rounded-[7px] border border-line bg-surface px-2 text-[11px] text-ink outline-none focus:border-accent"
              >
                <option value={0}>不等待</option>
                <option value={1000}>1 秒</option>
                <option value={2000}>2 秒</option>
                <option value={3000}>3 秒</option>
                <option value={5000}>5 秒</option>
              </select>
            </label>
          </div>

          <div className="flex flex-wrap items-center gap-2">
            <input
              value={argsText}
              disabled={disabled || !item.enabled}
              onChange={(e) => setArgsText(e.target.value)}
              onBlur={commit}
              onKeyDown={(e) => {
                if (e.key === "Enter") e.currentTarget.blur();
              }}
              placeholder="可选启动参数（回车保存）"
              className="h-7 min-w-[180px] flex-1 rounded-[7px] border border-line bg-surface px-2.5 font-mono text-[11px] text-ink placeholder:text-ink-3 outline-none focus:border-accent"
            />
            <SuggestArgs
              exeName={item.target?.split(/[\\/]/).pop() ?? item.displayName}
              target={suggestTarget}
              current={item.launchArgs}
              onApply={(args) => {
                setArgsText(args);
                onUpdate({ launchArgs: args });
              }}
            />
          </div>
        </div>
      ) : null}
    </li>
  );
}

/** 文件与网址卡片：支持一键系统文件/文件夹选择框、批量粘贴网址、一键导入 Chrome/Edge 书签夹 */
function ResourcesCard({
  workspaceId,
  items,
  onChanged,
  onError,
}: {
  workspaceId: string;
  items: WorkspaceItem[];
  onChanged: () => void;
  onError: (msg: string) => void;
}) {
  const [urlInput, setUrlInput] = useState("");
  const [urlError, setUrlError] = useState<string | null>(null);
  const [picking, setPicking] = useState(false);
  const [batchOpen, setBatchOpen] = useState(false);
  const [batchTab, setBatchTab] = useState<"paste" | "bookmarks">("paste");
  const [batchText, setBatchText] = useState("");

  const bookmarks = useQuery({
    queryKey: ["browser-bookmarks"],
    queryFn: api.bookmarksListFolders,
    enabled: batchOpen && batchTab === "bookmarks",
    staleTime: 60_000,
    retry: 0,
  });

  const add = useMutation({
    mutationFn: ({ type, target }: { type: "file" | "folder" | "url"; target: string }) =>
      api.workspaceItemAdd(workspaceId, type, target, null),
    onSuccess: (_, vars) => {
      if (vars.type === "url") setUrlInput("");
      setUrlError(null);
      onChanged();
      toast.success(
        vars.type === "url"
          ? "已添加网址"
          : vars.type === "folder"
            ? "已添加文件夹"
            : "已添加文件",
      );
    },
    onError: (e) => {
      const msg = String((e as Error).message);
      onError(msg);
      toast.danger("添加失败", msg);
    },
  });

  const addBatch = useMutation({
    mutationFn: (entries: { url: string; name?: string | null }[]) =>
      api.workspaceItemsAddBatch(workspaceId, entries),
    onSuccess: (count) => {
      setBatchOpen(false);
      setBatchText("");
      onChanged();
      toast.success(count > 0 ? `已批量添加 ${count} 个网址` : "所选网址均已存在");
    },
    onError: (e) => toast.danger("批量添加失败", String((e as Error).message)),
  });

  const remove = useMutation({
    mutationFn: (id: string) => api.workspaceItemDelete(workspaceId, id),
    onSuccess: () => {
      onChanged();
      toast.info("已移除");
    },
    onError: (e) => onError(String((e as Error).message)),
  });

  const pickFile = async () => {
    setPicking(true);
    try {
      const p = await api.dialogPickFile(false);
      if (p) add.mutate({ type: "file", target: p });
    } catch (e) {
      toast.danger("打开文件选择框失败", String((e as Error).message));
    } finally {
      setPicking(false);
    }
  };

  const pickFolder = async () => {
    setPicking(true);
    try {
      const p = await api.dialogPickFolder();
      if (p) add.mutate({ type: "folder", target: p });
    } catch (e) {
      toast.danger("打开文件夹选择框失败", String((e as Error).message));
    } finally {
      setPicking(false);
    }
  };

  const submitUrl = (e: React.FormEvent) => {
    e.preventDefault();
    let val = urlInput.trim();
    if (!val) return;
    if (!/^https?:\/\//i.test(val)) {
      val = `https://${val}`;
    }
    try {
      new URL(val);
    } catch {
      setUrlError("请输入有效的网址，例如 https://example.com");
      return;
    }
    setUrlError(null);
    add.mutate({ type: "url", target: val });
  };

  const submitBatchPaste = () => {
    const lines = batchText
      .split(/[\r\n,]+/)
      .map((s) => s.trim())
      .filter(Boolean);
    const valid: { url: string }[] = [];
    for (const line of lines) {
      const u = /^https?:\/\//i.test(line) ? line : `https://${line}`;
      try {
        new URL(u);
        valid.push({ url: u });
      } catch {
        // ignore invalid lines
      }
    }
    if (valid.length === 0) {
      toast.warning("没有识别到有效网址", "请每行输入一个网址，例如 github.com");
      return;
    }
    addBatch.mutate(valid);
  };

  return (
    <Card className="flex flex-col gap-3.5">
      <div className="flex flex-wrap items-center justify-between gap-2">
        <div>
          <p className="text-body font-semibold text-ink">恢复时顺手打开的文件、文件夹与网页</p>
          <p className="text-[11px] text-ink-3">
            浏览器窗口本身在「软件列表」里备份；如需自动打开指定网页，在这里添加网址或导入书签夹即可
          </p>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <Button size="sm" variant="primary" disabled={picking || add.isPending} onClick={pickFile}>
            <FilePlus2 size={13} /> 选择文件
          </Button>
          <Button
            size="sm"
            variant="secondary"
            disabled={picking || add.isPending}
            onClick={pickFolder}
          >
            <FolderPlus size={13} /> 选择文件夹
          </Button>
          <Button
            size="sm"
            variant="secondary"
            disabled={addBatch.isPending}
            onClick={() => setBatchOpen(true)}
          >
            <Bookmark size={13} /> 批量 / 书签
          </Button>
        </div>
      </div>

      <Dialog
        open={batchOpen}
        onClose={() => setBatchOpen(false)}
        title="批量添加网址或导入书签"
        maxWidth="max-w-lg"
        footer={
          batchTab === "paste" ? (
            <>
              <Button variant="secondary" onClick={() => setBatchOpen(false)}>
                取消
              </Button>
              <Button
                variant="primary"
                disabled={addBatch.isPending || !batchText.trim()}
                onClick={submitBatchPaste}
              >
                {addBatch.isPending ? <Spinner /> : <Plus size={13} />} 批量添加
              </Button>
            </>
          ) : (
            <Button variant="secondary" onClick={() => setBatchOpen(false)}>
              关闭
            </Button>
          )
        }
      >
        <div className="flex flex-col gap-3">
          <div className="grid grid-cols-2 rounded-[8px] bg-raised/70 p-0.5 text-aux">
            <button
              type="button"
              onClick={() => setBatchTab("paste")}
              className={`h-7 cursor-pointer rounded-[6px] font-medium transition-all ${
                batchTab === "paste" ? "apple-segment-active" : "text-ink-3"
              }`}
            >
              批量粘贴网址
            </button>
            <button
              type="button"
              onClick={() => setBatchTab("bookmarks")}
              className={`h-7 cursor-pointer rounded-[6px] font-medium transition-all ${
                batchTab === "bookmarks" ? "apple-segment-active" : "text-ink-3"
              }`}
            >
              导入 Chrome / Edge 书签
            </button>
          </div>

          {batchTab === "paste" ? (
            <div className="flex flex-col gap-1.5">
              <textarea
                autoFocus
                rows={5}
                value={batchText}
                onChange={(e) => setBatchText(e.target.value)}
                placeholder={"每行粘贴一个网址，例如：\nhttps://github.com\nhttps://chatgpt.com\ndocs.rs"}
                className="w-full rounded-[8px] border border-line-strong bg-canvas p-2.5 font-mono text-aux text-ink placeholder:text-ink-3 outline-none focus:border-accent"
              />
              <p className="text-[11px] text-ink-3">会自动补全 https:// 并跳过已经添加过的重复网址。</p>
            </div>
          ) : (
            <div className="flex flex-col gap-2">
              <p className="text-[11px] text-ink-3">
                仅只读本机浏览器公开的书签文件夹列表，不碰密码、Cookie 或浏览记录。
              </p>
              {bookmarks.isPending ? (
                <div className="flex items-center gap-2 py-4 text-aux text-ink-3">
                  <Spinner /> 扫描 Chrome / Edge 书签夹…
                </div>
              ) : (bookmarks.data ?? []).length === 0 ? (
                <p className="rounded-[9px] border border-dashed border-line py-6 text-center text-aux text-ink-3">
                  未在本机默认目录检测到 Chrome / Edge 书签文件夹。
                </p>
              ) : (
                <ul className="max-h-60 divide-y divide-line overflow-y-auto rounded-[9px] border border-line">
                  {(bookmarks.data ?? []).map((f) => (
                    <li
                      key={`${f.browser}:${f.folderPath}`}
                      className="flex items-center justify-between gap-2 px-3 py-2"
                    >
                      <div className="min-w-0 flex-1">
                        <div className="flex items-center gap-1.5">
                          <Badge tone="accent">{f.browser}</Badge>
                          <span className="truncate text-aux font-medium text-ink">
                            {f.folderPath}
                          </span>
                        </div>
                        <p className="mt-0.5 truncate text-[11px] text-ink-3">
                          {f.urls.length} 个网页 · {f.urls.slice(0, 2).map((u) => u.name).join("、")}
                        </p>
                      </div>
                      <Button
                        size="xs"
                        variant="primary"
                        disabled={addBatch.isPending}
                        onClick={() =>
                          addBatch.mutate(f.urls.map((u) => ({ url: u.url, name: u.name })))
                        }
                      >
                        导入 ({f.urls.length})
                      </Button>
                    </li>
                  ))}
                </ul>
              )}
            </div>
          )}
        </div>
      </Dialog>

      <form onSubmit={submitUrl} className="flex flex-col gap-1">
        <div className="flex items-center gap-2">
          <div className="relative flex flex-1 items-center">
            <Globe size={13} className="pointer-events-none absolute left-2.5 text-ink-3" />
            <input
              value={urlInput}
              onChange={(e) => {
                setUrlInput(e.target.value);
                if (urlError) setUrlError(null);
              }}
              placeholder="输入常用网址并回车添加，例如 github.com 或 https://docs.example.com"
              className="h-8 w-full rounded-[8px] border border-line bg-canvas pl-8 pr-3 text-aux text-ink placeholder:text-ink-3 outline-none focus:border-accent"
            />
          </div>
          <Button
            type="submit"
            size="sm"
            variant="secondary"
            disabled={add.isPending || !urlInput.trim()}
          >
            <Plus size={13} /> 添加网址
          </Button>
        </div>
        {urlError ? <p className="text-[11px] text-danger">{urlError}</p> : null}
      </form>

      {items.length === 0 ? (
        <p className="rounded-[9px] border border-dashed border-line py-6 text-center text-aux text-ink-3">
          还没有添加文件或网址，点击上方按钮即可快速添加。
        </p>
      ) : (
        <ul className="divide-y divide-line rounded-[10px] border border-line">
          {items.map((it) => {
            const Icon =
              it.itemType === "url"
                ? Globe
                : it.itemType === "folder"
                  ? FolderOpen
                  : FileText;
            return (
              <li key={it.id} className="flex items-center gap-2.5 px-3.5 py-2">
                <Icon size={14} className="shrink-0 text-accent" />
                <span className="min-w-0 flex-1 truncate text-aux font-medium text-ink">
                  {it.displayName}
                </span>
                <span className="selectable hidden max-w-[45%] truncate font-mono text-[11px] text-ink-3 sm:inline">
                  {it.target}
                </span>
                <Button
                  size="xs"
                  variant="ghost"
                  onClick={() => remove.mutate(it.id)}
                  title="移除"
                  className="text-ink-3 hover:text-danger"
                >
                  <Trash2 size={12} />
                </Button>
              </li>
            );
          })}
        </ul>
      )}
    </Card>
  );
}
