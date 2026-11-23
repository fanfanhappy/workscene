import { useEffect, useMemo, useRef, useState } from "react";
import { useNavigate, useSearchParams } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { listen } from "@tauri-apps/api/event";
import {
  AlertTriangle,
  Camera,
  CheckCircle2,
  CircleSlash,
  Clock,
  Lightbulb,
  Loader2,
  Play,
  Power,
  RefreshCw,
  RotateCw,
  ShieldCheck,
  SlidersHorizontal,
  Star,
  Undo2,
  XCircle,
} from "lucide-react";
import { api, formatRelative } from "@/services/api";
import type {
  RestoreReport,
  RestoreStepResult,
  StepStatus,
  WorkspaceSummary,
} from "@/types/contract";
import { confirmAction, toast } from "@/stores/ui.store";
import {
  Badge,
  Button,
  Card,
  EmptyState,
  ErrorNote,
  SearchInput,
  Spinner,
  Switch,
} from "@/components/ui";
import { WorkspaceColorDot } from "@/components/AppIcon";

const statusMeta: Record<
  StepStatus,
  {
    preview: string;
    report: string;
    tone: "neutral" | "success" | "warning" | "danger" | "accent";
    icon: typeof CheckCircle2;
  }
> = {
  success: { preview: "就绪", report: "已完成", tone: "success", icon: CheckCircle2 },
  failed: { preview: "找不到", report: "未成功", tone: "danger", icon: XCircle },
  skipped: { preview: "跳过", report: "已跳过", tone: "neutral", icon: CircleSlash },
  timeout: { preview: "超时", report: "超时", tone: "warning", icon: Clock },
  needsReview: { preview: "需留意", report: "需确认", tone: "warning", icon: AlertTriangle },
};

