import { useMemo, useState } from "react";
import { useSearchParams } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  ArrowRightLeft,
  Camera,
  RotateCcw,
  ShieldCheck,
  Sparkles,
  Trash2,
} from "lucide-react";
import { api, formatRelative, formatTime } from "@/services/api";
import type { SnapshotDiff, SnapshotSummary } from "@/types/contract";
import { Badge, Button, Card, EmptyState, ErrorNote, SectionTitle, Spinner } from "@/components/ui";

const typeMeta: Record<string, { label: string; tone: "neutral" | "accent" | "success" | "warning" }> = {
  initial: { label: "初始", tone: "neutral" },
  manual: { label: "手动", tone: "accent" },
  auto: { label: "自动", tone: "success" },
  protection: { label: "恢复前保护", tone: "warning" },
};

export function HistoryPage() {
  const qc = useQueryClient();
  const [params, setParams] = useSearchParams();
  const [picked, setPicked] = useState<string[]>([]);
  const [diff, setDiff] = useState<SnapshotDiff | null>(null);
  const [error, setError] = useState<string | null>(null);

  const workspaces = useQuery({
    queryKey: ["workspaces"],
    queryFn: api.workspaceList,
    staleTime: 2_000,
  });

  const activeId = params.get("workspace") ?? workspaces.data?.[0]?.id ?? null;

  const list = useQuery({
    queryKey: ["snapshots", activeId],
    queryFn: () => api.snapshotList(activeId!, 80),
    enabled: Boolean(activeId),
    retry: 0,
  });

  const refresh = () => {
    qc.invalidateQueries({ queryKey: ["snapshots", activeId] });
    qc.invalidateQueries({ queryKey: ["workspaces"] });
  };

  const create = useMutation({
    mutationFn: () => api.snapshotCreate(activeId!, "手动快照"),
    onSuccess: refresh,
    onError: (e) => setError(String((e as Error).message)),
  });

  const remove = useMutation({
    mutationFn: (id: string) => api.snapshotDelete(id),
    onSuccess: () => {
      setPicked([]);
      setDiff(null);
      refresh();
    },
    onError: (e) => setError(String((e as Error).message)),
  });

  const prune = useMutation({
    mutationFn: () => api.snapshotPrune(activeId!, 5),
    onSuccess: refresh,
    onError: (e) => setError(String((e as Error).message)),
  });

  const compare = useMutation({
    mutationFn: () => api.snapshotDiff(picked[0]!, picked[1]!),
    onSuccess: (d) => setDiff(d),
    onError: (e) => setError(String((e as Error).message)),
  });

  const restore = useMutation({
    mutationFn: (id: string) => api.snapshotRestore(id, false),
    onSuccess: () => refresh(),
    onError: (e) => setError(String((e as Error).message)),
  });

  const toggle = (id: string) => {
    setDiff(null);
    setPicked((prev) => {
      if (prev.includes(id)) return prev.filter((x) => x !== id);
      const next = [...prev, id];
      return next.slice(-2); // 只保留最近选中的两个
    });
  };

  const snapshots = useMemo(
    () => (list.data ?? []).slice().sort((a, b) => b.capturedAt - a.capturedAt),
    [list.data],
  );

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
        title="还没有工作空间"
        description="历史快照按工作空间归属。先捕获一个工作空间，再回来看历史。"
      />
    );
  }

  return (
    <div className="mx-auto flex max-w-[1200px] gap-5">
      <aside className="w-[240px] shrink-0">
        <SectionTitle title="工作空间" />
        <ul className="flex flex-col gap-1.5">
          {(workspaces.data ?? []).map((w) => (
            <li key={w.id}>
              <button
                onClick={() => {
                  setParams({ workspace: w.id });
                  setPicked([]);
                  setDiff(null);
                }}
                className={`w-full rounded-md border px-3 py-2 text-left text-body ${
                  w.id === activeId
                    ? "border-accent/50 bg-raised"
                    : "border-line bg-surface hover:bg-raised/60"
                }`}
              >
                {w.name}
              </button>
            </li>
          ))}
        </ul>
      </aside>

      <div className="flex min-w-0 flex-1 flex-col gap-4">
        {error ? <ErrorNote message={error} /> : null}

        <Card className="flex items-center justify-between gap-3">
          <div>
            <p className="text-body font-medium">快照时间线</p>
            <p className="mt-0.5 text-[11px] text-ink-3">
              自动快照按指纹去重并节流；保护快照与手动快照不参与清理
            </p>
          </div>
          <div className="flex flex-wrap gap-2">
            <Button size="sm" variant="secondary" onClick={() => create.mutate()} disabled={create.isPending}>
              <Camera size={13} /> 立即快照
            </Button>
            <Button
              size="sm"
              variant="secondary"
              disabled={picked.length !== 2 || compare.isPending}
              onClick={() => compare.mutate()}
              title="选择两份快照后比较"
            >
              <ArrowRightLeft size={13} /> 比较
            </Button>
            <Button
              size="sm"
              variant="ghost"
              disabled={prune.isPending}
              title="只保留最近 5 份自动快照"
              onClick={() => {
                if (confirm("清理旧的自动快照（保留最近 5 份，手动/保护快照不动）？"))
                  prune.mutate();
              }}
            >
              <Sparkles size={13} /> 清理自动快照
            </Button>
          </div>
        </Card>

        {list.isPending ? (
          <Card className="flex items-center gap-2 text-aux text-ink-3">
            <Spinner /> 读取快照…
          </Card>
        ) : snapshots.length === 0 ? (
          <EmptyState
            title="这个工作空间还没有快照"
            description="每次捕获或恢复都会留下快照；恢复前还会自动生成一份保护快照。"
          />
        ) : (
          <Card padded={false} className="overflow-hidden">
            <ul className="divide-y divide-line/60">
              {snapshots.map((s: SnapshotSummary) => {
                const meta = typeMeta[s.snapshotType] ?? { label: s.snapshotType, tone: "neutral" as const };
                const isPicked = picked.includes(s.id);
                return (
                  <li key={s.id} className="flex items-center gap-3 px-5 py-2.5">
                    <button
                      onClick={() => toggle(s.id)}
                      className={`grid size-4 shrink-0 place-items-center rounded-sm border ${
                        isPicked ? "border-accent bg-accent text-accent-ink" : "border-line-strong"
                      }`}
                      aria-label="选择用于比较"
                      title="选择两份快照进行比较"
                    />
                    <div className="min-w-0 flex-1">
                      <p className="flex items-center gap-2 truncate text-body">
                        {s.name}
                        <Badge tone={meta.tone}>{meta.label}</Badge>
                        {s.isProtected ? (
                          <span title="受保护，不会被清理">
                            <ShieldCheck size={13} className="text-warning" />
                          </span>
                        ) : null}
                      </p>
                      <p className="mt-0.5 text-[11px] text-ink-3">
                        {formatTime(s.capturedAt)} · {formatRelative(s.capturedAt)} ·{" "}
                        {s.windowCount} 窗口 · {s.displayCount} 显示器
                      </p>
                    </div>
                    <Button
                      size="sm"
                      variant="secondary"
                      disabled={restore.isPending}
                      onClick={() => {
                        if (confirm(`按这份快照恢复？会先自动留存一份保护快照。`))
                          restore.mutate(s.id);
                      }}
                    >
                      <RotateCcw size={13} /> 恢复
                    </Button>
                    <Button
                      size="sm"
                      variant="ghost"
                      disabled={remove.isPending}
                      onClick={() => {
                        if (confirm("删除这份快照？不可撤销。")) remove.mutate(s.id);
                      }}
                    >
                      <Trash2 size={13} />
                    </Button>
                  </li>
                );
              })}
            </ul>
          </Card>
        )}

        {diff ? <DiffPanel diff={diff} onClose={() => setDiff(null)} /> : null}
      </div>
    </div>
  );
}

