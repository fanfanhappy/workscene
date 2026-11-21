import { useEffect, useMemo, useState } from "react";
import { useNavigate } from "react-router-dom";
import { useMutation, useQueryClient } from "@tanstack/react-query";
import {
  ArrowLeft,
  Check,
  ChevronDown,
  MonitorSmartphone,
  ScanLine,
  ShieldCheck,
} from "lucide-react";
import { api } from "@/services/api";
import type { CaptureResult, WindowInfo } from "@/types/contract";
import { Badge, Button, Card, EmptyState, ErrorNote, Spinner } from "@/components/ui";
import { MonitorCanvas } from "@/components/MonitorCanvas";

type Step = 1 | 2 | 3;

const STEPS: { id: Step; label: string }[] = [
  { id: 1, label: "扫描桌面" },
  { id: 2, label: "选择对象" },
  { id: 3, label: "保存现场" },
];

export function CapturePage() {
  const navigate = useNavigate();
  const qc = useQueryClient();

  const [step, setStep] = useState<Step>(1);
  const [result, setResult] = useState<CaptureResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState<Set<number>>(new Set());
  const [name, setName] = useState("");
  const [showExcluded, setShowExcluded] = useState(false);

  const scan = useMutation({
    mutationFn: api.captureStart,
    onMutate: () => {
      setError(null);
      setResult(null);
    },
    onSuccess: (data) => {
      setResult(data);
      // 默认全选：符合「捕获当前现场」的直觉，用户再取消不要的窗口
      setSelected(new Set(data.windows.map((w) => w.hwnd)));
      const d = new Date();
      setName(
        `工作现场 ${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`,
      );
      setStep(2);
    },
    onError: (e) => setError(String((e as Error).message)),
  });

  useEffect(() => {
    scan.mutate();
    // 只扫描一次，重扫由用户点击触发
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const save = useMutation({
    mutationFn: async () => {
      if (!result) throw new Error("没有可保存的捕获结果");
      const subset: CaptureResult = {
        ...result,
        windows: result.windows.filter((w) => selected.has(w.hwnd)),
      };
      if (subset.windows.length === 0) throw new Error("请至少选择一个窗口");
      return api.workspaceCreate(name, subset);
    },
    onSuccess: (scene) => {
      qc.invalidateQueries({ queryKey: ["workspaces"] });
      navigate(`/workspaces?id=${scene.id}`);
    },
    onError: (e) => setError(String((e as Error).message)),
  });

  const groups = useMemo(() => {
    const map = new Map<string, WindowInfo[]>();
    for (const w of result?.windows ?? []) {
      const key = w.processName || "未知进程";
      const list = map.get(key) ?? [];
      list.push(w);
      map.set(key, list);
    }
    return [...map.entries()].sort((a, b) => b[1].length - a[1].length);
  }, [result]);

  const selectedWindows = (result?.windows ?? []).filter((w) => selected.has(w.hwnd));

  const toggle = (hwnd: number) => {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(hwnd)) next.delete(hwnd);
      else next.add(hwnd);
      return next;
    });
  };

  return (
    <div className="mx-auto flex max-w-[1100px] flex-col gap-5">
      <StepBar current={step} />

      {error ? <ErrorNote message={error} /> : null}

      {step === 1 ? (
        <Card className="flex flex-col items-center gap-4 py-14 text-center">
          {scan.isPending ? (
            <>
              <ScanLine size={26} className="animate-pulse text-accent" />
              <p className="text-card font-medium">正在枚举窗口、进程与显示器…</p>
              <p className="text-aux text-ink-3">只读取窗口元数据，不读取文件内容、不截屏</p>
              <Button
                variant="ghost"
                onClick={() => {
                  scan.reset();
                  navigate("/");
                }}
              >
                取消
              </Button>
            </>
          ) : (
            <>
              <ScanLine size={26} className="text-ink-3" />
              <p className="text-card font-medium">扫描未完成</p>
              <Button variant="primary" onClick={() => scan.mutate()}>
                重新扫描
              </Button>
            </>
          )}
        </Card>
      ) : null}

      {step === 2 && result ? (
        <>
          <Card padded={false} className="overflow-hidden">
            <div className="flex items-center justify-between border-b border-line px-5 py-3">
              <div className="flex items-center gap-3">
                <p className="text-body font-medium">选择要保存的窗口</p>
                <Badge tone="accent">{selected.size} / {result.windows.length}</Badge>
              </div>
              <div className="flex items-center gap-2">
                <Button
                  size="sm"
                  variant="ghost"
                  onClick={() => setSelected(new Set(result.windows.map((w) => w.hwnd)))}
                >
                  全选
                </Button>
                <Button size="sm" variant="ghost" onClick={() => setSelected(new Set())}>
                  清空
                </Button>
                <Button size="sm" variant="ghost" onClick={() => scan.mutate()}>
                  重新扫描
                </Button>
              </div>
            </div>

            <div className="max-h-[380px] overflow-y-auto">
              {groups.map(([app, wins]) => (
                <div key={app} className="border-b border-line/60 last:border-0">
                  <div className="flex items-center justify-between bg-surface/60 px-5 py-2">
                    <span className="text-aux font-medium text-ink-2">{app}</span>
                    <span className="text-[11px] text-ink-3">{wins.length} 个窗口</span>
                  </div>
                  <ul>
                    {wins.map((w) => {
                      const on = selected.has(w.hwnd);
                      return (
                        <li key={w.hwnd}>
                          <button
                            onClick={() => toggle(w.hwnd)}
                            className="flex w-full items-center gap-3 px-5 py-2 text-left hover:bg-raised/50"
                          >
                            <span
                              className={`grid size-4 shrink-0 place-items-center rounded-sm border ${
                                on
                                  ? "border-accent bg-accent text-accent-ink"
                                  : "border-line-strong"
                              }`}
                              aria-hidden
                            >
                              {on ? <Check size={11} strokeWidth={3} /> : null}
                            </span>
                            <span className="min-w-0 flex-1">
                              <span className="block truncate text-body">
                                {w.title || "（无标题窗口）"}
                              </span>
                              <span className="mt-0.5 block font-mono text-[11px] text-ink-3">
                                {w.width}×{w.height} · {w.className} · PID {w.processId}
                                {w.dpi !== 96 ? ` · ${Math.round((w.dpi / 96) * 100)}%` : ""}
                              </span>
                            </span>
                            {w.isMaximized ? <Badge>最大化</Badge> : null}
                            {w.isMinimized ? <Badge>最小化</Badge> : null}
                          </button>
                        </li>
                      );
                    })}
                  </ul>
                </div>
              ))}
            </div>
          </Card>

          {result.excluded.length > 0 ? (
            <Card padded={false}>
              <button
                onClick={() => setShowExcluded((v) => !v)}
                className="flex w-full items-center justify-between px-5 py-3 text-left"
              >
                <span className="flex items-center gap-2 text-aux text-ink-2">
                  <ShieldCheck size={14} className="text-ink-3" />
                  已自动排除 {result.excluded.length} 个系统/工具窗口
                </span>
                <ChevronDown
                  size={14}
                  className={`text-ink-3 transition-transform ${showExcluded ? "rotate-180" : ""}`}
                />
              </button>
              {showExcluded ? (
                <ul className="max-h-44 overflow-y-auto border-t border-line px-5 py-2">
                  {result.excluded.map((e, i) => (
                    <li key={`${e.className}-${i}`} className="flex gap-3 py-1 text-[11px]">
                      <span className="w-40 shrink-0 truncate font-mono text-ink-3">
                        {e.className}
                      </span>
                      <span className="w-32 shrink-0 truncate text-ink-3">{e.processName}</span>
                      <span className="w-48 shrink-0 truncate text-ink-2">{e.title}</span>
                      <span className="text-warning">{e.reason}</span>
                    </li>
                  ))}
                </ul>
              ) : null}
            </Card>
          ) : null}

          <Card>
            <div className="mb-2 flex items-center gap-2 text-aux text-ink-3">
              <MonitorSmartphone size={14} /> 布局预览（{selectedWindows.length} 个窗口）
            </div>
            <MonitorCanvas displays={result.displays} windows={selectedWindows} height={200} />
          </Card>

          <div className="flex items-center justify-between">
            <Button variant="ghost" onClick={() => navigate("/")}>
              <ArrowLeft size={15} /> 取消
            </Button>
            <Button
              variant="primary"
              disabled={selected.size === 0}
              onClick={() => setStep(3)}
            >
              下一步：保存
            </Button>
          </div>
        </>
      ) : null}

      {step === 3 && result ? (
        <>
          <Card className="flex flex-col gap-4">
            <label className="flex flex-col gap-1.5">
              <span className="text-aux text-ink-2">工作空间名称</span>
              <input
                value={name}
                onChange={(e) => setName(e.target.value)}
                maxLength={80}
                className="h-9 rounded-md border border-line-strong bg-canvas px-3 text-body outline-none focus:border-accent"
                placeholder="例如：前端开发"
              />
            </label>

            <dl className="grid grid-cols-3 gap-3 border-t border-line pt-4 text-aux">
              <div>
                <dt className="text-ink-3">窗口</dt>
                <dd className="tabnum mt-0.5 text-card text-ink">{selected.size}</dd>
              </div>
              <div>
                <dt className="text-ink-3">显示器</dt>
                <dd className="tabnum mt-0.5 text-card text-ink">{result.displays.length}</dd>
              </div>
              <div>
                <dt className="text-ink-3">识别不确定项</dt>
                <dd className="tabnum mt-0.5 text-card text-ink">
                  {result.windows.filter((w) => !w.processPath).length}
                </dd>
              </div>
            </dl>

            <div className="rounded-md border border-line bg-canvas/60 px-3 py-2 text-[11px] leading-relaxed text-ink-3">
              保存的内容可能包含应用名称、窗口标题、项目路径与显示器配置。
              本产品不读取密码、Cookie、键盘输入，也不会截图或上传任何数据；
              快照默认保存在本机的 {`%LOCALAPPDATA%\\WorkScene\\snapshots`}。
            </div>
          </Card>

          {save.isError ? <ErrorNote message={String(save.error.message)} /> : null}

          <div className="flex items-center justify-between">
            <Button variant="ghost" onClick={() => setStep(2)}>
              <ArrowLeft size={15} /> 返回选择
            </Button>
            <Button variant="primary" disabled={save.isPending} onClick={() => save.mutate()}>
              {save.isPending ? <Spinner /> : <Check size={15} />}
              保存为工作空间
            </Button>
          </div>
        </>
      ) : null}

      {step === 2 && result && result.windows.length === 0 ? (
        <EmptyState
          title="当前没有可保存的窗口"
          description="请先打开需要记录的应用，再点击「重新扫描」。"
          action={<Button onClick={() => scan.mutate()}>重新扫描</Button>}
        />
      ) : null}
    </div>
  );
}

function StepBar({ current }: { current: Step }) {
  return (
    <ol className="flex items-center gap-3">
      {STEPS.map((s, i) => {
        const done = s.id < current;
        const active = s.id === current;
        return (
          <li key={s.id} className="flex items-center gap-3">
            <span
              className={`grid size-5 place-items-center rounded-full text-[11px] font-semibold ${
                active
                  ? "bg-accent text-accent-ink"
                  : done
                    ? "bg-success/20 text-success"
                    : "bg-raised text-ink-3"
              }`}
            >
              {done ? <Check size={11} strokeWidth={3} /> : s.id}
            </span>
            <span className={`text-aux ${active ? "text-ink" : "text-ink-3"}`}>{s.label}</span>
            {i < STEPS.length - 1 ? <span className="h-px w-8 bg-line" /> : null}
          </li>
        );
      })}
    </ol>
  );
}
