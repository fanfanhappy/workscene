import { useMemo, useState } from "react";
import { useNavigate, useSearchParams } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  AlertTriangle,
  ArrowRightLeft,
  Camera,
  Check,
  CheckCircle2,
  ChevronDown,
  Eye,
  History as HistoryIcon,
  RotateCcw,
  ShieldCheck,
  Sparkles,
  Trash2,
  XCircle,
} from "lucide-react";
import { api, formatTime } from "@/services/api";
import type {
  DisplayInfo,
  RestoreReport,
  SceneFile,
  SnapshotDiff,
  SnapshotSummary,
  WindowInfo,
} from "@/types/contract";
import { confirmAction, toast } from "@/stores/ui.store";
import {
  Badge,
  Button,
  Card,
  Dialog,
  EmptyState,
  ErrorNote,
  Spinner,
} from "@/components/ui";
import { MonitorCanvas } from "@/components/MonitorCanvas";

const typeMeta: Record<
  string,
  { label: string; tone: "neutral" | "accent" | "success" | "warning" }
> = {
  initial: { label: "首次保存", tone: "neutral" },
  manual: { label: "手动备份", tone: "accent" },
  auto: { label: "自动备份", tone: "success" },
  protection: { label: "恢复前备份", tone: "warning" },
};

export function HistoryPage() {
  const navigate = useNavigate();
  const qc = useQueryClient();
  const [params, setParams] = useSearchParams();
  const [picked, setPicked] = useState<string[]>([]);
  const [inspectedId, setInspectedId] = useState<string | null>(null);
  const [filterType, setFilterType] = useState<"all" | "manual" | "auto" | "protection">("all");
  const [diff, setDiff] = useState<SnapshotDiff | null>(null);
  const [restoreReport, setRestoreReport] = useState<RestoreReport | null>(null);
  const [createOpen, setCreateOpen] = useState(false);
  const [snapName, setSnapName] = useState("");
  const [error, setError] = useState<string | null>(null);

  const workspaces = useQuery({
    queryKey: ["workspaces"],
    queryFn: api.workspaceList,
    staleTime: 2_000,
  });

  const requestedId = params.get("workspace");
  const activeId =
    (requestedId && (workspaces.data ?? []).some((w) => w.id === requestedId)
      ? requestedId
      : workspaces.data?.[0]?.id) ?? null;

  const list = useQuery({
    queryKey: ["snapshots", activeId],
    queryFn: () => api.snapshotList(activeId!, 80),
    enabled: Boolean(activeId),
    retry: 0,
  });

  const settings = useQuery({
    queryKey: ["settings"],
    queryFn: api.settingsGet,
    staleTime: 15_000,
    retry: 0,
  });

  const inspectedScene = useQuery({
    queryKey: ["snapshot-scene", inspectedId],
    queryFn: () => api.snapshotGet(inspectedId!),
    enabled: Boolean(inspectedId),
    retry: 0,
  });

  const displays = useQuery({
    queryKey: ["displays"],
    queryFn: api.getDisplays,
    staleTime: 30_000,
    retry: 0,
  });

  const refresh = () => {
    qc.invalidateQueries({ queryKey: ["snapshots", activeId] });
    qc.invalidateQueries({ queryKey: ["workspaces"], refetchType: "all" });
    qc.invalidateQueries({ queryKey: ["runs"] });
    qc.invalidateQueries({ queryKey: ["storage-stats"] });
    qc.invalidateQueries({ queryKey: ["scene-health"] });
  };

  const create = useMutation({
    mutationFn: (name: string) => api.snapshotCreate(activeId!, name),
    onSuccess: (snap) => {
      setCreateOpen(false);
      qc.setQueryData<SnapshotSummary[]>(["snapshots", activeId], (prev) =>
        prev ? [snap, ...prev] : [snap],
      );
      refresh();
      toast.success(`已创建快照「${snap.name}」`);
    },
    onError: (e) => {
      const msg = String((e as Error).message);
      setError(msg);
      toast.danger("创建快照失败", msg);
    },
  });

  const remove = useMutation({
    mutationFn: (id: string) => api.snapshotDelete(id),
    onSuccess: (_, deletedId) => {
      qc.setQueryData<SnapshotSummary[]>(["snapshots", activeId], (prev) =>
        prev ? prev.filter((s) => s.id !== deletedId) : prev,
      );
      setPicked((p) => p.filter((x) => x !== deletedId));
      if (inspectedId === deletedId) setInspectedId(null);
      setDiff(null);
      refresh();
      toast.info("已删除快照");
    },
    onError: (e) => {
      const msg = String((e as Error).message);
      setError(msg);
      toast.danger("删除快照失败", msg);
    },
  });

  const pruneKeep = settings.data?.autoKeep ?? 10;

  const prune = useMutation({
    mutationFn: () => api.snapshotPrune(activeId!, pruneKeep),
    onSuccess: (count) => {
      refresh();
      toast.success(count > 0 ? `已清理 ${count} 份旧自动快照` : "暂无可清理的自动快照");
    },
    onError: (e) => {
      const msg = String((e as Error).message);
      setError(msg);
      toast.danger("清理快照失败", msg);
    },
  });

  const compare = useMutation({
    mutationFn: () => api.snapshotDiff(picked[0]!, picked[1]!),
    onSuccess: setDiff,
    onError: (e) => setError(String((e as Error).message)),
  });

  const restore = useMutation({
    mutationFn: (id: string) => api.snapshotRestore(id, false),
    onSuccess: (r) => {
      refresh();
      setRestoreReport(r);
      toast.success("快照恢复完成", `成功 ${r.succeeded} · 失败 ${r.failed}`);
    },
    onError: (e) => setError(String((e as Error).message)),
  });

  const snapshots = useMemo(() => {
    const sorted = (list.data ?? []).slice().sort((a, b) => b.capturedAt - a.capturedAt);
    if (filterType === "all") return sorted;
    return sorted.filter((s) => s.snapshotType === filterType);
  }, [list.data, filterType]);

  const wsMap = useMemo(() => {
    const m = new Map<string, string>();
    for (const w of workspaces.data ?? []) m.set(w.id, w.name);
    return m;
  }, [workspaces.data]);

  const toggle = (id: string) => {
    setDiff(null);
    setPicked((p) => (p.includes(id) ? p.filter((x) => x !== id) : [...p, id].slice(-2)));
  };

  if (workspaces.isPending) {
    return (
      <Card className="flex items-center gap-2 text-aux text-ink-3">
        <Spinner /> 读取工作空间…
      </Card>
    );
  }

  if ((workspaces.data ?? []).length === 0) {
    return (
      <EmptyState
        icon={<HistoryIcon size={20} />}
        title="还没有历史快照"
        description="先捕获一个工作空间，即可查看快照时间线与恢复记录。"
        action={
          <Button variant="primary" onClick={() => navigate("/capture")}>
            <Camera size={13} /> 捕获现场
          </Button>
        }
      />
    );
  }

  return (
    <div className="mx-auto flex max-w-[1040px] flex-col gap-4 lg:flex-row">
      <aside className="flex w-full shrink-0 flex-col gap-1 lg:w-[216px]">
        {(workspaces.data ?? []).map((w) => (
          <button
            key={w.id}
            type="button"
            onClick={() => {
              setParams({ workspace: w.id });
              setPicked([]);
              setInspectedId(null);
              setDiff(null);
            }}
            className={`w-full cursor-pointer rounded-[10px] border px-3 py-2 text-left transition-all ${
              w.id === activeId
                ? "border-accent/45 bg-accent/12 text-ink"
                : "border-line bg-surface hover:bg-raised/55"
            }`}
          >
            <p className="truncate text-body font-medium text-ink">{w.name}</p>
            <p className="text-[11px] text-ink-3">
              {w.itemCount} 应用 · {w.windowCount} 窗口
            </p>
          </button>
        ))}
      </aside>

      <div className="flex min-w-0 flex-1 flex-col gap-3.5">
        {error ? <ErrorNote message={error} onDismiss={() => setError(null)} /> : null}

        <Card className="flex flex-wrap items-center justify-between gap-2.5 py-2.5">
          <div className="flex items-center gap-0.5 rounded-[8px] bg-raised/70 p-0.5 text-aux">
            {(
              [
                { id: "all", label: "全部" },
                { id: "manual", label: "手动" },
                { id: "auto", label: "自动" },
                { id: "protection", label: "保护" },
              ] as const
            ).map((f) => (
              <button
                key={f.id}
                type="button"
                onClick={() => setFilterType(f.id)}
                className={`cursor-pointer rounded-[6px] px-2.5 py-0.5 text-[11px] font-medium transition-all ${
                  filterType === f.id
                    ? "apple-segment-active"
                    : "text-ink-3 hover:text-ink-2"
                }`}
              >
                {f.label}
              </button>
            ))}
          </div>

          <div className="flex items-center gap-1.5">
            <Button
              size="xs"
              variant="primary"
              onClick={() => {
                const d = new Date();
                setSnapName(
                  `手动快照 ${String(d.getHours()).padStart(2, "0")}:${String(
                    d.getMinutes(),
                  ).padStart(2, "0")}`,
                );
                setCreateOpen(true);
              }}
            >
              <Camera size={12} /> 立即快照
            </Button>
            <Button
              size="xs"
              variant="secondary"
              disabled={picked.length !== 2 || compare.isPending}
              onClick={() => compare.mutate()}
            >
              <ArrowRightLeft size={12} /> 比较 ({picked.length}/2)
            </Button>
            <Button
              size="xs"
              variant="ghost"
              onClick={async () => {
                const ok = await confirmAction({
                  title: "清理旧自动快照？",
                  description: `保留最近 ${pruneKeep} 份自动快照，手动与保护快照不受影响。`,
                  confirmLabel: "清理",
                });
                if (ok) prune.mutate();
              }}
            >
              <Sparkles size={12} /> 清理
            </Button>
          </div>
        </Card>

        {diff ? <DiffCard diff={diff} onClose={() => setDiff(null)} /> : null}

        {inspectedId ? (
          <SnapshotPreview
            scene={inspectedScene.data}
            loading={inspectedScene.isPending}
            fallbackDisplays={displays.data ?? []}
            onClose={() => setInspectedId(null)}
          />
        ) : null}

        {list.isPending ? (
          <Card className="flex items-center gap-2 text-aux text-ink-3">
            <Spinner /> 读取快照…
          </Card>
        ) : snapshots.length === 0 ? (
          <EmptyState title="暂无快照" description="点击上方「立即快照」记录当前桌面现场。" />
        ) : (
          <Card padded={false} className="overflow-hidden">
            <ul className="divide-y divide-line">
              {snapshots.map((s: SnapshotSummary) => {
                const meta = typeMeta[s.snapshotType] ?? {
                  label: s.snapshotType,
                  tone: "neutral" as const,
                };
                const idx = picked.indexOf(s.id);
                const isPicked = idx !== -1;
                return (
                  <li key={s.id} className="flex items-center gap-2.5 px-4 py-2.5">
                    <button
                      type="button"
                      onClick={() => toggle(s.id)}
                      className={`grid size-4 shrink-0 cursor-pointer place-items-center rounded-[4px] border ${
                        isPicked
                          ? "border-accent bg-accent text-white"
                          : "border-line-strong"
                      }`}
                    >
                      {isPicked ? <Check size={10} strokeWidth={3} /> : null}
                    </button>
                    <div className="min-w-0 flex-1">
                      <div className="flex items-center gap-1.5">
                        <span className="truncate text-body font-medium text-ink">
                          {s.name}
                        </span>
                        <Badge tone={meta.tone}>{meta.label}</Badge>
                        {isPicked ? (
                          <Badge tone="accent">{idx === 0 ? "A" : "B"}</Badge>
                        ) : null}
                        {s.isProtected ? (
                          <ShieldCheck size={12} className="text-warning" />
                        ) : null}
                      </div>
                      <p className="tabnum text-[11px] text-ink-3">
                        {formatTime(s.capturedAt)} · {s.windowCount} 窗口 · {s.displayCount} 屏
                      </p>
                    </div>
                    <Button
                      size="xs"
                      variant={inspectedId === s.id ? "tinted" : "ghost"}
                      onClick={() => setInspectedId((c) => (c === s.id ? null : s.id))}
                    >
                      <Eye size={12} /> 预览
                    </Button>
                    <Button
                      size="xs"
                      variant="primary"
                      disabled={restore.isPending}
                      onClick={async () => {
                        const ok = await confirmAction({
                          title: `恢复快照「${s.name}」？`,
                          description: "恢复前会自动创建保护快照，不关闭已打开应用。",
                          confirmLabel: "开始恢复",
                        });
                        if (ok) restore.mutate(s.id);
                      }}
                    >
                      <RotateCcw size={11} /> 恢复
                    </Button>
                    <Button
                      size="xs"
                      variant="ghost"
                      onClick={async () => {
                        const ok = await confirmAction({
                          title: `删除快照「${s.name}」？`,
                          description: "删除不可撤销，不影响工作空间配置。",
                          confirmLabel: "删除",
                          variant: "danger",
                        });
                        if (ok) remove.mutate(s.id);
                      }}
                      className="px-1.5 text-ink-3 hover:text-danger"
                    >
                      <Trash2 size={12} />
                    </Button>
                  </li>
                );
              })}
            </ul>
          </Card>
        )}

        <RunHistorySection activeWorkspaceId={activeId} wsMap={wsMap} />
      </div>

      <Dialog
        open={createOpen}
        onClose={() => setCreateOpen(false)}
        title="创建手动快照"
        maxWidth="max-w-sm"
        footer={
          <>
            <Button variant="secondary" onClick={() => setCreateOpen(false)}>
              取消
            </Button>
            <Button
              variant="primary"
              disabled={create.isPending}
              onClick={() => create.mutate(snapName.trim() || "手动快照")}
            >
              立即保存
            </Button>
          </>
        }
      >
        <input
          autoFocus
          value={snapName}
          onChange={(e) => setSnapName(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") create.mutate(snapName.trim() || "手动快照");
          }}
          className="h-9 w-full rounded-[8px] border border-line-strong bg-canvas px-3 text-body text-ink outline-none focus:border-accent"
          placeholder="输入快照名称"
        />
      </Dialog>

      <Dialog
        open={Boolean(restoreReport)}
        onClose={() => setRestoreReport(null)}
        title="快照恢复报告"
        description={
          restoreReport
            ? `成功 ${restoreReport.succeeded} · 失败 ${restoreReport.failed} · 跳过 ${restoreReport.skipped}`
            : undefined
        }
        footer={
          <Button variant="primary" onClick={() => setRestoreReport(null)}>
            关闭
          </Button>
        }
      >
        {restoreReport ? (
          <ul className="divide-y divide-line">
            {restoreReport.steps.map((s, i) => (
              <li key={i} className="flex items-start gap-2 py-2">
                {s.status === "success" ? (
                  <CheckCircle2 size={14} className="mt-0.5 text-success" />
                ) : s.status === "failed" ? (
                  <XCircle size={14} className="mt-0.5 text-danger" />
                ) : (
                  <AlertTriangle size={14} className="mt-0.5 text-warning" />
                )}
                <div className="min-w-0 flex-1">
                  <p className="truncate text-aux font-medium text-ink">{s.targetName}</p>
                  <p className="text-[11px] text-ink-3">{s.message}</p>
                </div>
              </li>
            ))}
          </ul>
        ) : null}
      </Dialog>
    </div>
  );
}

