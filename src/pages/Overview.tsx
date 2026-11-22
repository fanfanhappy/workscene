import { useMemo } from "react";
import { useNavigate } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  Activity,
  AlertTriangle,
  AppWindow,
  ArrowRight,
  Camera,
  HardDrive,
  Monitor,
  Play,
  RefreshCw,
  RotateCcw,
  Settings,
  SlidersHorizontal,
  Star,
} from "lucide-react";
import { api, formatRelative, formatTime } from "@/services/api";
import type { DisplayInfo } from "@/types/contract";
import { toast } from "@/stores/ui.store";
import {
  Badge,
  Button,
  Card,
  EmptyState,
  ErrorNote,
  Metric,
  SectionTitle,
  Spinner,
} from "@/components/ui";
import { MonitorCanvas } from "@/components/MonitorCanvas";
import { AppIconStrip, useAppIcons, WorkspaceColorDot } from "@/components/AppIcon";

export function OverviewPage() {
  const navigate = useNavigate();
  const qc = useQueryClient();

  const capture = useQuery({
    queryKey: ["capture"],
    queryFn: api.captureStart,
    staleTime: 5_000,
    retry: 0,
  });

  const workspaces = useQuery({
    queryKey: ["workspaces"],
    queryFn: api.workspaceList,
    staleTime: 2_000,
  });

  const runs = useQuery({
    queryKey: ["runs", 5],
    queryFn: () => api.runList(5),
    staleTime: 5_000,
    retry: 0,
  });

  const settings = useQuery({
    queryKey: ["settings"],
    queryFn: api.settingsGet,
    staleTime: 15_000,
    retry: 0,
  });

  const storage = useQuery({
    queryKey: ["storage-stats"],
    queryFn: api.storageStats,
    staleTime: 15_000,
    retry: 0,
  });

  const health = useQuery({
    queryKey: ["scene-health"],
    queryFn: api.sceneHealth,
    staleTime: 30_000,
    retry: 0,
  });

  const hint = useQuery({
    queryKey: ["recovery-hint"],
    queryFn: api.recoveryHint,
    staleTime: 0,
    retry: 0,
  });

  const dismissHint = useMutation({
    mutationFn: api.recoveryDismiss,
    onSuccess: () => qc.invalidateQueries({ queryKey: ["recovery-hint"] }),
  });

  const recoverLast = useMutation({
    mutationFn: (snapshotId: string) => api.snapshotRestore(snapshotId, true),
    onSuccess: (report) => {
      dismissHint.mutate();
      qc.invalidateQueries({ queryKey: ["workspaces"] });
      qc.invalidateQueries({ queryKey: ["runs"] });
      if (report.failed === 0 && report.skipped === 0) {
        toast.success("已恢复上次现场", `共完成 ${report.succeeded} 项`);
      } else if (report.succeeded > 0) {
        toast.warning(
          "上次现场部分恢复",
          `成功 ${report.succeeded} · 跳过 ${report.skipped} · 失败 ${report.failed}`,
        );
      } else {
        toast.danger("上次现场恢复未成功", `失败 ${report.failed} · 跳过 ${report.skipped}`);
      }
    },
    onError: (e) => toast.danger("恢复失败", String((e as Error).message)),
  });

  const stats = useMemo(() => {
    const wins = capture.data?.windows ?? [];
    return {
      windows: wins.length,
      apps: new Set(wins.map((w) => w.processName.toLowerCase())).size,
    };
  }, [capture.data]);

  const wsMap = useMemo(() => {
    const m = new Map<string, string>();
    for (const w of workspaces.data ?? []) m.set(w.id, w.name);
    return m;
  }, [workspaces.data]);

  const recentWorkspaces = useMemo(
    () =>
      (workspaces.data ?? [])
        .slice()
        .sort(
          (a, b) =>
            (b.lastRestoredAt ?? b.updatedAt) - (a.lastRestoredAt ?? a.updatedAt),
        )
        .slice(0, 6),
    [workspaces.data],
  );

  const wsIconPaths = useMemo(
    () => recentWorkspaces.flatMap((w) => w.appPaths ?? []),
    [recentWorkspaces],
  );
  const appIcons = useAppIcons(wsIconPaths);

  return (
    <div className="mx-auto flex max-w-[1040px] flex-col gap-4">
      {/* 崩溃恢复横幅 */}
      {hint.data ? (
        <Card className="border-warning/35 bg-warning/8">
          <div className="flex flex-wrap items-center justify-between gap-3">
            <div className="min-w-0 flex-1">
              <p className="flex items-center gap-1.5 text-body font-semibold text-warning">
                <AlertTriangle size={14} />
                上次 WorkScene 未正常退出
              </p>
              <p className="mt-0.5 text-aux text-ink-2">
                {formatTime(hint.data.crashedAt)}
                {hint.data.workspaceName ? ` · 可恢复「${hint.data.workspaceName}」` : ""}
              </p>
            </div>
            <div className="flex shrink-0 items-center gap-2">
              {hint.data.snapshotId ? (
                <Button
                  size="sm"
                  variant="primary"
                  disabled={recoverLast.isPending}
                  onClick={() => recoverLast.mutate(hint.data!.snapshotId!)}
                >
                  <RotateCcw size={13} /> 恢复上次现场
                </Button>
              ) : null}
              <Button size="sm" variant="ghost" onClick={() => dismissHint.mutate()}>
                忽略
              </Button>
            </div>
          </div>
        </Card>
      ) : null}

      {/* 上半区：紧凑 Bento 网格（当前桌面 + 自动快照/显示器状态并排） */}
      <div className="grid gap-3.5 lg:grid-cols-[minmax(0,1.35fr)_minmax(0,0.65fr)]">
        <Card className="flex min-w-0 flex-col gap-3">
          <div className="flex items-center justify-between gap-3">
            <div className="flex items-center gap-2">
              <span className="text-card font-semibold text-ink">当前桌面</span>
              {capture.data ? (
                <Badge tone="success">{capture.data.durationMs} ms</Badge>
              ) : null}
            </div>
            <div className="flex items-center gap-1.5">
              <Button
                size="xs"
                variant="ghost"
                disabled={capture.isFetching}
                onClick={() => capture.refetch()}
              >
                <RefreshCw
                  size={12}
                  className={capture.isFetching ? "animate-spin" : ""}
                />
                刷新
              </Button>
              <Button size="xs" variant="tinted" onClick={() => navigate("/capture")}>
                <Camera size={12} /> 捕获现场
              </Button>
            </div>
          </div>

          {capture.isError ? <ErrorNote message={String(capture.error.message)} /> : null}

          <div className="flex items-end gap-6">
            <Metric label="顶层窗口" value={capture.isPending ? "—" : stats.windows} />
            <Metric label="运行应用" value={capture.isPending ? "—" : stats.apps} />
            <Metric
              label="显示器"
              value={capture.isPending ? "—" : (capture.data?.displays.length ?? 0)}
            />
          </div>

          {capture.data ? (
            <MonitorCanvas
              displays={capture.data.displays}
              windows={capture.data.windows}
              height={145}
            />
          ) : (
            <div className="flex items-center gap-2 py-8 text-aux text-ink-3">
              <Spinner /> 扫描桌面窗口…
            </div>
          )}
        </Card>

        <div className="flex min-w-0 flex-col gap-3.5">
          <Card className="flex flex-col gap-2.5">
            <div className="flex items-center justify-between gap-2">
              <span className="text-card font-semibold text-ink">自动快照与存储</span>
              <Badge tone={settings.data?.autoSnapshotEnabled ? "success" : "neutral"}>
                {settings.data?.autoSnapshotEnabled ? "已开启" : "已关闭"}
              </Badge>
            </div>
            <div className="grid grid-cols-3 gap-2">
              <div className="rounded-[8px] bg-canvas/70 px-2.5 py-1.5">
                <p className="text-[10px] text-ink-3">快照</p>
                <p className="tabnum text-body font-semibold text-ink">
                  {storage.data?.snapshotCount ?? 0} 份
                </p>
              </div>
              <div className="rounded-[8px] bg-canvas/70 px-2.5 py-1.5">
                <p className="text-[10px] text-ink-3">占用</p>
                <p className="tabnum text-body font-semibold text-ink">
                  {formatBytes(
                    (storage.data?.dbBytes ?? 0) + (storage.data?.snapshotBytes ?? 0),
                  )}
                </p>
              </div>
              <div className="rounded-[8px] bg-canvas/70 px-2.5 py-1.5">
                <p className="text-[10px] text-ink-3">报告</p>
                <p className="tabnum text-body font-semibold text-ink">
                  {storage.data?.runCount ?? 0} 条
                </p>
              </div>
            </div>
            <div className="flex items-center justify-between gap-2 text-[11px] text-ink-3">
              <span className="flex min-w-0 items-center gap-1 truncate font-mono">
                <HardDrive size={11} className="shrink-0" />
                <span className="truncate">{storage.data?.dataDir ?? "本机目录"}</span>
              </span>
              <button
                type="button"
                onClick={() => navigate("/settings")}
                className="inline-flex shrink-0 cursor-pointer items-center gap-1 text-accent hover:underline"
              >
                <Settings size={11} /> 设置
              </button>
            </div>
          </Card>

          <Card className="flex flex-1 flex-col gap-2">
            <span className="text-card font-semibold text-ink">显示器</span>
            {capture.isPending && !capture.data ? (
              <div className="flex items-center gap-2 py-3 text-aux text-ink-3">
                <Spinner /> 检测显示器中…
              </div>
            ) : (capture.data?.displays ?? []).length === 0 ? (
              <p className="py-3 text-aux text-ink-3">暂未读取到显示器信息</p>
            ) : (
              <ul className="flex flex-col gap-1.5">
                {(capture.data?.displays ?? []).map((d: DisplayInfo) => (
                  <li
                    key={d.monitorKey}
                    className="flex items-center gap-2.5 rounded-[8px] bg-canvas/70 px-2.5 py-2"
                  >
                    <Monitor size={14} className="shrink-0 text-accent" />
                    <div className="min-w-0 flex-1">
                      <div className="flex items-center gap-1.5">
                        <span className="text-aux font-medium text-ink">
                          {d.isPrimary ? "主显示器" : "副显示器"}
                        </span>
                        <Badge>{Math.round(d.scaleFactor * 100)}%</Badge>
                      </div>
                      <p className="tabnum font-mono text-[11px] text-ink-3">
                        {d.width}×{d.height} · 工作区 {d.workWidth}×{d.workHeight}
                      </p>
                    </div>
                  </li>
                ))}
              </ul>
            )}
          </Card>
        </div>
      </div>

      {/* 下半区：最近工作空间 + 最近恢复活动并排 */}
      <div className="grid gap-3.5 lg:grid-cols-[minmax(0,1.35fr)_minmax(0,0.65fr)]">
        <section className="min-w-0">
          <SectionTitle
            title="最近工作空间"
            action={
              workspaces.data?.length ? (
                <Button size="xs" variant="ghost" onClick={() => navigate("/workspaces")}>
                  全部 ({workspaces.data.length}) <ArrowRight size={12} />
                </Button>
              ) : null
            }
          />
          {workspaces.isPending ? (
            <Card className="flex items-center gap-2 text-aux text-ink-3">
              <Spinner /> 读取中…
            </Card>
          ) : (workspaces.data ?? []).length === 0 ? (
            <EmptyState
              icon={<AppWindow size={20} />}
              title="3 秒开始使用 WorkScene"
              description="先排好你常用的软件窗口并点「开始捕获」，以后一键就能把软件打开并摆回原位；每次恢复前都会自动备份，随时可撤销。"
              action={
                <Button size="sm" variant="primary" onClick={() => navigate("/capture")}>
                  <Camera size={13} /> 捕获第一个工作空间
                </Button>
              }
            />
          ) : (
            <div className="grid gap-2.5 sm:grid-cols-2">
              {recentWorkspaces.map((w) => (
                <Card
                  key={w.id}
                  interactive
                  onClick={() => navigate(`/workspaces?id=${w.id}`)}
                  className="flex flex-col justify-between gap-2.5 p-3"
                >
                  <div className="min-w-0">
                    <div className="flex items-center justify-between gap-2">
                      <h3 className="flex items-center gap-1.5 truncate text-body font-semibold text-ink">
                        <WorkspaceColorDot color={w.color} />
                        {w.isFavorite ? (
                          <Star size={12} className="shrink-0 fill-warning text-warning" />
                        ) : null}
                        <span className="truncate">{w.name}</span>
                        {w.hotkeySlot && settings.data?.workspaceHotkeysEnabled !== false ? (
                          <kbd
                            title={`全局快捷键 Ctrl+Alt+${w.hotkeySlot}`}
                            className="shrink-0 rounded-[5px] border border-line bg-canvas px-1 py-0.2 font-mono text-[10px] font-medium text-ink-3"
                          >
                            Ctrl+Alt+{w.hotkeySlot}
                          </kbd>
                        ) : null}
                      </h3>
                      <span className="shrink-0 text-[11px] text-ink-3">
                        {formatRelative(w.lastRestoredAt ?? w.createdAt)}
                      </span>
                    </div>
                    <p className="mt-1 text-[11px] text-ink-3">
                      {w.itemCount} 软件 · {w.resourceCount} 文件/网址 · {w.windowCount} 窗口
                    </p>
                    {(w.appPaths ?? []).length > 0 ? (
                      <div className="mt-2">
                        <AppIconStrip
                          paths={w.appPaths}
                          icons={appIcons}
                          totalApps={w.itemCount}
                        />
                      </div>
                    ) : null}
                  </div>
                  <div
                    className="flex items-center justify-between gap-2 border-t border-line pt-2"
                    onClick={(e) => e.stopPropagation()}
                  >
                    <Button
                      size="xs"
                      variant="primary"
                      onClick={() => navigate(`/restore?workspace=${w.id}&auto=1`)}
                    >
                      <Play size={11} className="fill-current" /> 一键恢复
                    </Button>
                    <Button
                      size="xs"
                      variant="ghost"
                      onClick={() => navigate(`/workspaces?id=${w.id}`)}
                    >
                      <SlidersHorizontal size={12} /> 编辑
                    </Button>
                  </div>
                </Card>
              ))}
            </div>
          )}
        </section>

        <section className="flex flex-col">
          <SectionTitle
            title="最近恢复活动"
            action={
              <Button size="xs" variant="ghost" onClick={() => navigate("/history")}>
                记录 <ArrowRight size={12} />
              </Button>
            }
          />
          <Card padded={false} className="flex-1 overflow-hidden">
            {runs.isPending ? (
              <div className="flex items-center gap-2 p-4 text-aux text-ink-3">
                <Spinner /> 读取中…
              </div>
            ) : (runs.data ?? []).length === 0 ? (
              <div className="flex flex-col items-center gap-1.5 px-4 py-8 text-center">
                <Activity size={16} className="text-ink-3" />
                <p className="text-aux text-ink-3">暂无恢复记录</p>
              </div>
            ) : (
              <ul className="divide-y divide-line">
                {(runs.data ?? []).map((r) => {
                  const tone =
                    r.status === "completed"
                      ? "success"
                      : r.status === "partial" || r.status === "cancelled"
                        ? "warning"
                        : "danger";
                  const label =
                    r.status === "completed"
                      ? "成功"
                      : r.status === "partial"
                        ? "部分"
                        : r.status === "cancelled"
                          ? "取消"
                          : "失败";
                  return (
                    <li key={r.id}>
                      <button
                        type="button"
                        onClick={() =>
                          navigate(
                            r.workspaceId
                              ? `/history?workspace=${r.workspaceId}`
                              : "/history",
                          )
                        }
                        className="flex w-full cursor-pointer items-center gap-2.5 px-3.5 py-2.5 text-left transition-colors hover:bg-raised/50"
                      >
                        <Badge tone={tone}>{label}</Badge>
                        <div className="min-w-0 flex-1">
                          <p className="truncate text-aux font-medium text-ink">
                            {(r.workspaceId && wsMap.get(r.workspaceId)) || "历史快照"}
                          </p>
                          <p className="tabnum text-[10px] text-ink-3">
                            {r.succeeded} 成 · {r.failed} 败 · {r.skipped} 跳
                          </p>
                        </div>
                        <span className="shrink-0 text-[10px] text-ink-3">
                          {formatRelative(r.startedAt)}
                        </span>
                      </button>
                    </li>
                  );
                })}
              </ul>
            )}
          </Card>
        </section>
      </div>

      {(health.data?.corrupted.length ?? 0) > 0 ? (
        <div className="rounded-[10px] border border-warning/35 bg-warning/8 px-3.5 py-2 text-aux text-warning">
          检测到 {health.data!.corrupted.length} 个损坏的快照文件（已隔离）。
        </div>
      ) : null}
    </div>
  );
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}