function DiffPanel({ diff, onClose }: { diff: SnapshotDiff; onClose: () => void }) {
  const rows = [
    { label: "新增", value: diff.added },
    { label: "移除", value: diff.removed },
    { label: "移动", value: diff.moved },
    { label: "状态变化", value: diff.stateChanged },
    { label: "未变", value: diff.unchanged },
  ];

  return (
    <Card className="flex flex-col gap-3">
      <div className="flex items-center justify-between">
        <div>
          <p className="text-body font-medium">快照差异</p>
          <p className="mt-0.5 text-[11px] text-ink-3">{diff.displayDetail}</p>
        </div>
        <Button size="sm" variant="ghost" onClick={onClose}>
          关闭
        </Button>
      </div>

      <div className="flex flex-wrap gap-2">
        {rows.map((r) => (
          <span
            key={r.label}
            className="rounded-sm border border-line px-2 py-1 text-[11px] text-ink-2"
          >
            {r.label} <span className="tabnum text-ink">{r.value}</span>
          </span>
        ))}
      </div>

      {diff.windows.length === 0 ? (
        <p className="text-aux text-ink-3">两份快照的窗口完全一致。</p>
      ) : (
        <ul className="flex flex-col divide-y divide-line/60">
          {diff.windows.map((w, i) => (
            <li key={`${w.change}-${i}`} className="flex items-start gap-3 py-2">
              <Badge
                tone={
                  w.change === "added"
                    ? "success"
                    : w.change === "removed"
                      ? "danger"
                      : w.change === "moved"
                        ? "accent"
                        : "warning"
                }
              >
                {w.change === "added"
                  ? "新增"
                  : w.change === "removed"
                    ? "移除"
                    : w.change === "moved"
                      ? "移动"
                      : "状态"}
              </Badge>
              <div className="min-w-0 flex-1">
                <p className="truncate text-body">{w.titleB || w.titleA}</p>
                <p className="mt-0.5 font-mono text-[11px] text-ink-3">{w.detail}</p>
              </div>
            </li>
          ))}
        </ul>
      )}
    </Card>
  );
}