function SnapshotPreview({
  scene,
  loading,
  fallbackDisplays,
  onClose,
}: {
  scene: SceneFile | undefined;
  loading: boolean;
  fallbackDisplays: DisplayInfo[];
  onClose: () => void;
}) {
  const displays = scene && scene.displays.length > 0 ? scene.displays : fallbackDisplays;
  const wins: WindowInfo[] = useMemo(() => {
    if (!scene) return [];
    return scene.windows.map((w, i) => {
      const m =
        displays.find((d) => d.monitorKey === w.monitorKey) ??
        displays.find((d) => d.isPrimary) ??
        displays[0];
      return {
        hwnd: i + 1,
        processId: 0,
        processPath: w.processPath,
        processName: w.processName,
        processStartedAt: null,
        title: w.title,
        className: w.className,
        x: w.originalWidth > 0 ? w.originalX : Math.round((m?.workX ?? 0) + w.xRatio * (m?.workWidth ?? 1920)),
        y: w.originalHeight > 0 ? w.originalY : Math.round((m?.workY ?? 0) + w.yRatio * (m?.workHeight ?? 1080)),
        width: w.originalWidth > 0 ? w.originalWidth : Math.round(w.widthRatio * (m?.workWidth ?? 1920)),
        height: w.originalHeight > 0 ? w.originalHeight : Math.round(w.heightRatio * (m?.workHeight ?? 1080)),
        dpi: w.sourceDpi || 96,
        monitorKey: w.monitorKey,
        isVisible: true,
        isMinimized: w.showState === "minimized",
        isMaximized: w.showState === "maximized",
        showState: w.showState,
      };
    });
  }, [scene, displays]);

  return (
    <Card className="flex flex-col gap-2.5 border-accent/35">
      <div className="flex items-center justify-between">
        <span className="text-body font-medium text-ink">
          快照预览{scene ? ` · ${scene.name}` : ""}
        </span>
        <Button size="xs" variant="ghost" onClick={onClose}>
          收起
        </Button>
      </div>
      {loading ? (
        <div className="flex items-center gap-2 py-4 text-aux text-ink-3">
          <Spinner /> 加载中…
        </div>
      ) : scene ? (
        <MonitorCanvas displays={displays} windows={wins} height={175} />
      ) : null}
    </Card>
  );
}

