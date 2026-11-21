import { useMemo, useRef, useState } from "react";
import { Maximize2, RotateCcw, Save, Trash2 } from "lucide-react";
import type { DisplayInfo, LayoutUpdate, WorkspaceWindow } from "@/types/contract";
import { Button, Card } from "@/components/ui";

/**
 * 可编辑的窗口布局画布（UI 规范 §7.4 / UI-06b）。
 *
 * 坐标模型：
 * - 每个窗口的比例坐标相对它所属显示器的**工作区**（与 Rust 侧一致）；
 * - 画布用显示屏的物理矩形拼出世界坐标，再整体缩放到容器里；
 * - 拖动时把像素位移换算回「该显示器工作区」的比例增量，因此跨屏拖动仍然正确。
 */
export function LayoutEditor({
  displays,
  windows,
  onChange,
  onSave,
  onReset,
  saving,
}: {
  displays: DisplayInfo[];
  windows: WorkspaceWindow[];
  onChange: (next: WorkspaceWindow[]) => void;
  onSave: () => void;
  onReset: () => void;
  saving?: boolean;
}) {
  const boxRef = useRef<HTMLDivElement>(null);
  const [selected, setSelected] = useState<string | null>(null);
  const [drag, setDrag] = useState<{
    id: string;
    mode: "move" | "resize";
    startX: number;
    startY: number;
    origin: WorkspaceWindow;
    scale: number;
    workW: number;
    workH: number;
  } | null>(null);

  const bounds = useMemo(() => {
    if (displays.length === 0) return { x: 0, y: 0, w: 1, h: 1 };
    const minX = Math.min(...displays.map((d) => d.x));
    const minY = Math.min(...displays.map((d) => d.y));
    const maxX = Math.max(...displays.map((d) => d.x + d.width));
    const maxY = Math.max(...displays.map((d) => d.y + d.height));
    return { x: minX, y: minY, w: Math.max(1, maxX - minX), h: Math.max(1, maxY - minY) };
  }, [displays]);

  const monitorOf = (key: string): DisplayInfo | undefined =>
    displays.find((d) => d.monitorKey === key) ??
    displays.find((d) => d.isPrimary) ??
    displays[0];

  const toPct = (w: WorkspaceWindow): { left: string; top: string; width: string; height: string } => {
    const m = monitorOf(w.monitorKey);
    if (!m) return { left: "0%", top: "0%", width: "0%", height: "0%" };
    const absX = m.workX + w.xRatio * m.workWidth;
    const absY = m.workY + w.yRatio * m.workHeight;
    const absW = w.widthRatio * m.workWidth;
    const absH = w.heightRatio * m.workHeight;
    return {
      left: `${((absX - bounds.x) / bounds.w) * 100}%`,
      top: `${((absY - bounds.y) / bounds.h) * 100}%`,
      width: `${(absW / bounds.w) * 100}%`,
      height: `${(absH / bounds.h) * 100}%`,
    };
  };

  const beginDrag = (
    e: React.PointerEvent,
    w: WorkspaceWindow,
    mode: "move" | "resize",
  ) => {
    const rect = boxRef.current?.getBoundingClientRect();
    if (!rect) return;
    const m = monitorOf(w.monitorKey);
    if (!m) return;
    (e.target as HTMLElement).setPointerCapture(e.pointerId);
    setSelected(w.id);
    setDrag({
      id: w.id,
      mode,
      startX: e.clientX,
      startY: e.clientY,
      origin: w,
      // 世界像素 / 容器像素
      scale: bounds.w / rect.width,
      workW: m.workWidth,
      workH: m.workHeight,
    });
  };

  const onMove = (e: React.PointerEvent) => {
    if (!drag) return;
    const minRatio = 0.08;
    const dxWorld = (e.clientX - drag.startX) * drag.scale;
    const dyWorld = (e.clientY - drag.startY) * drag.scale;
    const dxRatio = dxWorld / drag.workW;
    const dyRatio = dyWorld / drag.workH;

    const next = windows.map((w) => {
      if (w.id !== drag.id) return w;
      if (drag.mode === "move") {
        const x = clamp(drag.origin.xRatio + dxRatio, 0, 1 - drag.origin.widthRatio);
        const y = clamp(drag.origin.yRatio + dyRatio, 0, 1 - drag.origin.heightRatio);
        return { ...w, xRatio: x, yRatio: y };
      }
      const width = clamp(
        drag.origin.widthRatio + dxRatio,
        minRatio,
        1 - drag.origin.xRatio,
      );
      const height = clamp(
        drag.origin.heightRatio + dyRatio,
        minRatio,
        1 - drag.origin.yRatio,
      );
      return { ...w, widthRatio: width, heightRatio: height };
    });
    onChange(next);
  };

  const endDrag = () => setDrag(null);

  return (
    <Card className="flex flex-col gap-3">
      <div className="flex items-center justify-between">
        <div>
          <p className="text-body font-medium">窗口布局</p>
          <p className="mt-0.5 text-[11px] text-ink-3">
            拖动窗口调整位置，拖右下角改尺寸；坐标按显示器工作区比例保存
          </p>
        </div>
        <div className="flex items-center gap-2">
          {selected ? (
            <Button
              size="sm"
              variant="ghost"
              title="从工作空间中移除该窗口"
              onClick={() => {
                onChange(windows.filter((w) => w.id !== selected));
                setSelected(null);
              }}
            >
              <Trash2 size={13} /> 移除窗口
            </Button>
          ) : null}
          <Button size="sm" variant="primary" disabled={saving} onClick={onSave}>
            <Save size={13} /> 保存布局
          </Button>
        </div>
      </div>

      <div
        ref={boxRef}
        className="relative w-full select-none overflow-hidden rounded-md border border-line bg-canvas"
        style={{ aspectRatio: `${bounds.w} / ${bounds.h}`, maxHeight: 420 }}
        onPointerMove={onMove}
        onPointerUp={endDrag}
        onPointerCancel={endDrag}
      >
        {displays.map((d) => (
          <div
            key={d.monitorKey}
            className="absolute rounded-sm border border-line-strong/70 bg-surface/40"
            style={{
              left: `${((d.x - bounds.x) / bounds.w) * 100}%`,
              top: `${((d.y - bounds.y) / bounds.h) * 100}%`,
              width: `${(d.width / bounds.w) * 100}%`,
              height: `${(d.height / bounds.h) * 100}%`,
            }}
          >
            <div
              className="absolute border border-dashed border-line"
              style={{
                left: `${((d.workX - d.x) / d.width) * 100}%`,
                top: `${((d.workY - d.y) / d.height) * 100}%`,
                width: `${(d.workWidth / d.width) * 100}%`,
                height: `${(d.workHeight / d.height) * 100}%`,
              }}
            />
            <span className="absolute left-1 top-1 font-mono text-[10px] text-ink-3">
              {d.isPrimary ? "主屏" : "副屏"} {d.width}×{d.height}
            </span>
          </div>
        ))}

        {windows.map((w) => {
          const isSel = w.id === selected;
          return (
            <div
              key={w.id}
              onPointerDown={(e) => beginDrag(e, w, "move")}
              className={`absolute cursor-move rounded-sm border text-[10px] ${
                isSel
                  ? "border-accent bg-accent/25"
                  : "border-accent/60 bg-accent/12 hover:bg-accent/20"
              }`}
              style={toPct(w)}
              title={`${w.title || w.processName}\n${(w.xRatio * 100).toFixed(1)}% / ${(
                w.yRatio * 100
              ).toFixed(1)}% · ${(w.widthRatio * 100).toFixed(1)}% × ${(
                w.heightRatio * 100
              ).toFixed(1)}%`}
            >
              <span className="pointer-events-none block truncate px-1 pt-0.5 text-ink-2">
                {w.title || w.processName}
              </span>
              {w.showState === "maximized" ? (
                <Maximize2
                  size={10}
                  className="pointer-events-none absolute right-1 top-1 text-ink-3"
                />
              ) : null}
              <span
                onPointerDown={(e) => {
                  e.stopPropagation();
                  beginDrag(e, w, "resize");
                }}
                className="absolute -bottom-1 -right-1 size-2.5 cursor-nwse-resize rounded-sm border border-accent bg-accent/70"
                aria-label="调整大小"
              />
            </div>
          );
        })}
      </div>

      <div className="flex items-center justify-between text-[11px] text-ink-3">
        <span>{windows.length} 个窗口 · 拖动后记得保存</span>
        <Button
          size="sm"
          variant="ghost"
          onClick={onReset}
          title="放弃未保存的改动"
        >
          <RotateCcw size={12} /> 放弃改动
        </Button>
      </div>
    </Card>
  );
}

function clamp(v: number, min: number, max: number): number {
  if (!Number.isFinite(v)) return min;
  return Math.min(Math.max(v, min), Math.max(min, max));
}

export function toLayoutUpdates(windows: WorkspaceWindow[]): LayoutUpdate[] {
  return windows.map((w) => ({
    windowId: w.id,
    xRatio: w.xRatio,
    yRatio: w.yRatio,
    widthRatio: w.widthRatio,
    heightRatio: w.heightRatio,
  }));
}
