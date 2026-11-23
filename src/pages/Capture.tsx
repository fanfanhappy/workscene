import { useEffect, useMemo, useRef, useState } from "react";
import { useNavigate } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  AlertTriangle,
  Check,
  ChevronDown,
  Minus,
  RefreshCw,
  ScanLine,
  ShieldCheck,
} from "lucide-react";
import { api } from "@/services/api";
import type { CaptureResult, DetectedContext, WindowInfo, WorkspaceSummary } from "@/types/contract";
import { confirmAction, toast } from "@/stores/ui.store";
import {
  Badge,
  Button,
  Card,
  EmptyState,
  ErrorNote,
  SearchInput,
  Spinner,
} from "@/components/ui";
import { MonitorCanvas } from "@/components/MonitorCanvas";
import { AppIcon, useAppIcons } from "@/components/AppIcon";

export function CapturePage() {
  const navigate = useNavigate();
  const qc = useQueryClient();

  const [result, setResult] = useState<CaptureResult | null>(null);
  const [selected, setSelected] = useState<Set<number>>(new Set());
  const [search, setSearch] = useState("");
  const [saveMode, setSaveMode] = useState<"new" | "update">("new");
  const [targetWsId, setTargetWsId] = useState<string>("");
  const [name, setName] = useState("");
  const [showExcluded, setShowExcluded] = useState(false);
  const [highlightedHwnd, setHighlightedHwnd] = useState<number | null>(null);
  const [detectedCtx, setDetectedCtx] = useState<DetectedContext[]>([]);
  const [selectedCtx, setSelectedCtx] = useState<Set<string>>(new Set());
  const [error, setError] = useState<string | null>(null);

  const workspaces = useQuery({
    queryKey: ["workspaces"],
    queryFn: api.workspaceList,
    staleTime: 5_000,
  });

  useEffect(() => {
    const list = workspaces.data ?? [];
    if (list.length === 0) {
      if (targetWsId) setTargetWsId("");
      if (saveMode === "update") setSaveMode("new");
      return;
    }
    if (!targetWsId || !list.some((w) => w.id === targetWsId)) {
      setTargetWsId(list[0]!.id);
    }
  }, [workspaces.data, targetWsId, saveMode]);

  const scannedOnce = useRef(false);
  const detectSeqRef = useRef(0);
  const prevResultRef = useRef<CaptureResult | null>(null);

  const scan = useMutation({
    mutationFn: api.captureStart,
    onMutate: () => {
      setError(null);
      setHighlightedHwnd(null);
      setDetectedCtx([]);
      setSelectedCtx(new Set());
    },
    onSuccess: (data) => {
      qc.setQueryData(["capture"], data);
      const prevResult = prevResultRef.current;
      prevResultRef.current = data;
      setResult(data);
      if (!prevResult) {
        setSelected(new Set(data.windows.map((w) => w.hwnd)));
      } else {
        // 重新扫描时保留用户已取消勾选的窗口，只把新出现的窗口自动勾选
        const prevKnown = new Set(prevResult.windows.map((w) => w.hwnd));
        setSelected((prevSel) => {
          const next = new Set<number>();
          for (const w of data.windows) {
            if (!prevKnown.has(w.hwnd) || prevSel.has(w.hwnd)) {
              next.add(w.hwnd);
            }
          }
          return next;
        });
      }
      setName((prevName) => {
        if (prevName.trim()) return prevName;
        const d = new Date();
        return `工作现场 ${String(d.getHours()).padStart(2, "0")}:${String(
          d.getMinutes(),
        ).padStart(2, "0")}`;
      });
      // 异步探测 L2 上下文（资源管理器已打开文件夹 + VS Code/Cursor/Trae 等 IDE 项目目录），带序号防竞态
      const seq = ++detectSeqRef.current;
      api
        .captureDetectContext(data.windows)
        .then((items) => {
          if (detectSeqRef.current !== seq) return;
          setDetectedCtx(items);
          setSelectedCtx(new Set(items.map((it) => it.target)));
        })
        .catch(() => undefined);
    },
    onError: (e) => {
      const msg = String((e as Error).message);
      setError(msg);
      toast.danger("扫描桌面失败", msg);
    },
  });

  useEffect(() => {
    if (scannedOnce.current) return;
    scannedOnce.current = true;
    scan.mutate();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const save = useMutation({
    mutationFn: async () => {
      if (!result) throw new Error("没有可保存的扫描结果");
      const subset: CaptureResult = {
        ...result,
        windows: result.windows.filter((w) => selected.has(w.hwnd)),
      };
      if (subset.windows.length === 0) throw new Error("请至少勾选一个窗口");

      const selectedApps = new Set(
        subset.windows.map((w) => w.processName.toLowerCase()),
      );
      const pickedCtx = detectedCtx.filter(
        (c) =>
          selectedCtx.has(c.target) &&
          selectedApps.has(c.sourceApp.toLowerCase()),
      );
      const applyDetectedContext = async (wsId: string) => {
        if (pickedCtx.length === 0) return;
        const folders = pickedCtx.filter((c) => c.kind !== "ide_project");
        const ideProjects = pickedCtx.filter((c) => c.kind === "ide_project");
        for (const ctx of folders) {
          await api
            .workspaceItemAdd(wsId, "folder", ctx.target, ctx.displayName)
            .catch(() => undefined);
        }
        if (ideProjects.length > 0) {
          const detail = await api.workspaceGet(wsId).catch(() => null);
          if (detail) {
            const assignedIds = new Set<string>();
            for (const proj of ideProjects) {
              const appItem = detail.items.find(
                (it) =>
                  it.itemType === "application" &&
                  !assignedIds.has(it.id) &&
                  (it.displayName.toLowerCase() === proj.sourceApp.toLowerCase() ||
                    (it.target ?? "").toLowerCase().endsWith(proj.sourceApp.toLowerCase())),
              );
              if (appItem && !appItem.launchArgs) {
                assignedIds.add(appItem.id);
                const arg = proj.target.includes(" ") ? `"${proj.target}"` : proj.target;
                await api
                  .workspaceItemUpdate(
                    appItem.id,
                    appItem.reusePolicy,
                    arg,
                    appItem.enabled,
                    appItem.launchPriority,
                    appItem.launchDelayMs,
                  )
                  .catch(() => undefined);
              } else {
                // 同款 IDE 打开了多个项目目录（或已有自定义启动参数）时，将额外项目目录加入资源列表，避免丢失
                await api
                  .workspaceItemAdd(wsId, "folder", proj.target, proj.displayName)
                  .catch(() => undefined);
              }
            }
          }
        }
      };

      if (saveMode === "update") {
        if (!targetWsId) throw new Error("请选择要更新的工作空间");
        const targetName =
          (workspaces.data ?? []).find((w) => w.id === targetWsId)?.name ?? "所选工作空间";
        const ok = await confirmAction({
          title: `覆盖更新「${targetName}」？`,
          description: `将用当前勾选的 ${subset.windows.length} 个窗口替换原有布局（已有的网页/文件夹资源与启动参数会保留，并自动创建覆盖前保护快照）。`,
          confirmLabel: "确认覆盖更新",
        });
        if (!ok) return null;
        const updated = await api.workspaceUpdateFromCapture(targetWsId, subset);
        await applyDetectedContext(updated.id);
        return {
          id: updated.id,
          name: updated.name,
          updated: true,
          count: updated.windows.length,
          createdSummary: null as WorkspaceSummary | null,
        };
      }

      const trimmed = name.trim();
      if (!trimmed) throw new Error("请给工作空间起个名字");
      const created = await api.workspaceCreate(trimmed, subset);
      await applyDetectedContext(created.id);
      return {
        id: created.id,
        name: created.name,
        updated: false,
        count: created.windowCount,
        createdSummary: created,
      };
    },
    onSuccess: (res) => {
      if (!res) return;
      if (res.createdSummary) {
        const created = res.createdSummary;
        qc.setQueryData<WorkspaceSummary[]>(["workspaces"], (prev) =>
          prev ? [created, ...prev.filter((w) => w.id !== created.id)] : [created],
        );
      }
      qc.invalidateQueries({ queryKey: ["workspaces"], refetchType: "all" });
      qc.invalidateQueries({ queryKey: ["workspace", res.id] });
      qc.invalidateQueries({ queryKey: ["preview", res.id] });
      qc.invalidateQueries({ queryKey: ["ws-profiles", res.id] });
      qc.invalidateQueries({ queryKey: ["snapshots", res.id] });
      qc.invalidateQueries({ queryKey: ["storage-stats"] });
      if (res.updated) {
        toast.success(
          `已更新「${res.name}」的窗口摆法`,
          `同步 ${res.count} 个窗口，原文件和网址已保留，并自动备份了旧版本`,
        );
      } else {
        toast.success(`已保存工作空间「${res.name}」`, `记录了 ${res.count} 个窗口`);
      }
      navigate(`/workspaces?id=${res.id}`);
    },
    onError: (e) => {
      const msg = String((e as Error).message);
      setError(msg);
      toast.danger("保存失败", msg);
    },
  });

  const groups = useMemo(() => {
    const q = search.trim().toLowerCase();
    const map = new Map<string, WindowInfo[]>();
    for (const w of result?.windows ?? []) {
      if (
        q &&
        !w.title.toLowerCase().includes(q) &&
        !w.processName.toLowerCase().includes(q)
      ) {
        continue;
      }
      const key = w.processName.replace(/\.exe$/i, "") || "未知程序";
      const list = map.get(key) ?? [];
      list.push(w);
      map.set(key, list);
    }
    return [...map.entries()].sort((a, b) => b[1].length - a[1].length);
  }, [result, search]);

  const selectedWindows = useMemo(
    () => (result?.windows ?? []).filter((w) => selected.has(w.hwnd)),
    [result, selected],
  );

  const visibleDetectedCtx = useMemo(() => {
    const activeApps = new Set(
      selectedWindows.map((w) => w.processName.toLowerCase()),
    );
    return detectedCtx.filter((c) => activeApps.has(c.sourceApp.toLowerCase()));
  }, [detectedCtx, selectedWindows]);

  const uncertainCount = useMemo(
    () => selectedWindows.filter((w) => !w.processPath).length,
    [selectedWindows],
  );

  const toggle = (hwnd: number) => {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(hwnd)) next.delete(hwnd);
      else next.add(hwnd);
      return next;
    });
  };

  const toggleGroup = (wins: WindowInfo[]) => {
    setSelected((prev) => {
      const next = new Set(prev);
      const all = wins.every((w) => next.has(w.hwnd));
      for (const w of wins) {
        if (all) next.delete(w.hwnd);
        else next.add(w.hwnd);
      }
      return next;
    });
  };

  const hasExisting = (workspaces.data ?? []).length > 0;
  const appIcons = useAppIcons((result?.windows ?? []).map((w) => w.processPath));

  return (
    <div className="mx-auto flex max-w-[1040px] flex-col gap-3.5">
      {error ? <ErrorNote message={error} onDismiss={() => setError(null)} /> : null}

      {scan.isPending && !result ? (
        <Card className="flex flex-col items-center gap-3 py-14 text-center">
          <ScanLine size={24} className="animate-pulse text-accent" />
          <p className="text-card font-semibold text-ink">正在扫描你当前打开的窗口…</p>
          <p className="text-aux text-ink-3">仅记录窗口位置和软件名称，不截屏、不读密码</p>
        </Card>
      ) : result && result.windows.length === 0 ? (
        <EmptyState
          title="没有发现可记录的窗口"
          description="请先打开需要保存的软件窗口，再点击重新扫描。"
          action={
            <Button variant="primary" onClick={() => scan.mutate()}>
              <RefreshCw size={13} /> 重新扫描
            </Button>
          }
        />
      ) : result ? (
        <div className="grid gap-3.5 lg:grid-cols-[minmax(0,1.15fr)_minmax(0,0.85fr)]">
          {/* 左列：勾选要记住的窗口 */}
          <Card padded={false} className="flex min-w-0 flex-col overflow-hidden">
            <div className="flex flex-wrap items-center gap-2 border-b border-line px-3.5 py-2.5">
              <SearchInput
                value={search}
                onChange={setSearch}
                placeholder="筛选软件或窗口标题…"
                className="flex-1"
              />
              <Button
                size="xs"
                variant="ghost"
                onClick={() => {
                  const visible = groups.flatMap(([, ws]) => ws.map((w) => w.hwnd));
                  setSelected((prev) => {
                    const next = new Set(prev);
                    visible.forEach((h) => next.add(h));
                    return next;
                  });
                }}
              >
                {search.trim() ? "全选当前" : "全选"}
              </Button>
              <Button
                size="xs"
                variant="ghost"
                onClick={() => {
                  if (!search.trim()) {
                    setSelected(new Set());
                    return;
                  }
                  const visible = new Set(groups.flatMap(([, ws]) => ws.map((w) => w.hwnd)));
                  setSelected((prev) => {
                    const next = new Set(prev);
                    visible.forEach((h) => next.delete(h));
                    return next;
                  });
                }}
              >
                {search.trim() ? "取消当前" : "清空"}
              </Button>
              <Button
                size="xs"
                variant="secondary"
                disabled={scan.isPending}
                onClick={() => scan.mutate()}
                title="重新扫描当前桌面"
              >
                <RefreshCw size={12} className={scan.isPending ? "animate-spin" : ""} />
              </Button>
            </div>

            <div className="max-h-[430px] flex-1 overflow-y-auto">
              {groups.length === 0 ? (
                <div className="px-4 py-10 text-center text-aux text-ink-3">
                  没有匹配「{search}」的窗口
                </div>
              ) : null}
              {groups.map(([app, wins]) => {
                const checked = wins.filter((w) => selected.has(w.hwnd)).length;
                const all = checked === wins.length;
                const some = checked > 0 && !all;
                return (
                  <div key={app} className="border-b border-line last:border-0">
                    <button
                      type="button"
                      onClick={() => toggleGroup(wins)}
                      className="flex w-full cursor-pointer items-center justify-between bg-canvas/55 px-3.5 py-2 text-left"
                    >
                      <span className="flex items-center gap-2">
                        <span
                          className={`grid size-4 place-items-center rounded-[4px] border ${
                            all || some
                              ? "border-accent bg-accent text-white"
                              : "border-line-strong"
                          }`}
                        >
                          {all ? (
                            <Check size={10} strokeWidth={3} />
                          ) : some ? (
                            <Minus size={10} strokeWidth={3} />
                          ) : null}
                        </span>
                        <AppIcon
                          path={wins[0]?.processPath}
                          name={app}
                          icons={appIcons}
                          size={16}
                        />
                        <span className="text-aux font-semibold text-ink">{app}</span>
                      </span>
                      <span className="tabnum text-[11px] text-ink-3">
                        已选 {checked}/{wins.length}
                      </span>
                    </button>
                    <ul className="divide-y divide-line/60">
                      {wins.map((w) => {
                        const on = selected.has(w.hwnd);
                        return (
                          <li key={w.hwnd}>
                            <button
                              type="button"
                              title={`${w.processName} · ${w.width}×${w.height}`}
                              onMouseEnter={() => setHighlightedHwnd(w.hwnd)}
                              onMouseLeave={() =>
                                setHighlightedHwnd((c) => (c === w.hwnd ? null : c))
                              }
                              onClick={() => toggle(w.hwnd)}
                              className={`flex w-full cursor-pointer items-center gap-2.5 px-3.5 py-2 pl-7 text-left transition-colors ${
                                highlightedHwnd === w.hwnd
                                  ? "bg-accent/10"
                                  : "hover:bg-raised/45"
                              }`}
                            >
                              <span
                                className={`grid size-4 shrink-0 place-items-center rounded-[4px] border ${
                                  on
                                    ? "border-accent bg-accent text-white"
                                    : "border-line-strong"
                                }`}
                              >
                                {on ? <Check size={10} strokeWidth={3} /> : null}
                              </span>
                              <span className="min-w-0 flex-1 truncate text-body text-ink">
                                {w.title || `${app} 窗口`}
                              </span>
                              {!w.processPath ? <Badge tone="warning">权限受限</Badge> : null}
                              {w.isMaximized ? <Badge tone="accent">最大化</Badge> : null}
                              {w.isMinimized ? <Badge tone="warning">最小化</Badge> : null}
                            </button>
                          </li>
                        );
                      })}
                    </ul>
                  </div>
                );
              })}
            </div>

            {result.excluded.length > 0 ? (
              <div className="border-t border-line bg-canvas/45">
                <button
                  type="button"
                  onClick={() => setShowExcluded((v) => !v)}
                  className="flex w-full cursor-pointer items-center justify-between px-3.5 py-2 text-[11px] text-ink-3 hover:text-ink-2"
                >
                  <span>已自动跳过 {result.excluded.length} 个系统后台窗口</span>
                  <ChevronDown
                    size={13}
                    className={`transition-transform ${showExcluded ? "rotate-180" : ""}`}
                  />
                </button>
                {showExcluded ? (
                  <ul className="max-h-28 overflow-y-auto border-t border-line px-3.5 py-1.5 text-[11px]">
                    {result.excluded.slice(0, 25).map((e, i) => (
                      <li key={i} className="flex justify-between gap-2 py-0.5 text-ink-3">
                        <span className="truncate font-mono">
                          {e.processName || e.className}
                        </span>
                        <span className="shrink-0 text-warning">{e.reason}</span>
                      </li>
                    ))}
                  </ul>
                ) : null}
              </div>
            ) : null}
          </Card>

          {/* 右列：保存方式（新建 or 覆盖更新已有）+ 实时布局预览 */}
          <form
            onSubmit={(e) => {
              e.preventDefault();
              save.mutate();
            }}
            className="flex min-w-0 flex-col gap-3.5"
          >
            <Card className="flex flex-col gap-3">
              {hasExisting ? (
                <div className="grid grid-cols-2 rounded-[8px] bg-raised/70 p-0.5 text-aux">
                  <button
                    type="button"
                    onClick={() => setSaveMode("new")}
                    className={`h-7 cursor-pointer rounded-[6px] font-medium transition-all ${
                      saveMode === "new" ? "apple-segment-active" : "text-ink-3"
                    }`}
                  >
                    存为新工作空间
                  </button>
                  <button
                    type="button"
                    onClick={() => setSaveMode("update")}
                    className={`h-7 cursor-pointer rounded-[6px] font-medium transition-all ${
                      saveMode === "update" ? "apple-segment-active" : "text-ink-3"
                    }`}
                  >
                    更新已有工作空间
                  </button>
                </div>
              ) : null}

              {saveMode === "new" ? (
                <label className="flex flex-col gap-1">
                  <span className="text-aux font-medium text-ink">给这个桌面方案起个名字</span>
                  <input
                    autoFocus
                    value={name}
                    onChange={(e) => setName(e.target.value)}
                    maxLength={80}
                    placeholder="例如：日常办公、写代码、看论文"
                    className="h-9 rounded-[8px] border border-line-strong/80 bg-canvas px-3 text-body text-ink outline-none focus:border-accent"
                  />
                </label>
              ) : (
                <label className="flex flex-col gap-1">
                  <span className="text-aux font-medium text-ink">选择要更新的工作空间</span>
                  <select
                    value={targetWsId}
                    onChange={(e) => setTargetWsId(e.target.value)}
                    className="h-9 rounded-[8px] border border-line-strong/80 bg-canvas px-3 text-body text-ink outline-none focus:border-accent"
                  >
                    {(workspaces.data ?? []).map((w) => (
                      <option key={w.id} value={w.id}>
                        {w.name}（当前 {w.windowCount} 窗口）
                      </option>
                    ))}
                  </select>
                  <span className="text-[11px] text-ink-3">
                    会自动保留你添加的文件与网址，更新前还会自动备份旧版本。
                  </span>
                </label>
              )}

              <div>
                <div className="mb-1.5 flex items-center justify-between text-[11px] text-ink-3">
                  <span>窗口摆法预览</span>
                  <span className="tabnum">
                    已选 {selectedWindows.length} 窗口 · {result.displays.length} 屏 · 耗时{" "}
                    {result.durationMs} ms
                  </span>
                </div>
                <MonitorCanvas
                  displays={result.displays}
                  windows={selectedWindows}
                  height={190}
                  selectedHwnd={highlightedHwnd}
                  onSelectWindow={(w) => setHighlightedHwnd(w.hwnd)}
                />
              </div>

              {visibleDetectedCtx.length > 0 ? (
                <div className="flex flex-col gap-1.5 rounded-[10px] border border-accent/25 bg-accent/5 p-2.5">
                  <div className="flex items-center justify-between text-[11px]">
                    <span className="font-medium text-ink">
                      自动识别到 {visibleDetectedCtx.length} 个项目/文件夹上下文
                    </span>
                    <span className="text-ink-3">IDE 项目将写入启动参数，文件夹加入资源</span>
                  </div>
                  <div className="flex max-h-28 flex-col gap-1 overflow-y-auto">
                    {visibleDetectedCtx.map((ctx) => {
                      const on = selectedCtx.has(ctx.target);
                      return (
                        <label
                          key={ctx.target}
                          className="flex cursor-pointer items-center gap-2 rounded-[7px] bg-canvas/80 px-2 py-1 text-[11px] hover:bg-raised/60"
                        >
                          <input
                            type="checkbox"
                            checked={on}
                            onChange={() =>
                              setSelectedCtx((prev) => {
                                const next = new Set(prev);
                                if (next.has(ctx.target)) next.delete(ctx.target);
                                else next.add(ctx.target);
                                return next;
                              })
                            }
                            className="accent-accent"
                          />
                          <Badge tone={ctx.kind === "ide_project" ? "accent" : "neutral"}>
                            {ctx.kind === "ide_project" ? "IDE 项目" : "文件夹"}
                          </Badge>
                          <span className="truncate font-medium text-ink">{ctx.displayName}</span>
                          <span className="ml-auto truncate font-mono text-[10px] text-ink-3">
                            {ctx.target}
                          </span>
                        </label>
                      );
                    })}
                  </div>
                </div>
              ) : null}

              {uncertainCount > 0 ? (
                <div className="flex items-start gap-2 rounded-[9px] border border-warning/30 bg-warning/10 px-3 py-2 text-[11px] text-warning">
                  <AlertTriangle size={13} className="mt-0.5 shrink-0" />
                  <span>
                    有 {uncertainCount} 个窗口受系统权限限制，恢复时如果未打开可能无法自动启动。
                  </span>
                </div>
              ) : null}

              <div className="flex items-start gap-2 rounded-[9px] bg-canvas/70 px-3 py-2 text-[11px] text-ink-3">
                <ShieldCheck size={13} className="mt-0.5 shrink-0 text-success" />
                <span>
                  捕获会记住软件与窗口位置；网页链接、常用文件夹可在保存后的详情页补充。所有数据仅保存在本机。
                </span>
              </div>

              <Button
                type="submit"
                variant="primary"
                className="w-full"
                disabled={
                  save.isPending ||
                  selectedWindows.length === 0 ||
                  (saveMode === "new" ? name.trim() === "" : !targetWsId)
                }
              >
                {save.isPending ? <Spinner /> : <Check size={14} />}
                {saveMode === "update"
                  ? `更新所选工作空间 (${selectedWindows.length} 窗口)`
                  : `保存为工作空间 (${selectedWindows.length} 窗口)`}
              </Button>
            </Card>
          </form>
        </div>
      ) : null}
    </div>
  );
}
