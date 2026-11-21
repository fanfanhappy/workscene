import { useState } from "react";
import { useNavigate, useSearchParams } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  AppWindow,
  Check,
  MonitorSmartphone,
  Pencil,
  RotateCcw,
  Trash2,
  X,
} from "lucide-react";
import { api, formatRelative, formatTime } from "@/services/api";
import { Badge, Button, Card, EmptyState, ErrorNote, SectionTitle, Spinner } from "@/components/ui";

export function WorkspacesPage() {
  const navigate = useNavigate();
  const qc = useQueryClient();
  const [params, setParams] = useSearchParams();
  const [editingName, setEditingName] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const list = useQuery({
    queryKey: ["workspaces"],
    queryFn: api.workspaceList,
    staleTime: 2_000,
  });

  const activeId = params.get("id") ?? list.data?.[0]?.id ?? null;

  const detail = useQuery({
    queryKey: ["workspace", activeId],
    queryFn: () => api.workspaceGet(activeId!),
    enabled: Boolean(activeId),
    retry: 0,
  });

  const rename = useMutation({
    mutationFn: ({ id, name }: { id: string; name: string }) => api.workspaceRename(id, name),
    onSuccess: () => {
      setEditingName(null);
      qc.invalidateQueries({ queryKey: ["workspaces"] });
      qc.invalidateQueries({ queryKey: ["workspace", activeId] });
    },
    onError: (e) => setError(String((e as Error).message)),
  });

  const remove = useMutation({
    mutationFn: (id: string) => api.workspaceDelete(id),
    onSuccess: () => {
      setParams({});
      qc.invalidateQueries({ queryKey: ["workspaces"] });
    },
    onError: (e) => setError(String((e as Error).message)),
  });

  const saveItem = useMutation({
    mutationFn: (v: {
      itemId: string;
      reusePolicy: string;
      launchArgs: string | null;
      enabled: boolean;
    }) => api.workspaceItemUpdate(v.itemId, v.reusePolicy, v.launchArgs, v.enabled),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: ["workspace", activeId] });
    },
    onError: (e) => setError(String((e as Error).message)),
  });

  if (list.isPending) {
    return (
      <Card className="flex items-center gap-2 text-aux text-ink-3">
        <Spinner /> 读取工作空间…
      </Card>
    );
  }

  if ((list.data ?? []).length === 0) {
    return (
      <EmptyState
        icon={<AppWindow size={22} />}
        title="还没有工作空间"
        description="工作空间保存一套应用清单和窗口布局。先捕获当前桌面，再给它起个名字。"
        action={
          <Button variant="primary" onClick={() => navigate("/capture")}>
            去捕获
          </Button>
        }
      />
    );
  }

  return (
    <div className="mx-auto flex max-w-[1200px] gap-5">
      <aside className="w-[280px] shrink-0">
        <SectionTitle title="工作空间" hint={`${list.data!.length} 个`} />
        <ul className="flex flex-col gap-1.5">
          {(list.data ?? []).map((w) => {
            const active = w.id === activeId;
            return (
              <li key={w.id}>
                <button
                  onClick={() => setParams({ id: w.id })}
                  className={`w-full rounded-md border px-3 py-2.5 text-left transition-colors ${
                    active
                      ? "border-accent/50 bg-raised"
                      : "border-line bg-surface hover:bg-raised/60"
                  }`}
                >
                  <p className="truncate text-body font-medium">{w.name}</p>
                  <p className="mt-0.5 text-[11px] text-ink-3">
                    {w.itemCount} 应用 · {w.windowCount} 窗口
                  </p>
                </button>
              </li>
            );
          })}
        </ul>
      </aside>

      <div className="flex min-w-0 flex-1 flex-col gap-4">
        {error ? <ErrorNote message={error} /> : null}
        {detail.isError ? <ErrorNote message={String(detail.error.message)} /> : null}

        {detail.data ? (
          <>
            <Card className="flex flex-col gap-4">
              <div className="flex items-start justify-between gap-4">
                <div className="min-w-0 flex-1">
                  {editingName !== null ? (
                    <div className="flex items-center gap-2">
                      <input
                        autoFocus
                        value={editingName}
                        onChange={(e) => setEditingName(e.target.value)}
                        maxLength={80}
                        className="h-9 min-w-0 flex-1 rounded-md border border-line-strong bg-canvas px-3 text-card outline-none focus:border-accent"
                      />
                      <Button
                        size="sm"
                        variant="primary"
                        disabled={rename.isPending}
                        onClick={() =>
                          rename.mutate({ id: detail.data!.id, name: editingName })
                        }
                      >
                        <Check size={14} />
                      </Button>
                      <Button size="sm" variant="ghost" onClick={() => setEditingName(null)}>
                        <X size={14} />
                      </Button>
                    </div>
                  ) : (
                    <div className="flex items-center gap-2">
                      <h2 className="truncate text-module font-semibold tracking-tight">
                        {detail.data.name}
                      </h2>
                      <Button
                        size="sm"
                        variant="ghost"
                        title="重命名"
                        onClick={() => setEditingName(detail.data!.name)}
                      >
                        <Pencil size={13} />
                      </Button>
                    </div>
                  )}
                  <p className="mt-1 text-aux text-ink-3">
                    创建于 {formatTime(detail.data.createdAt)} ·{" "}
                    {detail.data.lastRestoredAt
                      ? `上次恢复 ${formatRelative(detail.data.lastRestoredAt)}`
                      : "尚未恢复过"}
                  </p>
                </div>
                <div className="flex shrink-0 gap-2">
                  <Button
                    size="sm"
                    variant="primary"
                    onClick={() => navigate(`/restore?workspace=${detail.data!.id}`)}
                  >
                    <RotateCcw size={14} />
                    恢复
                  </Button>
                  <Button
                    size="sm"
                    variant="danger"
                    disabled={remove.isPending}
                    onClick={() => {
                      if (confirm(`删除「${detail.data!.name}」及其全部快照？`))
                        remove.mutate(detail.data!.id);
                    }}
                  >
                    <Trash2 size={14} />
                  </Button>
                </div>
              </div>

              <dl className="grid grid-cols-3 gap-3 border-t border-line pt-3 text-aux">
                <div>
                  <dt className="text-ink-3">应用</dt>
                  <dd className="tabnum mt-0.5 text-card">{detail.data.items.length}</dd>
                </div>
                <div>
                  <dt className="text-ink-3">窗口</dt>
                  <dd className="tabnum mt-0.5 text-card">{detail.data.windows.length}</dd>
                </div>
                <div>
                  <dt className="text-ink-3">显示器</dt>
                  <dd className="tabnum mt-0.5 text-card">
                    {new Set(detail.data.windows.map((w) => w.monitorKey)).size}
                  </dd>
                </div>
              </dl>
            </Card>

            <Card padded={false} className="overflow-hidden">
              <div className="border-b border-line px-5 py-2.5 text-aux text-ink-2">
                应用与窗口
              </div>
              <ul className="divide-y divide-line/60">
                {detail.data.windows.map((w) => (
                  <li key={w.id} className="flex items-start gap-3 px-5 py-2.5">
                    <MonitorSmartphone size={15} className="mt-0.5 shrink-0 text-ink-3" />
                    <div className="min-w-0 flex-1">
                      <p className="truncate text-body">
                        {w.title || `${w.processName}（无标题窗口）`}
                      </p>
                      <p className="mt-0.5 font-mono text-[11px] text-ink-3">
                        {w.processName} · {w.className} · {w.monitorKey || "未知显示器"}
                      </p>
                      <p className="mt-0.5 font-mono text-[11px] text-ink-3">
                        {(w.xRatio * 100).toFixed(1)}% / {(w.yRatio * 100).toFixed(1)}% ·{" "}
                        {(w.widthRatio * 100).toFixed(1)}% × {(w.heightRatio * 100).toFixed(1)}%
                      </p>
                    </div>
                    {w.showState === "maximized" ? <Badge>最大化</Badge> : null}
                    {w.showState === "minimized" ? <Badge>最小化</Badge> : null}
                  </li>
                ))}
              </ul>
            </Card>

            <Card padded={false} className="overflow-hidden">
              <div className="border-b border-line px-5 py-2.5 text-aux text-ink-2">
                应用清单（复用策略与启动参数）
              </div>
              <ul className="divide-y divide-line/60">
                {detail.data.items.map((it) => (
                  <li key={it.id} className="flex flex-col gap-2 px-5 py-2.5">
                    <div className="flex items-center gap-3">
                      <span className="min-w-0 flex-1 truncate text-body">{it.displayName}</span>
                      <span className="max-w-[45%] truncate font-mono text-[11px] text-ink-3">
                        {it.target ?? "路径不可读"}
                      </span>
                    </div>
                    <div className="flex items-center gap-2">
                      <select
                        value={it.reusePolicy}
                        onChange={(e) =>
                          saveItem.mutate({
                            itemId: it.id,
                            reusePolicy: e.target.value,
                            launchArgs: it.launchArgs,
                            enabled: it.enabled,
                          })
                        }
                        className="h-7 rounded-sm border border-line-strong bg-canvas px-2 text-[11px] text-ink-2 outline-none focus:border-accent"
                      >
                        <option value="reuse_or_launch">缺几个补几个（默认）</option>
                        <option value="reuse_if_matched">只复用，不新启</option>
                        <option value="always_launch">每个窗口都新启</option>
                      </select>
                      <input
                        defaultValue={it.launchArgs ?? ""}
                        placeholder="启动参数，例如 --new-window"
                        onBlur={(e) => {
                          const value = e.target.value.trim();
                          if (value === (it.launchArgs ?? "")) return;
                          saveItem.mutate({
                            itemId: it.id,
                            reusePolicy: it.reusePolicy,
                            launchArgs: value === "" ? null : value,
                            enabled: it.enabled,
                          });
                        }}
                        className="h-7 min-w-0 flex-1 rounded-sm border border-line-strong bg-canvas px-2 font-mono text-[11px] outline-none focus:border-accent"
                      />
                    </div>
                  </li>
                ))}
              </ul>
            </Card>
          </>
        ) : null}
      </div>
    </div>
  );
}
