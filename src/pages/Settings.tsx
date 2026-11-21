import { useEffect, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { HardDrive, Info, Keyboard, RotateCcw, Sparkles, Trash2 } from "lucide-react";
import { api } from "@/services/api";
import type { AppSettings } from "@/types/contract";
import { Badge, Button, Card, ErrorNote, SectionTitle, Spinner } from "@/components/ui";

export function SettingsPage() {
  const qc = useQueryClient();
  const [error, setError] = useState<string | null>(null);
  const [draft, setDraft] = useState<AppSettings | null>(null);

  const settings = useQuery({ queryKey: ["settings"], queryFn: api.settingsGet });
  const stats = useQuery({ queryKey: ["storage-stats"], queryFn: api.storageStats });
  const info = useQuery({ queryKey: ["app-info"], queryFn: api.getAppInfo, staleTime: 60_000 });

  useEffect(() => {
    if (settings.data) setDraft(settings.data);
  }, [settings.data]);

  const save = useMutation({
    mutationFn: (next: AppSettings) => api.settingsSet(next),
    onSuccess: (saved) => {
      setDraft(saved);
      qc.invalidateQueries({ queryKey: ["settings"] });
    },
    onError: (e) => setError(String((e as Error).message)),
  });

  const cleanup = useMutation({
    mutationFn: api.storageCleanup,
    onSuccess: () => qc.invalidateQueries({ queryKey: ["storage-stats"] }),
    onError: (e) => setError(String((e as Error).message)),
  });

  const patch = (next: Partial<AppSettings>) => {
    if (!draft) return;
    const merged = { ...draft, ...next };
    setDraft(merged);
    save.mutate(merged);
  };

  if (settings.isPending || !draft) {
    return (
      <Card className="flex items-center gap-2 text-aux text-ink-3">
        <Spinner /> 读取设置…
      </Card>
    );
  }

  return (
    <div className="mx-auto flex max-w-[900px] flex-col gap-6">
      {error ? <ErrorNote message={error} /> : null}

      <section>
        <SectionTitle title="自动快照" hint="后台按指纹去重，只有在环境真的变化时才写库" />
        <Card className="flex flex-col gap-4">
          <Row
            label="启用自动快照"
            hint="关闭后不会再后台记录；手动快照与恢复前保护快照不受影响"
          >
            <Toggle
              checked={draft.autoSnapshotEnabled}
              onChange={(v) => patch({ autoSnapshotEnabled: v })}
            />
          </Row>

          <Row label="每个工作空间保留" hint="只淘汰最旧的自动快照，手动/保护快照永不清理">
            <NumberInput
              value={draft.autoKeep}
              min={1}
              max={500}
              suffix="份"
              onCommit={(v) => patch({ autoKeep: v })}
            />
          </Row>

          <Row label="最小间隔" hint="两次自动快照之间的最短时间，避免拖动窗口时连续写入">
            <NumberInput
              value={draft.autoMinIntervalSecs}
              min={10}
              max={86400}
              suffix="秒"
              onCommit={(v) => patch({ autoMinIntervalSecs: v })}
            />
          </Row>
        </Card>
      </section>

      <section>
        <SectionTitle title="启动" hint="开机启动写入当前用户的注册表 Run 项，不需要管理员权限" />
        <Card className="flex flex-col gap-4">
          <Row label="开机启动" hint="登录 Windows 后自动启动 WorkScene（最小化到托盘）">
            <Toggle
              checked={draft.autostart}
              onChange={(v) => patch({ autostart: v })}
            />
          </Row>
          {save.isError ? (
            <p className="text-[11px] text-danger">
              开机启动写入失败：{String((save.error as Error).message)}
            </p>
          ) : null}
        </Card>
      </section>

      <section>
        <SectionTitle title="恢复" hint="恢复过程不会关闭你已打开的应用" />
        <Card className="flex flex-col gap-4">
          <Row label="默认自动启动缺失的应用" hint="恢复前仍可在预览页逐次关闭">
            <Toggle
              checked={draft.restoreRelaunchDefault}
              onChange={(v) => patch({ restoreRelaunchDefault: v })}
            />
          </Row>
        </Card>
      </section>

      <section>
        <SectionTitle title="存储" hint="快照是不可变文件，数据库只存索引与报告" />
        <Card className="flex flex-col gap-4">
          <div className="grid grid-cols-2 gap-4 text-aux md:grid-cols-4">
            <Stat label="数据库" value={formatBytes(stats.data?.dbBytes ?? 0)} />
            <Stat label="快照文件" value={`${stats.data?.snapshotCount ?? 0} 个`} />
            <Stat label="快照占用" value={formatBytes(stats.data?.snapshotBytes ?? 0)} />
            <Stat label="恢复报告" value={`${stats.data?.runCount ?? 0} 条`} />
          </div>

          <div className="flex items-center justify-between gap-3 border-t border-line pt-3">
            <p className="min-w-0 flex-1 truncate font-mono text-[11px] text-ink-3">
              <HardDrive size={12} className="mr-1 inline" />
              {stats.data?.dataDir ?? "—"}
            </p>
            <Button
              size="sm"
              variant="secondary"
              disabled={cleanup.isPending}
              onClick={() => {
                if (confirm(`按保留策略清理旧自动快照（每空间保留 ${draft.autoKeep} 份）？`))
                  cleanup.mutate();
              }}
            >
              <Sparkles size={13} /> 清理旧自动快照
            </Button>
          </div>
          {cleanup.isSuccess ? (
            <p className="text-[11px] text-success">已清理 {cleanup.data} 份自动快照</p>
          ) : null}
        </Card>
      </section>

      <section>
        <SectionTitle title="快捷键" hint="当前为固定组合，暂不支持在界面里改" />
        <Card className="flex flex-col gap-3">
          <div className="flex items-center gap-3">
            <Keyboard size={16} className="text-ink-3" />
            <span className="text-body">唤起主窗口</span>
            <kbd className="rounded-sm border border-line-strong px-2 py-0.5 font-mono text-[11px] text-ink-2">
              Ctrl + Alt + W
            </kbd>
            <Badge>全局</Badge>
          </div>
          <p className="text-[11px] text-ink-3">
            修改需要在源码里改 `win::tray::spawn_hotkey` 后重新编译；如果组合被其它程序占用，注册会静默失败并写日志。
          </p>
        </Card>
      </section>

      <section>
        <SectionTitle title="关于" />
        <Card className="flex flex-col gap-3">
          <div className="flex items-center gap-2">
            <Info size={15} className="text-ink-3" />
            <span className="text-body">WorkScene {info.data?.version ?? "0.1.0"}</span>
            <Badge>Tauri {info.data?.tauriVersion ?? "2"}</Badge>
            <Badge>{info.data?.dpiAwareness ?? "Per-Monitor V2"}</Badge>
          </div>
          <p className="text-[11px] leading-relaxed text-ink-3">
            所有数据保存在本机，不上传任何桌面信息。不支持 UWP 打包应用的窗口恢复；最小化窗口会还原为普通窗口并在报告里注明。
          </p>
          <div className="flex gap-2">
            <Button size="sm" variant="ghost" onClick={() => qc.invalidateQueries()}>
              <RotateCcw size={13} /> 刷新
            </Button>
            <Button
              size="sm"
              variant="ghost"
              onClick={() => {
                if (confirm("打开数据目录？")) {
                  // 不引入 shell 插件：交给前端下载链接之外的方式展示路径即可
                  navigator.clipboard.writeText(stats.data?.dataDir ?? "").catch(() => undefined);
                }
              }}
              title="复制数据目录路径"
            >
              <Trash2 size={13} /> 复制数据目录
            </Button>
          </div>
        </Card>
      </section>
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
    <div className="flex items-start justify-between gap-6">
      <div className="min-w-0">
        <p className="text-body">{label}</p>
        {hint ? <p className="mt-0.5 text-[11px] text-ink-3">{hint}</p> : null}
      </div>
      <div className="shrink-0">{children}</div>
    </div>
  );
}

function Toggle({ checked, onChange }: { checked: boolean; onChange: (v: boolean) => void }) {
  return (
    <button
      role="switch"
      aria-checked={checked}
      onClick={() => onChange(!checked)}
      className={`relative h-6 w-11 rounded-full border transition-colors ${
        checked ? "border-accent bg-accent/80" : "border-line-strong bg-canvas"
      }`}
    >
      <span
        className={`absolute top-0.5 size-4.5 rounded-full bg-ink transition-[left] ${
          checked ? "left-[22px]" : "left-0.5"
        }`}
        style={{ width: 18, height: 18 }}
      />
    </button>
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

  return (
    <span className="flex items-center gap-2">
      <input
        value={text}
        onChange={(e) => setText(e.target.value)}
        onBlur={() => {
          const n = Number(text);
          if (!Number.isFinite(n)) {
            setText(String(value));
            return;
          }
          onCommit(Math.min(Math.max(Math.round(n), min), max));
        }}
        inputMode="numeric"
        className="h-8 w-20 rounded-sm border border-line-strong bg-canvas px-2 text-right text-body tabnum outline-none focus:border-accent"
      />
      <span className="text-aux text-ink-3">{suffix}</span>
    </span>
  );
}

function Stat({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <p className="text-[11px] text-ink-3">{label}</p>
      <p className="tabnum mt-0.5 text-card text-ink">{value}</p>
    </div>
  );
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}