export function RestorePage() {
  const navigate = useNavigate();
  const [params, setParams] = useSearchParams();
  const qc = useQueryClient();
  const workspaceId = params.get("workspace");
  const autoStart = params.get("auto") === "1";

  const [wsSearch, setWsSearch] = useState("");
  const [relaunch, setRelaunch] = useState(true);
  const [focusMode, setFocusMode] = useState(false);
  const [relaunchSynced, setRelaunchSynced] = useState(false);
  const [showTech, setShowTech] = useState(false);
  const [liveSteps, setLiveSteps] = useState<RestoreStepResult[]>([]);
  const [report, setReport] = useState<RestoreReport | null>(null);
  const [isUndoReport, setIsUndoReport] = useState(false);
  const [expectedSteps, setExpectedSteps] = useState(1);
  const [reportFilter, setReportFilter] = useState<"all" | "failed" | "skipped" | "success">("all");
  const [error, setError] = useState<string | null>(null);
  const [elapsedMs, setElapsedMs] = useState(0);
  const [cancelling, setCancelling] = useState(false);
  const [autoPausedNote, setAutoPausedNote] = useState<string | null>(null);
  const autoTriggeredFor = useRef<string | null>(null);

  const settings = useQuery({
    queryKey: ["settings"],
    queryFn: api.settingsGet,
    staleTime: 10_000,
    retry: 0,
  });

  useEffect(() => {
    if (!relaunchSynced && (settings.data || settings.isError)) {
      if (settings.data) {
        setRelaunch(settings.data.restoreRelaunchDefault);
        setFocusMode(settings.data.restoreFocusModeDefault);
      }
      setRelaunchSynced(true);
    }
  }, [settings.data, settings.isError, relaunchSynced]);

  const workspaces = useQuery({
    queryKey: ["workspaces"],
    queryFn: api.workspaceList,
    staleTime: 2_000,
  });

  const busy = useQuery({
    queryKey: ["restore-busy"],
    queryFn: api.restoreIsBusy,
    refetchInterval: 1_500,
  });

  const active = useMemo(
    () =>
      (workspaces.data ?? []).find((s) => s.id === workspaceId) ??
      (workspaces.data ?? [])[0],
    [workspaces.data, workspaceId],
  );

  // 当通过 Ctrl+K 或路由切换到另一工作空间时，自动清理旧空间报告与状态，确保 ?auto=1 正常触发
  const lastWorkspaceIdRef = useRef<string | null>(workspaceId);
  useEffect(() => {
    if (workspaceId && lastWorkspaceIdRef.current && workspaceId !== lastWorkspaceIdRef.current) {
      setReport(null);
      setIsUndoReport(false);
      setLiveSteps([]);
      setError(null);
      setAutoPausedNote(null);
      autoTriggeredFor.current = null;
    }
    lastWorkspaceIdRef.current = workspaceId;
  }, [workspaceId]);

  const filteredWorkspaces = useMemo(() => {
    const q = wsSearch.trim().toLowerCase();
    return (workspaces.data ?? []).filter((w) =>
      q ? w.name.toLowerCase().includes(q) : true,
    );
  }, [workspaces.data, wsSearch]);

  const preview = useQuery({
    queryKey: ["preview", active?.id],
    queryFn: () => api.workspacePreview(active!.id),
    enabled: Boolean(active?.id),
    retry: 0,
  });

  useEffect(() => {
    const unlisten = listen<RestoreStepResult>("restore:step", (e) => {
      setLiveSteps((prev) => [...prev, e.payload]);
    });
    return () => {
      unlisten.then((fn) => fn()).catch(() => undefined);
    };
  }, []);

  const handleReportResult = (r: RestoreReport, labelPrefix = "") => {
    setCancelling(false);
    setReport(r);
    qc.invalidateQueries({ queryKey: ["workspaces"], refetchType: "all" });
    qc.invalidateQueries({ queryKey: ["runs"] });
    qc.invalidateQueries({ queryKey: ["snapshots"] });
    qc.invalidateQueries({ queryKey: ["storage-stats"] });
    qc.invalidateQueries({ queryKey: ["preview", active?.id] });
    const wasCancelled = r.steps.some((s) => s.code === "cancelled");
    if (wasCancelled) {
      toast.warning(`「${r.sceneName}」恢复已取消`, `已完成 ${r.succeeded} 项`);
    } else if (r.failed === 0 && r.skipped === 0) {
      toast.success(`${labelPrefix}「${r.sceneName}」已恢复完成`, `共完成 ${r.succeeded} 项`);
    } else if (r.succeeded > 0) {
      toast.warning(
        `${labelPrefix}「${r.sceneName}」部分完成`,
        `完成 ${r.succeeded} · 未完成 ${r.failed + r.skipped}`,
      );
    } else {
      toast.danger(`「${r.sceneName}」恢复未完成`, `请查看原因或点击重试`);
    }
  };

  const run = useMutation({
    mutationFn: () => api.workspaceRestore(active!.id, relaunch, focusMode),
    onMutate: () => {
      setLiveSteps([]);
      setReport(null);
      setIsUndoReport(false);
      setExpectedSteps(Math.max(1, preview.data?.summary.total ?? 1));
      setReportFilter("all");
      setError(null);
      setElapsedMs(0);
      setCancelling(false);
      setAutoPausedNote(null);
    },
    onSuccess: (r) => handleReportResult(r),
    onError: (e) => {
      setCancelling(false);
      const msg = String((e as Error).message);
      setError(msg);
      toast.danger("恢复失败", msg);
    },
  });

  const closeWindows = useMutation({
    mutationFn: () => api.workspaceCloseWindows(active!.id),
    onSuccess: (count) => {
      // WM_CLOSE 为异步关闭，延迟 600ms 刷新预览才能反映真实关闭后的桌面状态
      window.setTimeout(() => {
        qc.invalidateQueries({ queryKey: ["preview", active?.id] });
      }, 600);
      toast.info(
        count > 0 ? `已向 ${count} 个窗口发送关闭请求` : "当前没有已打开的本空间窗口",
        count > 0 ? "有未保存内容的软件会正常弹出保存确认" : undefined,
      );
    },
    onError: (e) => toast.danger("收工失败", String((e as Error).message)),
  });

  // 仅重试未完成/失败项（支持全量重试或单条重试）
  const retryFailed = useMutation({
    mutationFn: (targets: string[]) =>
      api.workspaceRestoreRetry(active!.id, relaunch, targets),
    onMutate: (targets) => {
      setLiveSteps([]);
      setIsUndoReport(false);
      setExpectedSteps(Math.max(1, targets.length));
      setError(null);
      setElapsedMs(0);
      setCancelling(false);
    },
    onSuccess: (r) => handleReportResult(r, "重试"),
    onError: (e) => {
      setCancelling(false);
      const msg = String((e as Error).message);
      setError(msg);
      toast.danger("重试失败", msg);
    },
  });

  // 智能一键恢复：只要没有缺失/失效的程序路径（未打开软件属于正常一键补启），直接自动开始恢复；
  // 仅当存在失效路径时停在预览页让用户核对或修复路径。
  useEffect(() => {
    if (!autoStart || !active?.id || !preview.data || !relaunchSynced) return;
    if (autoTriggeredFor.current === active.id) return;
    if (run.isPending || report || busy.data === true) return;

    autoTriggeredFor.current = active.id;
    const isClean =
      preview.data.summary.missing === 0 &&
      preview.data.summary.total > 0;

    // 清除 URL 上的 auto 参数，避免返回预览后反复触发
    setParams({ workspace: active.id }, { replace: true });

    if (isClean) {
      run.mutate();
    } else {
      setAutoPausedNote(
        "检测到有失效的程序或资源路径，已先为你停在确认页，请修复或核对后再点「开始恢复」。",
      );
    }
  }, [
    autoStart,
    active?.id,
    preview.data,
    relaunchSynced,
    run,
    report,
    busy.data,
    setParams,
  ]);

  const cancelRun = useMutation({
    mutationFn: api.restoreCancel,
    onMutate: () => setCancelling(true),
    onSuccess: (ok) => {
      if (ok) toast.info("正在安全停止", "已经打开的软件不会被关闭");
    },
    onError: (e) => {
      setCancelling(false);
      toast.danger("取消失败", String((e as Error).message));
    },
  });

  const undo = useMutation({
    mutationFn: (preId: string) => api.snapshotRestore(preId, true),
    onMutate: () => {
      setLiveSteps([]);
      setError(null);
    },
    onSuccess: (r) => {
      setIsUndoReport(true);
      setReport(r);
      qc.invalidateQueries({ queryKey: ["workspaces"], refetchType: "all" });
      qc.invalidateQueries({ queryKey: ["runs"] });
      qc.invalidateQueries({ queryKey: ["snapshots"] });
      qc.invalidateQueries({ queryKey: ["storage-stats"] });
      qc.invalidateQueries({ queryKey: ["preview", active?.id] });
      toast.success("已撤销上次恢复", `已还原 ${r.succeeded} 个窗口`);
    },
    onError: (e) => {
      const msg = String((e as Error).message);
      setError(msg);
      toast.danger("撤销失败", msg);
    },
  });

  const running = run.isPending || retryFailed.isPending || undo.isPending;

  useEffect(() => {
    if (!running) return;
    const start = Date.now();
    const timer = window.setInterval(() => setElapsedMs(Date.now() - start), 100);
    return () => window.clearInterval(timer);
  }, [running]);

  const plainSummary = useMemo(() => {
    if (!preview.data) return "";
    const { total, ready, warning, missing } = preview.data.summary;
    if (total === 0) return "这个工作空间还没有可恢复的内容。";
    const parts: string[] = [];
    if (ready > 0) parts.push(`${ready} 项已就绪`);
    if (warning > 0) parts.push(`${warning} 项需留意${relaunch ? "（未运行的将自动启动）" : ""}`);
    if (missing > 0) parts.push(`${missing} 项找不到路径`);
    return `共 ${total} 项：${parts.join("，")}；恢复前会自动备份当前桌面，随时可撤销。`;
  }, [preview.data, relaunch]);

  const incompleteTargets = useMemo(() => {
    if (!report) return [];
    const set = new Set<string>();
    for (const s of report.steps) {
      if (s.status !== "success") set.add(s.targetName);
    }
    return [...set];
  }, [report]);

  const filteredReportSteps = useMemo(() => {
    if (!report) return [];
    if (reportFilter === "all") return report.steps;
    if (reportFilter === "failed")
      return report.steps.filter((s) => s.status === "failed" || s.status === "timeout");
    if (reportFilter === "skipped")
      return report.steps.filter((s) => s.status === "skipped" || s.status === "needsReview");
    return report.steps.filter((s) => s.status === "success");
  }, [report, reportFilter]);

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
        title="还没有可恢复的工作空间"
        description="先在「现场捕获」保存一个桌面方案，再来这里一键恢复。"
        action={
          <Button variant="primary" onClick={() => navigate("/capture")}>
            <Camera size={13} /> 前往捕获
          </Button>
        }
      />
    );
  }

  const reportCancelled = Boolean(report?.steps.some((s) => s.code === "cancelled"));
  const headline = report
    ? reportCancelled
      ? { title: "恢复已取消", tone: "warning" as const, icon: CircleSlash }
      : report.failed === 0 && report.skipped === 0
        ? { title: "全部恢复完成", tone: "success" as const, icon: CheckCircle2 }
        : report.succeeded > 0
          ? { title: "部分恢复完成", tone: "warning" as const, icon: AlertTriangle }
          : report.failed === 0
            ? { title: "步骤均已跳过", tone: "warning" as const, icon: CircleSlash }
            : { title: "恢复未成功", tone: "danger" as const, icon: XCircle }
    : null;

  return (
    <div className="mx-auto flex max-w-[1040px] flex-col gap-4 lg:flex-row">
      {/* 左侧空间列表 */}
      <aside className="flex w-full shrink-0 flex-col gap-2 lg:w-[228px]">
        {(workspaces.data ?? []).length > 4 ? (
          <SearchInput value={wsSearch} onChange={setWsSearch} placeholder="筛选工作空间…" />
        ) : null}
        <ul className="flex flex-col gap-1">
          {filteredWorkspaces.map((s: WorkspaceSummary) => {
            const isActive = s.id === active?.id;
            return (
              <li key={s.id}>
                <button
                  type="button"
                  disabled={running}
                  onClick={() => {
                    autoTriggeredFor.current = null;
                    setAutoPausedNote(null);
                    setParams({ workspace: s.id });
                    setReport(null);
                    setLiveSteps([]);
                    setError(null);
                  }}
                  className={`flex w-full cursor-pointer items-center justify-between gap-2 rounded-[10px] border px-3 py-2 text-left transition-all disabled:opacity-50 ${
                    isActive
                      ? "border-accent/45 bg-accent/12 text-ink"
                      : "border-line bg-surface hover:bg-raised/55"
                  }`}
                >
                  <div className="min-w-0 flex-1">
                    <p className="flex items-center gap-1.5 truncate text-body font-medium text-ink">
                      <WorkspaceColorDot color={s.color} />
                      {s.isFavorite ? (
                        <Star size={11} className="shrink-0 fill-warning text-warning" />
                      ) : null}
                      <span className="truncate">{s.name}</span>
                    </p>
                    <p className="mt-0.5 text-[11px] text-ink-3">
                      {s.itemCount} 软件 · {s.windowCount} 窗口
                    </p>
                  </div>
                  {isActive ? <Badge tone="accent">已选</Badge> : null}
                </button>
              </li>
            );
          })}
        </ul>
      </aside>

      {/* 右侧三态主体 */}
      <div className="flex min-w-0 flex-1 flex-col gap-3.5">
        {error ? <ErrorNote message={error} onDismiss={() => setError(null)} /> : null}

        {autoPausedNote && !report && !running ? (
          <div className="flex items-start gap-2 rounded-[10px] border border-warning/35 bg-warning/10 px-3.5 py-2.5 text-aux text-warning">
            <AlertTriangle size={14} className="mt-0.5 shrink-0" />
            <span>{autoPausedNote}</span>
          </div>
        ) : null}

        {/* 状态 A：恢复预览 */}
        {!report && !running ? (
          <>
            <Card className="flex flex-col gap-3">
              <div className="flex flex-wrap items-center justify-between gap-3">
                <div className="min-w-0">
                  <h2 className="truncate text-module font-semibold tracking-tight text-ink">
                    {active?.name}
                  </h2>
                  <p className="mt-0.5 text-aux text-ink-2">
                    {plainSummary ||
                      (active?.lastRestoredAt
                        ? `上次使用 ${formatRelative(active.lastRestoredAt)}`
                        : "正在检查当前桌面…")}
                  </p>
                </div>

                <div className="flex items-center gap-1.5">
                  {active ? (
                    <>
                      <Button
                        size="sm"
                        variant="ghost"
                        disabled={closeWindows.isPending}
                        onClick={async () => {
                          const ok = await confirmAction({
                            title: `收工：关闭「${active.name}」的窗口？`,
                            description: "仅向本空间已打开的窗口发送标准关闭请求，不会强杀进程。",
                            detail: "如有未保存文档，软件会正常弹出保存确认提示。",
                            confirmLabel: "发送关闭请求",
                          });
                          if (ok) closeWindows.mutate();
                        }}
                      >
                        <Power size={13} /> 收工
                      </Button>
                      <Button
                        size="sm"
                        variant="ghost"
                        onClick={() => navigate(`/workspaces?id=${active.id}`)}
                      >
                        <SlidersHorizontal size={13} /> 编辑
                      </Button>
                    </>
                  ) : null}
                  <Button
                    size="sm"
                    variant="secondary"
                    disabled={preview.isFetching}
                    onClick={() => preview.refetch()}
                  >
                    <RefreshCw
                      size={13}
                      className={preview.isFetching ? "animate-spin" : ""}
                    />
                    重新检查
                  </Button>
                  <Button
                    size="sm"
                    variant="primary"
                    disabled={preview.isPending || !preview.data || busy.data === true}
                    onClick={() => run.mutate()}
                  >
                    <Play size={13} className="fill-current" /> 开始恢复
                  </Button>
                </div>
              </div>

              <div className="flex flex-col gap-2.5 border-t border-line pt-2.5">
                <div className="flex items-center justify-between gap-4">
                  <div>
                    <p className="text-body font-medium text-ink">自动打开没启动的软件</p>
                    <p className="text-[11px] text-ink-3">
                      打开后自动帮你拉起还没运行的软件；关闭则只整理已打开的窗口
                    </p>
                  </div>
                  <Switch
                    checked={relaunch}
                    onChange={setRelaunch}
                    ariaLabel="自动打开没启动的软件"
                  />
                </div>

                <div className="flex items-center justify-between gap-4 border-t border-line/60 pt-2.5">
                  <div>
                    <p className="text-body font-medium text-ink">专注模式（收起无关窗口）</p>
                    <p className="text-[11px] text-ink-3">
                      自动把非本空间的窗口最小化到任务栏，一秒切换出干净桌面（绝不关闭任何软件）
                    </p>
                  </div>
                  <Switch
                    checked={focusMode}
                    onChange={async (next) => {
                      // 首次开启时说清后果：它会收起你其它窗口，虽然不关闭
                      if (next && !localStorage.getItem("workscene.focusModeAck")) {
                        const ok = await confirmAction({
                          title: "开启专注模式？",
                          description:
                            "恢复完成后，不属于这个工作空间的窗口会被最小化到任务栏（不会关闭任何软件，也不会丢数据）。它们需要你手动点回来。",
                          confirmLabel: "开启",
                        });
                        if (!ok) return;
                        localStorage.setItem("workscene.focusModeAck", "1");
                      }
                      setFocusMode(next);
                    }}
                    ariaLabel="专注模式"
                  />
                </div>
              </div>
            </Card>

            {preview.isError ? <ErrorNote message={String(preview.error.message)} /> : null}

            {preview.isPending ? (
              <Card className="flex items-center gap-2 text-aux text-ink-3">
                <Spinner /> 正在检查当前桌面…
              </Card>
            ) : preview.data ? (
              <>
                {preview.data.displayNotes.length > 0 ? (
                  <Card className="flex flex-col gap-1 border-warning/35 bg-warning/8 py-2.5 text-aux text-warning">
                    <p className="flex items-center gap-1.5 font-medium">
                      <Lightbulb size={13} /> 显示器有变化，系统会自动把窗口调整到当前主屏
                    </p>
                    {preview.data.displayNotes.map((n) => (
                      <p key={n} className="text-ink-2">
                        · {n}
                      </p>
                    ))}
                  </Card>
                ) : null}

                <Card padded={false} className="overflow-hidden">
                  <div className="flex items-center justify-between border-b border-line px-4 py-2 text-aux">
                    <span className="font-medium text-ink">
                      将要处理的项目 ({preview.data.items.length})
                    </span>
                    <button
                      type="button"
                      onClick={() => setShowTech((v) => !v)}
                      className="cursor-pointer text-[11px] text-accent hover:underline"
                    >
                      {showTech ? "隐藏技术细节" : "显示技术细节"}
                    </button>
                  </div>
                  <ul className="divide-y divide-line">
                    {preview.data.items.map((item, i) => {
                      const m = statusMeta[item.status];
                      const Icon = m.icon;
                      return (
                        <li key={i} className="flex items-start gap-2.5 px-4 py-2.5">
                          <Icon size={14} className={`mt-0.5 shrink-0 ${toneText(m.tone)}`} />
                          <div className="min-w-0 flex-1">
                            <p className="truncate text-body font-medium text-ink">
                              {item.targetName}
                            </p>
                            <p className="text-[11px] text-ink-3">{item.detail}</p>
                            {showTech ? (
                              <p className="mt-0.5 font-mono text-[10px] text-ink-3">
                                {item.matchNote}
                              </p>
                            ) : null}
                          </div>
                          <div className="flex shrink-0 items-center gap-1.5">
                            {item.status === "failed" && active ? (
                              <Button
                                size="xs"
                                variant="ghost"
                                onClick={() => navigate(`/workspaces?id=${active.id}`)}
                              >
                                去修复路径
                              </Button>
                            ) : null}
                            <Badge tone={m.tone}>{m.preview}</Badge>
                          </div>
                        </li>
                      );
                    })}
                  </ul>
                </Card>
              </>
            ) : null}
          </>
        ) : null}

        {/* 状态 B：执行中 */}
        {running ? (
          <Card className="flex flex-col gap-3.5">
            <div className="flex items-center justify-between gap-3">
              <div className="flex items-center gap-2.5">
                <Loader2 size={17} className="animate-spin text-accent" />
                <div>
                  <p className="text-card font-semibold text-ink">
                    正在帮你恢复「{active?.name}」…
                  </p>
                  <p className="tabnum text-aux text-ink-3">
                    已处理 {liveSteps.length} 项 · 用时 {(elapsedMs / 1000).toFixed(1)} 秒
                  </p>
                </div>
              </div>
              <Button
                size="sm"
                variant="danger"
                disabled={cancelling}
                onClick={() => cancelRun.mutate()}
              >
                {cancelling ? <Spinner /> : <CircleSlash size={13} />}
                {cancelling ? "正在停止…" : "取消恢复"}
              </Button>
            </div>
            <div className="h-1.5 overflow-hidden rounded-full bg-raised">
              <div
                className="h-full rounded-full bg-accent transition-[width] duration-200"
                style={{
                  width: `${Math.min(
                    100,
                    (liveSteps.length / Math.max(1, expectedSteps)) * 100,
                  )}%`,
                }}
              />
            </div>
            <div className="flex items-center gap-1.5 rounded-[8px] bg-canvas/70 px-3 py-2 text-[11px] text-ink-3">
              <ShieldCheck size={13} className="text-success" />
              {focusMode
                ? "放心，不会关闭你当前开着的任何软件；专注模式会将非本空间窗口安静最小化到任务栏。"
                : "放心，不会关闭你当前开着的任何软件，也不会乱抢窗口焦点。"}
            </div>
            <ul className="divide-y divide-line rounded-[10px] border border-line">
              {liveSteps.map((s, i) => (
                <StepRow key={i} step={s} showTech={showTech} />
              ))}
            </ul>
          </Card>
        ) : null}

        {/* 状态 C：恢复报告（支持「仅重试未完成项」+「单条重试」+「撤销」） */}
        {report && headline ? (
          <>
            <Card className="flex flex-col gap-3">
              <div className="flex flex-wrap items-center justify-between gap-3">
                <div className="flex items-center gap-2">
                  <headline.icon size={17} className={toneText(headline.tone)} />
                  <h2 className="text-card font-semibold text-ink">
                    {isUndoReport ? "已撤销上次恢复" : headline.title}
                  </h2>
                  <Badge tone={headline.tone}>
                    完成 {report.succeeded}/{report.totalSteps}
                  </Badge>
                </div>
                <div className="flex flex-wrap items-center gap-1.5">
                  {!isUndoReport && report.preRestoreSceneId ? (
                    <Button
                      size="sm"
                      variant="secondary"
                      disabled={undo.isPending}
                      onClick={async () => {
                        const ok = await confirmAction({
                          title: "撤销这次恢复？",
                          description: "将窗口还原到刚才恢复之前的样子。",
                          confirmLabel: "确认撤销",
                        });
                        if (ok) undo.mutate(report.preRestoreSceneId!);
                      }}
                    >
                      <Undo2 size={13} /> 撤销恢复
                    </Button>
                  ) : null}
                  {!isUndoReport && incompleteTargets.length > 0 && active ? (
                    <Button
                      size="sm"
                      variant="primary"
                      disabled={running || busy.data === true}
                      onClick={() => retryFailed.mutate(incompleteTargets)}
                    >
                      <RotateCw size={13} /> 仅重试未完成项 ({incompleteTargets.length})
                    </Button>
                  ) : (
                    <Button
                      size="sm"
                      variant="primary"
                      disabled={running || busy.data === true}
                      onClick={() => run.mutate()}
                    >
                      <RefreshCw size={13} /> {isUndoReport ? "重新恢复工作空间" : "再恢复一次"}
                    </Button>
                  )}
                  <Button
                    size="sm"
                    variant="ghost"
                    onClick={() => {
                      setReport(null);
                      setIsUndoReport(false);
                      preview.refetch();
                    }}
                  >
                    返回
                  </Button>
                </div>
              </div>
              <div className="grid grid-cols-3 gap-2.5">
                <MiniStat label="已完成" value={report.succeeded} tone="success" />
                <MiniStat label="已跳过" value={report.skipped} tone="warning" />
                <MiniStat label="未成功" value={report.failed} tone="danger" />
              </div>
            </Card>

            <Card padded={false} className="overflow-hidden">
              <div className="flex flex-wrap items-center justify-between gap-2 border-b border-line px-4 py-2">
                <div className="flex items-center gap-3">
                  <span className="text-aux font-medium text-ink">恢复结果明细</span>
                  <button
                    type="button"
                    onClick={() => setShowTech((v) => !v)}
                    className="cursor-pointer text-[11px] text-accent hover:underline"
                  >
                    {showTech ? "隐藏技术细节" : "显示技术细节"}
                  </button>
                </div>
                <div className="flex items-center gap-1 rounded-[8px] bg-raised/70 p-0.5 text-[11px]">
                  {(
                    [
                      { id: "all", label: `全部 ${report.steps.length}` },
                      { id: "failed", label: `未成功 ${report.failed}` },
                      { id: "skipped", label: `跳过 ${report.skipped}` },
                      { id: "success", label: `完成 ${report.succeeded}` },
                    ] as const
                  ).map((f) => (
                    <button
                      key={f.id}
                      type="button"
                      onClick={() => setReportFilter(f.id)}
                      className={`cursor-pointer rounded-[6px] px-2 py-0.5 ${
                        reportFilter === f.id
                          ? "apple-segment-active font-medium"
                          : "text-ink-3 hover:text-ink-2"
                      }`}
                    >
                      {f.label}
                    </button>
                  ))}
                </div>
              </div>
              <ul className="divide-y divide-line">
                {filteredReportSteps.map((s, i) => (
                  <StepRow
                    key={i}
                    step={s}
                    showTech={showTech}
                    onRetry={
                      !isUndoReport && active && s.status !== "success" && s.code !== "focus_mode"
                        ? () => retryFailed.mutate([s.targetName])
                        : undefined
                    }
                    onFixPath={
                      active && s.status !== "success"
                        ? () => navigate(`/workspaces?id=${active.id}`)
                        : undefined
                    }
                  />
                ))}
              </ul>
            </Card>
          </>
        ) : null}
      </div>
    </div>
  );
}