function DiffCard({ diff, onClose }: { diff: SnapshotDiff; onClose: () => void }) {
  return (
    <Card className="flex flex-col gap-2.5 border-accent/35">
      <div className="flex items-center justify-between">
        <span className="text-body font-medium text-ink">
          差异对比 · {diff.displayDetail}
        </span>
        <Button size="xs" variant="ghost" onClick={onClose}>
          关闭
        </Button>
      </div>
      <div className="flex flex-wrap gap-1.5">
        <Badge tone="success">新增 {diff.added}</Badge>
        <Badge tone="danger">移除 {diff.removed}</Badge>
        <Badge tone="accent">移动 {diff.moved}</Badge>
        <Badge tone="warning">状态 {diff.stateChanged}</Badge>
        <Badge>未变 {diff.unchanged}</Badge>
      </div>
      {diff.windows.length > 0 ? (
        <ul className="max-h-48 divide-y divide-line overflow-y-auto rounded-[10px] border border-line">
          {diff.windows.map((w, i) => (
            <li key={i} className="flex items-center gap-2 px-3 py-1.5 text-aux">
              <Badge
                tone={
                  w.change === "added"
                    ? "success"
                    : w.change === "removed"
                      ? "danger"
                      : "accent"
                }
              >
                {w.change}
              </Badge>
              <span className="truncate text-ink">{w.titleB || w.titleA}</span>
              <span className="ml-auto truncate font-mono text-[11px] text-ink-3">
                {w.detail}
              </span>
            </li>
          ))}
        </ul>
      ) : null}
    </Card>
  );
}

