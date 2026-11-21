import { useEffect, useMemo, useState } from "react";
import { useSearchParams } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { listen } from "@tauri-apps/api/event";
import {
  AlertTriangle,
  CheckCircle2,
  CircleSlash,
  Clock,
  Loader2,
  Play,
  RefreshCw,
  Undo2,
  XCircle,
} from "lucide-react";
import { api, formatRelative, formatTime } from "@/services/api";
import type {
  RestoreReport,
  RestoreStepResult,
  SceneSummary,
  StepStatus,
} from "@/types/contract";
import { Badge, Button, Card, EmptyState, ErrorNote, Spinner } from "@/components/ui";

const statusMeta: Record<
  StepStatus,
  { label: string; tone: "neutral" | "success" | "warning" | "danger" | "accent"; icon: typeof CheckCircle2 }
> = {
  success: { label: "成功", tone: "success", icon: CheckCircle2 },
  failed: { label: "失败", tone: "danger", icon: XCircle },
  skipped: { label: "跳过", tone: "neutral", icon: CircleSlash },
  timeout: { label: "超时", tone: "warning", icon: Clock },
  needsReview: { label: "需确认", tone: "warning", icon: AlertTriangle },
};

export function RestorePage() {
  const [params, setParams] = useSearchParams();
  const qc = useQueryClient();
  const sceneId = params.get("scene");
  const [relaunch, setRelaunch] = useState(true);
  const [liveSteps, setLiveSteps] = useState<RestoreStepResult[]>([]);
  const [report, setReport] = useState<RestoreReport | null>(null);
  const [error, setError] = useState<string | null>(null);

  const scenes = useQuery({
    queryKey: ["scenes"],
    queryFn: api.sceneList,
    staleTime: 2_000,
  });

  const active = useMemo(
    () => (scenes.data ?? []).find((s) => s.id === sceneId) ?? (scenes.data ?? [])[0],
    [scenes.data, sceneId],
  );

  const preview = useQuery({
    queryKey: ["preview", active?.id],
    queryFn: () => api.restorePreview(active!.id),
    enabled: Boolean(active?.id),
    retry: 0,
  });

  // 后端每完成一步就推送真实事件，前端只展示，不伪造进度
  useEffect(() => {
    const unlisten = listen<RestoreStepResult>("restore:step", (e) => {
      setLiveSteps((prev) => [...prev, e.payload]);
    });
    return () => {
      unlisten.then((fn) => fn()).catch(() => undefined);
    };
  }, []);

  const run = useMutation({
    mutationFn: () => api.restoreApply(active!.id, relaunch),
    onMutate: () => {
      setLiveSteps([]);
      setReport(null);
      setError(null);
    },
    onSuccess: (r) => {
      setReport(r);
      qc.invalidateQueries({ queryKey: ["scenes"] });
    },
    onError: (e) => setError(String((e as Error).message)),
  });

  // 撤销：把恢复前自动创建的保护快照再 apply 一次
  const undo = useMutation({
    mutationFn: (preId: string) => api.restoreApply(preId, false),
    onMutate: () => {
      setLiveSteps([]);
      setError(null);
    },
    onSuccess: (r) => {
      setReport(r);
      qc.invalidateQueries({ queryKey: ["scenes"] });
    },
    onError: (e) => setError(String((e as Error).message)),
  });

  if (scenes.isPending) {
    return (
      <Card className="flex items-center gap-2 text-aux text-ink-3">
        <Spinner /> 读取快照…
      </Card>
    );
  }

  if ((scenes.data ?? []).length === 0) {
    return (
      <EmptyState
        title="还没有可恢复的工作现场"
        description="先在「现场捕获」里保存一个工作现场，再回到这里执行恢复。"
        action={<Button onClick={() => setParams({})}>刷新</Button>}
      />
    );
  }

  const running = run.isPending;

  return (
    <div className="mx-auto flex max-w-[1200px] gap-5">
      <aside className="w-[260px] shrink-0">
        <p className="mb-2 px-1 text-[11px] font-medium uppercase tracking-widest text-ink-3">
          工作现场
        </p>
        <ul className="flex flex-col gap-1.5">
          {(scenes.data ?? []).map((s: SceneSummary) => {
            const isActive = s.id === active?.id;
            return (
              <li key={s.id}>
                <button
                  onClick={() => {
                    setParams({ scene: s.id });
                    setReport(null);
                    setLiveSteps([]);
                  }}
                  className={`w-full rounded-md border px-3 py-2.5 text-left transition-colors ${
                    isActive
                      ? "border-accent/50 bg-raised"
                      : "border-line bg-surface hover:bg-raised/60"
                  }`}
                >
                  <p className="truncate text-body font-medium">{s.name}</p>
                  <p className="mt-0.5 text-[11px] text-ink-3">
                    {s.windowCount} 窗口 · {formatRelative(s.capturedAt)}
                  </p>
                </button>
              </li>
            );
          })}
        </ul>
      </aside>

      <div className="flex min-w-0 flex-1 flex-col gap-4">
        {error ? <ErrorNote message={error} /> : null}

        {/* 状态 A：恢复预览 */}
        {!report && !running ? (
          <>
            <Card className="flex flex-col gap-4">
              <div className="flex items-start justify-between gap-4">
                <div>
                  <h2 className="text-module font-semibold tracking-tight">{active?.name}</h2>
                  <p className="mt-1 text-aux text-ink-3">
                    创建于 {active ? formatTime(active.capturedAt) : ""} ·{" "}
                    {preview.data?.summary.total ?? active?.windowCount ?? 0} 个窗口
                  </p>
                </div>
                <div className="flex items-center gap-2">
                  <Button variant="ghost" size="sm" onClick={() => preview.refetch()}>
                    <RefreshCw size={14} /> 重新检查
                  </Button>
                  <Button
                    variant="primary"
                    disabled={preview.isPending || !preview.data}
                    onClick={() => run.mutate()}
                  >
                    <Play size={15} /> 开始恢复
                  </Button>
                </div>
              </div>

              <label className="flex items-center gap-2 text-aux text-ink-2">
                <input
                  type="checkbox"
                  checked={relaunch}
                  onChange={(e) => setRelaunch(e.target.checked)}
                  className="size-3.5 accent-[var(--color-accent)]"
                />
                应用未运行时自动启动（需要可执行路径仍然有效）
              </label>
            </Card>

            {preview.isError ? <ErrorNote message={String(preview.error.message)} /> : null}

            {preview.data ? (
              <>
                <div className="grid grid-cols-3 gap-3">
                  <SummaryTile label="可恢复" value={preview.data.summary.ready} tone="success" />
                  <SummaryTile
                    label="需确认"
                    value={preview.data.summary.warning}
                    tone="warning"
                  />
                  <SummaryTile label="缺失" value={preview.data.summary.missing} tone="danger" />
                </div>

                {preview.data.displayNotes.length > 0 ? (
                  <Card className="border-warning/40 bg-warning/5">
                    <p className="mb-1.5 flex items-center gap-2 text-aux font-medium text-warning">
                      <AlertTriangle size={14} /> 显示器变化
                    </p>
                    <ul className="flex flex-col gap-1 text-aux text-ink-2">
                      {preview.data.displayNotes.map((n) => (
                        <li key={n}>· {n}</li>
                      ))}
                    </ul>
                  </Card>
                ) : null}

                <Card padded={false} className="overflow-hidden">
                  <div className="border-b border-line px-5 py-2.5 text-aux text-ink-2">
                    恢复计划
                  </div>
                  <ul className="divide-y divide-line/60">
                    {preview.data.items.map((item, i) => {
                      const meta = statusMeta[item.status];
                      const Icon = meta.icon;
                      return (
                        <li key={`${item.targetName}-${i}`} className="flex gap-3 px-5 py-2.5">
                          <Icon
                            size={15}
                            className={`mt-0.5 shrink-0 ${
                              meta.tone === "success"
                                ? "text-success"
                                : meta.tone === "danger"
                                  ? "text-danger"
                                  : meta.tone === "warning"
                                    ? "text-warning"
                                    : "text-ink-3"
                            }`}
                          />
                          <div className="min-w-0 flex-1">
                            <p className="truncate text-body">{item.targetName}</p>
                            <p className="mt-0.5 text-[11px] text-ink-3">{item.detail}</p>
                            <p className="mt-0.5 font-mono text-[11px] text-ink-3">
                              {item.matchNote}
                            </p>
                          </div>
                          <Badge tone={meta.tone}>{meta.label}</Badge>
                        </li>
                      );
                    })}
                    {preview.data.items.length === 0 ? (
                      <li className="px-5 py-6 text-center text-aux text-ink-3">
                        该快照没有窗口
                      </li>
                    ) : null}
                  </ul>
                </Card>
              </>
            ) : null}
          </>
        ) : null}

        {/* 状态 B：执行中 */}
        {running ? (
          <Card className="flex flex-col gap-4">
            <div className="flex items-center gap-3">
              <Loader2 size={18} className="animate-spin text-accent" />
              <div>
                <p className="text-card font-medium">正在恢复「{active?.name}」</p>
                <p className="text-aux text-ink-3">
                  {liveSteps.length} / {preview.data?.summary.total ?? active?.windowCount ?? 0} 步已完成
                </p>
              </div>
            </div>
            <div className="h-1 overflow-hidden rounded-full bg-raised">
              <div
                className="h-full bg-accent transition-[width] duration-200"
                style={{
                  width: `${Math.min(
                    100,
                    (liveSteps.length /
                      Math.max(1, preview.data?.summary.total ?? active?.windowCount ?? 1)) *
                      100,
                  )}%`,
                }}
              />
            </div>
            <ul className="flex flex-col gap-1.5">
              {liveSteps.map((s, i) => (
                <StepRow key={`${s.targetName}-${i}`} step={s} />
              ))}
            </ul>
            <p className="text-[11px] text-ink-3">
              恢复过程中不会抢焦点，也不会关闭你已打开的任何应用。
            </p>
          </Card>
        ) : null}

        {/* 状态 C：恢复报告 */}
        {report ? (
          <>
            <Card className="flex flex-col gap-4">
              <div className="flex items-start justify-between gap-4">
                <div>
                  <p className="flex items-center gap-2 text-card font-medium">
                    {report.failed === 0 ? (
                      <CheckCircle2 size={16} className="text-success" />
                    ) : report.succeeded > 0 ? (
                      <AlertTriangle size={16} className="text-warning" />
                    ) : (
                      <XCircle size={16} className="text-danger" />
                    )}
                    {report.failed === 0
                      ? "恢复完成"
                      : report.succeeded > 0
                        ? "部分恢复成功"
                        : "恢复失败"}
                  </p>
                  <p className="mt-1 text-aux text-ink-3">
                    成功 {report.succeeded} · 失败 {report.failed} · 跳过 {report.skipped} · 耗时{" "}
                    {report.finishedAt - report.startedAt} ms
                  </p>
                </div>
                <div className="flex gap-2">
                  {report.preRestoreSceneId ? (
                    <Button
                      size="sm"
                      variant="secondary"
                      disabled={undo.isPending}
                      title="把恢复前的保护快照再应用一次"
                      onClick={() => {
                        if (confirm("撤销这次恢复？将把窗口恢复到恢复前的状态。"))
                          undo.mutate(report.preRestoreSceneId!);
                      }}
                    >
                      {undo.isPending ? <Spinner /> : <Undo2 size={14} />}
                      撤销这次恢复
                    </Button>
                  ) : null}
                  <Button
                    size="sm"
                    variant="secondary"
                    onClick={() => {
                      setReport(null);
                      preview.refetch();
                    }}
                  >
                    返回预览
                  </Button>
                  <Button size="sm" variant="primary" onClick={() => run.mutate()}>
                    <RefreshCw size={14} /> 再次恢复
                  </Button>
                </div>
              </div>

              {report.displayNotes.length > 0 ? (
                <ul className="rounded-md border border-warning/40 bg-warning/5 px-3 py-2 text-[11px] text-warning">
                  {report.displayNotes.map((n) => (
                    <li key={n}>· {n}</li>
                  ))}
                </ul>
              ) : null}
            </Card>

            <Card padded={false} className="overflow-hidden">
              <div className="border-b border-line px-5 py-2.5 text-aux text-ink-2">
                逐项结果
              </div>
              <ul className="divide-y divide-line/60">
                {report.steps.map((s, i) => (
                  <StepRow key={`${s.targetName}-${i}`} step={s} detailed />
                ))}
              </ul>
            </Card>
          </>
        ) : null}
      </div>
    </div>
  );
}