function StepRow({
  step,
  showTech,
  onRetry,
  onFixPath,
}: {
  step: RestoreStepResult;
  showTech: boolean;
  onRetry?: () => void;
  onFixPath?: () => void;
}) {
  const m = statusMeta[step.status];
  const Icon = m.icon;
  return (
    <li className="flex items-start gap-2.5 px-4 py-2.5">
      <Icon size={14} className={`mt-0.5 shrink-0 ${toneText(m.tone)}`} />
      <div className="min-w-0 flex-1">
        <div className="flex flex-wrap items-center gap-2">
          <p className="truncate text-body font-medium text-ink">{step.targetName}</p>
          <Badge tone={m.tone}>{m.report}</Badge>
          {showTech && step.code ? (
            <code className="rounded-[5px] bg-raised/80 px-1.5 font-mono text-[10px] text-ink-2">
              {step.code}
            </code>
          ) : null}
          <div className="ml-auto flex items-center gap-1.5">
            {onRetry ? (
              <button
                type="button"
                onClick={onRetry}
                className="cursor-pointer rounded-[6px] border border-line px-2 py-0.5 text-[11px] text-ink-2 hover:border-accent/40 hover:text-accent"
              >
                重试此项
              </button>
            ) : null}
            {onFixPath ? (
              <button
                type="button"
                onClick={onFixPath}
                className="cursor-pointer rounded-[6px] border border-line px-2 py-0.5 text-[11px] text-ink-3 hover:text-ink"
              >
                去修复
              </button>
            ) : null}
            {showTech ? (
              <span className="tabnum font-mono text-[11px] text-ink-3">
                {step.durationMs}ms
              </span>
            ) : null}
          </div>
        </div>
        <p className="mt-0.5 text-[11px] text-ink-2">{step.message}</p>
        {showTech && step.deltaX !== null && step.deltaY !== null ? (
          <p className="tabnum font-mono text-[10px] text-ink-3">
            偏差 Δ({step.deltaX}, {step.deltaY})px · {step.matchNote}
          </p>
        ) : null}
      </div>
    </li>
  );
}

function MiniStat({
  label,
  value,
  tone,
}: {
  label: string;
  value: number;
  tone: "success" | "warning" | "danger";
}) {
  return (
    <Card className="flex items-baseline justify-between px-3.5 py-2.5">
      <span className="text-aux text-ink-2">{label}</span>
      <span className={`tabnum text-[20px] font-semibold leading-none ${toneText(tone)}`}>
        {value}
      </span>
    </Card>
  );
}

function toneText(tone: "neutral" | "success" | "warning" | "danger" | "accent"): string {
  if (tone === "success") return "text-success";
  if (tone === "warning") return "text-warning";
  if (tone === "danger") return "text-danger";
  if (tone === "accent") return "text-accent";
  return "text-ink-3";
}