function RunHistorySection({
  activeWorkspaceId,
  wsMap,
}: {
  activeWorkspaceId: string | null;
  wsMap: Map<string, string>;
}) {
  const [expanded, setExpanded] = useState<string | null>(null);
  const [scope, setScope] = useState<"all" | "current">("all");

  const runs = useQuery({
    queryKey: ["runs"],
    queryFn: () => api.runList(15),
    staleTime: 0,
  });

  const steps = useQuery({
    queryKey: ["run-steps", expanded],
    queryFn: () => api.runSteps(expanded!),
    enabled: Boolean(expanded),
    retry: 0,
  });

  const visible = useMemo(() => {
    const all = runs.data ?? [];
    if (scope === "current" && activeWorkspaceId) {
      return all.filter((r) => r.workspaceId === activeWorkspaceId);
    }
    return all;
  }, [runs.data, scope, activeWorkspaceId]);

  const tone = (status: string) =>
    status === "completed"
      ? "success"
      : status === "partial" || status === "cancelled"
        ? "warning"
        : "danger";

  return (
    <Card padded={false} className="overflow-hidden">
      <div className="flex items-center justify-between border-b border-line px-4 py-2.5">
        <span className="text-aux font-medium text-ink">恢复执行记录</span>
        <div className="inline-flex rounded-[7px] bg-raised/70 p-0.5 text-[11px]">
          <button
            type="button"
            onClick={() => setScope("all")}
            className={`cursor-pointer rounded-[5px] px-2 py-0.5 ${
              scope === "all" ? "apple-segment-active font-medium" : "text-ink-3"
            }`}
          >
            全部
          </button>
          <button
            type="button"
            onClick={() => setScope("current")}
            className={`cursor-pointer rounded-[5px] px-2 py-0.5 ${
              scope === "current" ? "apple-segment-active font-medium" : "text-ink-3"
            }`}
          >
            当前空间
          </button>
        </div>
      </div>

      {runs.isPending ? (
        <div className="flex items-center gap-2 p-4 text-aux text-ink-3">
          <Spinner /> 读取记录…
        </div>
      ) : visible.length === 0 ? (
        <p className="px-4 py-5 text-center text-aux text-ink-3">暂无恢复记录</p>
      ) : (
        <ul className="divide-y divide-line">
          {visible.map((r) => {
            const open = expanded === r.id;
            return (
              <li key={r.id}>
                <button
                  type="button"
                  onClick={() => setExpanded((c) => (c === r.id ? null : r.id))}
                  className="flex w-full cursor-pointer items-center gap-2.5 px-4 py-2.5 text-left hover:bg-raised/45"
                >
                  <Badge tone={tone(r.status)}>
                    {r.status === "completed"
                      ? "成功"
                      : r.status === "partial"
                        ? "部分"
                        : r.status === "cancelled"
                          ? "取消"
                          : "失败"}
                  </Badge>
                  <div className="min-w-0 flex-1">
                    <p className="truncate text-aux font-medium text-ink">
                      {(r.workspaceId && wsMap.get(r.workspaceId)) || "历史快照"}
                    </p>
                    <p className="tabnum text-[11px] text-ink-3">
                      {formatTime(r.startedAt)} · 成 {r.succeeded} / 败 {r.failed} / 跳{" "}
                      {r.skipped}
                    </p>
                  </div>
                  <ChevronDown
                    size={13}
                    className={`shrink-0 text-ink-3 transition-transform ${
                      open ? "rotate-180" : ""
                    }`}
                  />
                </button>
                {open ? (
                  <div className="border-t border-line bg-canvas/55 px-4 py-2">
                    {steps.isPending ? (
                      <p className="py-1 text-aux text-ink-3">读取步骤…</p>
                    ) : (
                      <ul className="divide-y divide-line/60">
                        {(steps.data ?? []).map((s) => (
                          <li key={s.seq} className="flex items-center gap-2 py-1.5 text-[11px]">
                            <span className="tabnum w-4 text-right font-mono text-ink-3">
                              {s.seq + 1}
                            </span>
                            <span className="min-w-0 flex-1 truncate text-ink">
                              {s.targetName}
                            </span>
                            <span className="truncate text-ink-3">{s.message}</span>
                            <span className="tabnum shrink-0 font-mono text-ink-3">
                              {s.durationMs ?? 0}ms
                            </span>
                          </li>
                        ))}
                      </ul>
                    )}
                  </div>
                ) : null}
              </li>
            );
          })}
        </ul>
      )}
    </Card>
  );
}