function StepRow({ step, detailed = false }: { step: RestoreStepResult; detailed?: boolean }) {
  const meta = statusMeta[step.status];
  const Icon = meta.icon;
  return (
    <li className="flex items-start gap-3 rounded-md px-1 py-1.5">
      <Icon
        size={15}
        className={`mt-0.5 shrink-0 ${
          meta.tone === "success"
            ? "text-success"
            : meta.tone === "danger"
              ? "text-danger"
              : meta.tone === "warning"
                ? "text-warning"
                : "text-ink-3"
        }`}
      />
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2">
          <p className="truncate text-body">{step.targetName}</p>
          <Badge tone={meta.tone}>{meta.label}</Badge>
          {detailed ? (
            <span className="tabnum ml-auto text-[11px] text-ink-3">{step.durationMs} ms</span>
          ) : null}
        </div>
        <p className="mt-0.5 text-[11px] text-ink-2">{step.message}</p>
        {detailed ? (
          <p className="mt-0.5 font-mono text-[11px] text-ink-3">{step.matchNote}</p>
        ) : null}
      </div>
    </li>
  );
}

function SummaryTile({
  label,
  value,
  tone,
}: {
  label: string;
  value: number;
  tone: "success" | "warning" | "danger";
}) {
  const color =
    tone === "success" ? "text-success" : tone === "warning" ? "text-warning" : "text-danger";
  return (
    <Card className="flex items-baseline gap-2 py-3">
      <span className={`tabnum text-[22px] font-semibold ${color}`}>{value}</span>
      <span className="text-aux text-ink-3">{label}</span>
    </Card>
  );
}
