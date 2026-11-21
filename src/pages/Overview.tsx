import { useMemo } from "react";
import { useNavigate } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { AppWindow, AlertTriangle, Camera, Monitor, RotateCcw, Trash2 } from "lucide-react";
import { api, formatRelative, formatTime } from "@/services/api";
import type { DisplayInfo } from "@/types/contract";
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

  const health = useQuery({
    queryKey: ["scene-health"],
    queryFn: api.sceneHealth,
    staleTime: 30_000,
  });

  // 崩溃恢复提示：上次没有正常退出时，后端会在数据目录留下 hint
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
    mutationFn: (snapshotId: string) => api.snapshotRestore(snapshotId, false),
    onSuccess: () => {
      dismissHint.mutate();
      qc.invalidateQueries({ queryKey: ["workspaces"] });
    },
  });

  const remove = useMutation({
    mutationFn: (id: string) => api.workspaceDelete(id),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["workspaces"] }),
  });

  const stats = useMemo(() => {
    const windows = capture.data?.windows ?? [];
    return {
      windows: windows.length,
      apps: new Set(windows.map((w) => w.processName.toLowerCase())).size,
    };
  }, [capture.data]);

  return (
    <div className="mx-auto flex max-w-[1100px] flex-col gap-6">
      {hint.data ? (
        <Card className="border-warning/40 bg-warning/5">
          <div className="flex items-start justify-between gap-4">
            <div>
              <p className="flex items-center gap-2 text-body font-medium text-warning">
                <AlertTriangle size={15} />
                上次 WorkScene 没有正常退出
              </p>
              <p className="mt-1 text-aux text-ink-2">
                时间：{formatTime(hint.data.crashedAt)}
                {hint.data.snapshotId
                  ? ` · 可以恢复到崩溃前最近一份快照${
                      hint.data.workspaceName ? `（${hint.data.workspaceName}）` : ""
                    }${hint.data.snapshotAt ? `，捕获于 ${formatTime(hint.data.snapshotAt)}` : ""}`
                  : " · 没有找到可恢复的快照"}
              </p>
            </div>
            <div className="flex shrink-0 gap-2">
              {hint.data.snapshotId ? (
                <Button
                  size="sm"
                  variant="primary"
                  disabled={recoverLast.isPending}
                  onClick={() => recoverLast.mutate(hint.data!.snapshotId!)}
                >
                  <RotateCcw size={14} />
                  恢复上次现场
                </Button>
              ) : null}
              <Button size="sm" variant="ghost" onClick={() => dismissHint.mutate()}>
                忽略
              </Button>
            </div>
          </div>
        </Card>
      ) : null}

      <Card className="flex flex-col gap-5">
        <div className="flex items-start justify-between gap-4">
          <div>
            <p className="text-[11px] font-medium uppercase tracking-widest text-ink-3">
              Current Desktop
            </p>
            <p className="mt-1 text-card text-ink-2">
              {capture.isPending
                ? "正在扫描当前桌面…"
                : capture.isError
                  ? "无法读取当前桌面"
                  : "当前桌面已就绪，可以捕获为工作空间"}
            </p>
          </div>
          <Button variant="primary" onClick={() => navigate("/capture")}>
            <Camera size={15} />
            捕获当前现场
          </Button>
        </div>

        {capture.isError ? <ErrorNote message={String(capture.error.message)} /> : null}

        <div className="flex flex-wrap items-end gap-8 border-t border-line pt-4">
          <Metric label="窗口" value={capture.isPending ? "—" : stats.windows} />
          <Metric label="应用" value={capture.isPending ? "—" : stats.apps} />
          <Metric
            label="显示器"
            value={capture.isPending ? "—" : (capture.data?.displays.length ?? 0)}
          />
          {capture.data ? (
            <div className="ml-auto flex items-center gap-2 text-aux text-ink-3">
              <Badge tone="success">扫描完成</Badge>
              <span className="tabnum">耗时 {capture.data.durationMs} ms</span>
            </div>
          ) : null}
        </div>

        {capture.data ? (
          <MonitorCanvas
            displays={capture.data.displays}
            windows={capture.data.windows}
            height={180}
          />
        ) : null}
      </Card>

      <section>
        <SectionTitle
          title="显示器"
          hint="窗口恢复按显示器工作区计算比例坐标，显示器变化时自动重新映射"
        />
        <div className="grid gap-3 md:grid-cols-2 xl:grid-cols-3">
          {(capture.data?.displays ?? []).map((d: DisplayInfo) => (
            <Card key={d.monitorKey} className="flex items-start gap-3">
              <Monitor size={18} className="mt-0.5 text-ink-3" />
              <div className="min-w-0">
                <div className="flex items-center gap-2">
                  <p className="text-body font-medium">
                    {d.isPrimary ? "主显示器" : "副显示器"}
                  </p>
                  <Badge>{Math.round(d.scaleFactor * 100)}%</Badge>
                </div>
                <p className="mt-1 font-mono text-aux text-ink-3">
                  {d.width}×{d.height} · 工作区 {d.workWidth}×{d.workHeight}
                </p>
              </div>
            </Card>
          ))}
          {capture.isPending ? (
            <Card className="flex items-center gap-2 text-aux text-ink-3">
              <Spinner /> 读取显示器…
            </Card>
          ) : null}
        </div>
      </section>

      <section>
        <SectionTitle
          title="工作空间"
          hint="每个工作空间保存一套应用清单与窗口布局，恢复前会自动留存保护快照"
          action={
            workspaces.data?.length ? (
              <Button size="sm" variant="ghost" onClick={() => navigate("/workspaces")}>
                全部工作空间
              </Button>
            ) : null
          }
        />

        {workspaces.isPending ? (
          <Card className="flex items-center gap-2 text-aux text-ink-3">
            <Spinner /> 读取工作空间…
          </Card>
        ) : (workspaces.data ?? []).length === 0 ? (
          <EmptyState
            icon={<AppWindow size={22} />}
            title="还没有保存任何工作空间"
            description="打开需要记录的应用与窗口，调整好布局，然后捕获为工作空间。"
            action={
              <Button variant="primary" onClick={() => navigate("/capture")}>
                <Camera size={15} />
                开始捕获
              </Button>
            }
          />
        ) : (
          <ul className="flex flex-col gap-2">
            {(workspaces.data ?? []).slice(0, 5).map((w) => (
              <li key={w.id}>
                <Card className="flex items-center gap-4">
                  <div className="min-w-0 flex-1">
                    <p className="truncate text-body font-medium">{w.name}</p>
                    <p className="mt-0.5 text-aux text-ink-3">
                      {w.itemCount} 个应用 · {w.resourceCount} 个资源 · {w.windowCount} 个窗口 ·{" "}
                      {w.lastRestoredAt
                        ? `上次恢复 ${formatRelative(w.lastRestoredAt)}`
                        : `创建于 ${formatRelative(w.createdAt)}`}
                    </p>
                  </div>
                  <Button
                    size="sm"
                    variant="secondary"
                    onClick={() => navigate(`/restore?workspace=${w.id}`)}
                  >
                    <RotateCcw size={14} />
                    恢复
                  </Button>
                  <Button
                    size="sm"
                    variant="danger"
                    title="删除该工作空间及其快照"
                    disabled={remove.isPending}
                    onClick={() => {
                      if (confirm(`删除工作空间「${w.name}」及其所有快照？此操作不可撤销。`))
                        remove.mutate(w.id);
                    }}
                  >
                    <Trash2 size={14} />
                  </Button>
                </Card>
              </li>
            ))}
          </ul>
        )}
        {remove.isError ? <ErrorNote message={String(remove.error.message)} /> : null}

        {(health.data?.corrupted.length ?? 0) > 0 ? (
          <div className="mt-3 rounded-md border border-warning/40 bg-warning/5 px-3 py-2 text-aux text-warning">
            <p className="font-medium">
              {health.data!.corrupted.length} 个快照文件已损坏，未出现在列表中
            </p>
            <ul className="mt-1 flex flex-col gap-0.5 font-mono text-[11px]">
              {health.data!.corrupted.slice(0, 5).map((c) => (
                <li key={c.file}>
                  {c.file} — {c.reason}
                </li>
              ))}
            </ul>
          </div>
        ) : null}
      </section>

      {capture.data && capture.data.diagnostics.length > 0 ? (
        <details className="rounded-lg border border-line bg-surface/50 px-4 py-3">
          <summary className="cursor-pointer text-aux text-ink-3">捕获诊断信息</summary>
          <ul className="mt-2 flex flex-col gap-1 font-mono text-[11px] text-ink-3">
            {capture.data.diagnostics.map((d) => (
              <li key={d}>{d}</li>
            ))}
          </ul>
        </details>
      ) : null}
    </div>
  );
}
