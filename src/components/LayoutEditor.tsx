import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  Grid3X3,
  Layers,
  Magnet,
  Maximize2,
  Minimize2,
  Redo2,
  RotateCcw,
  Save,
  ShieldCheck,
  Trash2,
  Undo2,
  ZoomIn,
  ZoomOut,
} from "lucide-react";
import type { DisplayInfo, LayoutUpdate, ShowState, WorkspaceWindow } from "@/types/contract";
import { Badge, Button, Card } from "@/components/ui";

interface DragState {
  id: string;
  mode: "move" | "resize";
  startX: number;
  startY: number;
  origin: WorkspaceWindow;
  scaleX: number;
  scaleY: number;
  workW: number;
  workH: number;
  snapshotBefore: WorkspaceWindow[];
}

/** 最小可编辑尺寸（与 Rust 侧 restore::layout::MIN_RATIO 保持一致） */
const MIN_RATIO = 0.08;

/**
 * Visual Workspace Map 可编辑画布 + Inspector 工作台（UI 规范 §7.3–7.6 & 评审 U5/U6）。
 *
 * 坐标模型：
 * - 每个窗口的比例坐标相对它所属显示器的工作区（与 Rust 侧一致）；
 * - 支持拖拽移动/缩放、边缘吸附、网格辅助、缩放视图、撤销/重做、方向键微调，
 *   以及右侧 Inspector 精确数值双向编辑（无障碍键盘替代）。
 */
export function LayoutEditor({
  displays,
  windows,
  dirty,
  onChange,
  onSave,
  onReset,
  saving,
}: {
  displays: DisplayInfo[];
  windows: WorkspaceWindow[];
  dirty: boolean;
  onChange: (next: WorkspaceWindow[]) => void;
  onSave: () => void;
  onReset: () => void;
  saving?: boolean;
}) {
  const boxRef = useRef<HTMLDivElement>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [hoveredId, setHoveredId] = useState<string | null>(null);
  const [drag, setDrag] = useState<DragState | null>(null);
  const [snapEnabled, setSnapEnabled] = useState(true);
  const [showGrid, setShowGrid] = useState(false);
  const [zoom, setZoom] = useState(1);
  const [undoStack, setUndoStack] = useState<WorkspaceWindow[][]>([]);
  const [redoStack, setRedoStack] = useState<WorkspaceWindow[][]>([]);

  // 如果当前选中的窗口被外部删除，清空选中
  useEffect(() => {
    if (selectedId && !windows.some((w) => w.id === selectedId)) {
      setSelectedId(null);
    }
  }, [selectedId, windows]);

  // 撤销/重做历史只跟随组件生命周期：切换工作空间由父级用 key 重新挂载来清空，
  // 保存后（dirty 变回 false）不能清空，否则「撤销回初始状态」会同时把重做也清掉。

  const commitChange = useCallback(
    (next: WorkspaceWindow[], prevSnapshot?: WorkspaceWindow[]) => {
      setUndoStack((s) => [...s.slice(-29), prevSnapshot ?? windows]);
      setRedoStack([]);
      onChange(next);
    },
    [onChange, windows],
  );

  const handleUndo = useCallback(() => {
    if (undoStack.length === 0) return;
    const last = undoStack[undoStack.length - 1]!;
    setUndoStack((prev) => prev.slice(0, -1));
    setRedoStack((r) => [...r.slice(-29), windows]);
    onChange(last);
  }, [onChange, undoStack, windows]);

  const handleRedo = useCallback(() => {
    if (redoStack.length === 0) return;
    const next = redoStack[redoStack.length - 1]!;
    setRedoStack((prev) => prev.slice(0, -1));
    setUndoStack((u) => [...u.slice(-29), windows]);
    onChange(next);
  }, [onChange, redoStack, windows]);

  const bounds = useMemo(() => {
    if (displays.length === 0) return { x: 0, y: 0, w: 1, h: 1 };
    const minX = Math.min(...displays.map((d) => d.x));
    const minY = Math.min(...displays.map((d) => d.y));
    const maxX = Math.max(...displays.map((d) => d.x + d.width));
    const maxY = Math.max(...displays.map((d) => d.y + d.height));
    return {
      x: minX,
      y: minY,
      w: Math.max(1, maxX - minX),
      h: Math.max(1, maxY - minY),
    };
  }, [displays]);

  const monitorOf = useCallback(
    (key: string): DisplayInfo | undefined =>
      displays.find((d) => d.monitorKey === key) ??
      displays.find((d) => d.isPrimary) ??
      displays[0],
    [displays],
  );

  const selectedWindow = useMemo(
    () => windows.find((w) => w.id === selectedId) ?? null,
    [windows, selectedId],
  );

  const selectedMonitorMissing = Boolean(
    selectedWindow &&
      selectedWindow.monitorKey &&
      !displays.some((d) => d.monitorKey === selectedWindow.monitorKey),
  );

  const updateWindow = useCallback(
    (id: string, patch: Partial<WorkspaceWindow>, recordUndo = true) => {
      const next = windows.map((w) => (w.id === id ? { ...w, ...patch } : w));
      if (recordUndo) commitChange(next);
      else onChange(next);
    },
    [commitChange, onChange, windows],
  );

  const removeWindow = useCallback(
    (id: string) => {
      commitChange(windows.filter((w) => w.id !== id));
      if (selectedId === id) setSelectedId(null);
    },
    [commitChange, selectedId, windows],
  );

  /** 一键分屏排布模板：按显示器分组自动排齐，支持 Ctrl+Z 撤销 */
  const applyLayoutTemplate = useCallback(
    (preset: "split2" | "focus65" | "cols3" | "grid4" | "center") => {
      if (windows.length === 0) return;
      const gap = 0.008;
      const primaryKey =
        displays.find((d) => d.isPrimary)?.monitorKey ?? displays[0]?.monitorKey ?? "";

      const groups = new Map<string, WorkspaceWindow[]>();
      for (const w of windows) {
        const k = w.monitorKey || primaryKey;
        const list = groups.get(k) ?? [];
        list.push(w);
        groups.set(k, list);
      }

      const patchMap = new Map<string, Partial<WorkspaceWindow>>();
      for (const list of groups.values()) {
        const total = list.length;
        list.forEach((w, idx) => {
          if (preset === "split2") {
            const cols = Math.min(2, total);
            const rows = Math.max(1, Math.ceil(total / cols));
            const col = idx % cols;
            const row = Math.floor(idx / cols);
            const cellW = (1 - gap * (cols + 1)) / cols;
            const cellH = (1 - gap * (rows + 1)) / rows;
            patchMap.set(w.id, {
              xRatio: round4(gap + col * (cellW + gap)),
              yRatio: round4(gap + row * (cellH + gap)),
              widthRatio: round4(cellW),
              heightRatio: round4(Math.max(0.15, cellH)),
              showState: "normal",
            });
          } else if (preset === "focus65") {
            const leftW = 0.64;
            const rightW = 1 - leftW - gap * 3;
            if (idx === 0) {
              patchMap.set(w.id, {
                xRatio: gap,
                yRatio: gap,
                widthRatio: round4(leftW),
                heightRatio: round4(1 - gap * 2),
                showState: "normal",
              });
            } else {
              const stackCount = Math.max(1, total - 1);
              const row = idx - 1;
              const cellH = (1 - gap * (stackCount + 1)) / stackCount;
              patchMap.set(w.id, {
                xRatio: round4(gap * 2 + leftW),
                yRatio: round4(gap + row * (cellH + gap)),
                widthRatio: round4(rightW),
                heightRatio: round4(Math.max(0.15, cellH)),
                showState: "normal",
              });
            }
          } else if (preset === "cols3") {
            const cols = Math.min(3, total);
            const rows = Math.max(1, Math.ceil(total / cols));
            const col = idx % cols;
            const row = Math.floor(idx / cols);
            const colW = (1 - gap * (cols + 1)) / cols;
            const cellH = (1 - gap * (rows + 1)) / rows;
            patchMap.set(w.id, {
              xRatio: round4(gap + col * (colW + gap)),
              yRatio: round4(gap + row * (cellH + gap)),
              widthRatio: round4(colW),
              heightRatio: round4(Math.max(0.15, cellH)),
              showState: "normal",
            });
          } else if (preset === "grid4") {
            const cols = total <= 1 ? 1 : total <= 4 ? 2 : Math.ceil(Math.sqrt(total));
            const rows = Math.max(1, Math.ceil(total / cols));
            const col = idx % cols;
            const row = Math.floor(idx / cols);
            const cellW = (1 - gap * (cols + 1)) / cols;
            const cellH = (1 - gap * (rows + 1)) / rows;
            patchMap.set(w.id, {
              xRatio: round4(gap + col * (cellW + gap)),
              yRatio: round4(gap + row * (cellH + gap)),
              widthRatio: round4(cellW),
              heightRatio: round4(Math.max(0.15, cellH)),
              showState: "normal",
            });
          } else {
            const step = total > 6 ? 0.012 : 0.018;
            const offset = Math.min(idx, 10) * step;
            patchMap.set(w.id, {
              xRatio: round4(Math.min(0.24, 0.06 + offset)),
              yRatio: round4(Math.min(0.2, 0.05 + offset)),
              widthRatio: 0.74,
              heightRatio: 0.78,
              showState: "normal",
            });
          }
        });
      }

      // 窗口很多时，模板单元格被最小高度/宽度撑大，row/col 累加后会越出工作区：
      // 统一夹一次，保证每个窗口都满足 x+width <= 1、y+height <= 1。
      for (const [id, p] of patchMap) {
        const x = clamp(p.xRatio ?? 0, 0, 1 - MIN_RATIO);
        const y = clamp(p.yRatio ?? 0, 0, 1 - MIN_RATIO);
        const w = clamp(p.widthRatio ?? 0, MIN_RATIO, 1 - x);
        const h = clamp(p.heightRatio ?? 0, MIN_RATIO, 1 - y);
        patchMap.set(id, {
          ...p,
          xRatio: round4(x),
          yRatio: round4(y),
          widthRatio: round4(w),
          heightRatio: round4(h),
        });
      }

      const next = windows.map((w) => {
        const p = patchMap.get(w.id);
        return p ? { ...w, ...p } : w;
      });
      commitChange(next);
    },
    [commitChange, displays, windows],
  );

  // 键盘编辑支持：方向键微调、Shift+方向键 5% 步进、Ctrl+Z/Y 撤销重做、Esc 取消选中
  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (document.querySelector('[data-ws-modal="true"]')) return;
      const target = e.target as HTMLElement | null;
      if (
        target &&
        (target.tagName === "INPUT" ||
          target.tagName === "TEXTAREA" ||
          target.tagName === "SELECT")
      ) {
        return;
      }

      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "z") {
        e.preventDefault();
        if (e.shiftKey) handleRedo();
        else handleUndo();
        return;
      }
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "y") {
        e.preventDefault();
        handleRedo();
        return;
      }
      if (e.key === "Escape") {
        if (drag) {
          onChange(drag.snapshotBefore);
          setDrag(null);
        } else {
          setSelectedId(null);
        }
        return;
      }
      if (!selectedWindow) return;

      const step = e.shiftKey ? 0.05 : 0.01;
      if (e.key === "ArrowLeft") {
        e.preventDefault();
        updateWindow(selectedWindow.id, {
          xRatio: clamp(selectedWindow.xRatio - step, 0, 1 - selectedWindow.widthRatio),
        });
      } else if (e.key === "ArrowRight") {
        e.preventDefault();
        updateWindow(selectedWindow.id, {
          xRatio: clamp(selectedWindow.xRatio + step, 0, 1 - selectedWindow.widthRatio),
        });
      } else if (e.key === "ArrowUp") {
        e.preventDefault();
        updateWindow(selectedWindow.id, {
          yRatio: clamp(selectedWindow.yRatio - step, 0, 1 - selectedWindow.heightRatio),
        });
      } else if (e.key === "ArrowDown") {
        e.preventDefault();
        updateWindow(selectedWindow.id, {
          yRatio: clamp(selectedWindow.yRatio + step, 0, 1 - selectedWindow.heightRatio),
        });
      }
    };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [drag, handleRedo, handleUndo, onChange, selectedWindow, updateWindow]);

  const toPct = (w: WorkspaceWindow) => {
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

  const snapRatio = (v: number, snapPoints: number[]) => {
    if (!snapEnabled) return v;
    const threshold = 0.015;
    for (const p of snapPoints) {
      if (Math.abs(v - p) <= threshold) return p;
    }
    return v;
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
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
    setSelectedId(w.id);
    setDrag({
      id: w.id,
      mode,
      startX: e.clientX,
      startY: e.clientY,
      origin: w,
      scaleX: bounds.w / rect.width,
      scaleY: bounds.h / rect.height,
      workW: m.workWidth,
      workH: m.workHeight,
      snapshotBefore: windows,
    });
  };

  const onMove = (e: React.PointerEvent) => {
    if (!drag) return;
    const minRatio = MIN_RATIO;
    const dxWorld = (e.clientX - drag.startX) * drag.scaleX;
    const dyWorld = (e.clientY - drag.startY) * drag.scaleY;
    const dxRatio = dxWorld / drag.workW;
    const dyRatio = dyWorld / drag.workH;

    const next = windows.map((w) => {
      if (w.id !== drag.id) return w;
      if (drag.mode === "move") {
        let x = clamp(drag.origin.xRatio + dxRatio, 0, 1 - drag.origin.widthRatio);
        let y = clamp(drag.origin.yRatio + dyRatio, 0, 1 - drag.origin.heightRatio);
        x = snapRatio(x, [0, 0.25, 0.5, 1 - drag.origin.widthRatio]);
        y = snapRatio(y, [0, 0.25, 0.5, 1 - drag.origin.heightRatio]);
        // 手动拖放后自动切为 normal，避免恢复时被原 maximized 状态覆盖回全屏
        return { ...w, xRatio: round4(x), yRatio: round4(y), showState: "normal" as ShowState };
      }
      let width = clamp(
        drag.origin.widthRatio + dxRatio,
        minRatio,
        1 - drag.origin.xRatio,
      );
      let height = clamp(
        drag.origin.heightRatio + dyRatio,
        minRatio,
        1 - drag.origin.yRatio,
      );
      width = snapRatio(width, [0.3333, 0.5, 0.6667, 1 - drag.origin.xRatio]);
      height = snapRatio(height, [0.5, 1 - drag.origin.yRatio]);
      return {
        ...w,
        widthRatio: round4(width),
        heightRatio: round4(height),
        showState: "normal" as ShowState,
      };
    });
    onChange(next);
  };

  const endDrag = () => {
    if (!drag) return;
    const cur = windows.find((w) => w.id === drag.id);
    const changed =
      cur &&
      (cur.xRatio !== drag.origin.xRatio ||
        cur.yRatio !== drag.origin.yRatio ||
        cur.widthRatio !== drag.origin.widthRatio ||
        cur.heightRatio !== drag.origin.heightRatio ||
        cur.showState !== drag.origin.showState);
    if (changed) {
      setUndoStack((s) => [...s.slice(-29), drag.snapshotBefore]);
      setRedoStack([]);
    }
    setDrag(null);
  };

  // 将选中/悬停窗口放在最上层渲染，解决重叠窗口遮挡和文字糊成一团的问题
  const renderOrder = useMemo(() => {
    return [...windows].sort((a, b) => {
      const score = (w: WorkspaceWindow) =>
        w.id === selectedId ? 3 : w.id === hoveredId ? 2 : 1;
      return score(a) - score(b);
    });
  }, [windows, selectedId, hoveredId]);

  const applyPreset = (xRatio: number, yRatio: number, widthRatio: number, heightRatio: number) => {
    if (!selectedWindow) return;
    updateWindow(selectedWindow.id, {
      xRatio,
      yRatio,
      widthRatio,
      heightRatio,
      showState: "normal",
    });
  };

  return (
    <Card padded={false} className="overflow-hidden">
      {/* 工具栏 */}
      <div className="flex flex-wrap items-center justify-between gap-3 border-b border-line bg-sidebar/60 px-4 py-2.5">
        <div className="flex items-center gap-2">
          <span className="text-body font-medium text-ink">Visual Workspace Map</span>
          <Badge tone={dirty ? "warning" : "neutral"}>
            {dirty ? "未保存修改" : `${windows.length} 个窗口`}
          </Badge>
        </div>

        <div className="flex flex-wrap items-center gap-1.5">
          <Button
            size="xs"
            variant="ghost"
            disabled={undoStack.length === 0}
            onClick={handleUndo}
            title="撤销 (Ctrl+Z)"
          >
            <Undo2 size={13} />
          </Button>
          <Button
            size="xs"
            variant="ghost"
            disabled={redoStack.length === 0}
            onClick={handleRedo}
            title="重做 (Ctrl+Y)"
          >
            <Redo2 size={13} />
          </Button>

          <span className="mx-1 h-4 w-px bg-line" />

          <Button
            size="xs"
            variant={snapEnabled ? "secondary" : "ghost"}
            onClick={() => setSnapEnabled((v) => !v)}
            title="吸附显示器边缘与半屏/四分屏参考线"
          >
            <Magnet size={13} />
            吸附
          </Button>
          <Button
            size="xs"
            variant={showGrid ? "secondary" : "ghost"}
            onClick={() => setShowGrid((v) => !v)}
            title="显示辅助网格"
          >
            <Grid3X3 size={13} />
          </Button>

          <span className="mx-1 h-4 w-px bg-line" />

          <Button
            size="xs"
            variant="ghost"
            disabled={zoom <= 0.75}
            onClick={() => setZoom((z) => Math.max(0.75, round2(z - 0.15)))}
            title="缩小画布"
          >
            <ZoomOut size={13} />
          </Button>
          <button
            type="button"
            onClick={() => setZoom(1)}
            title="重置缩放至 100%"
            className="tabnum cursor-pointer rounded-sm px-1.5 py-0.5 font-mono text-[11px] text-ink-2 hover:bg-raised"
          >
            {Math.round(zoom * 100)}%
          </button>
          <Button
            size="xs"
            variant="ghost"
            disabled={zoom >= 1.6}
            onClick={() => setZoom((z) => Math.min(1.6, round2(z + 0.15)))}
            title="放大画布"
          >
            <ZoomIn size={13} />
          </Button>

          <span className="mx-1 h-4 w-px bg-line" />

          <Button
            size="xs"
            variant="ghost"
            disabled={!dirty || saving}
            onClick={onReset}
            title="放弃未保存的布局改动"
          >
            <RotateCcw size={13} /> 放弃
          </Button>
          <Button
            size="xs"
            variant="primary"
            disabled={!dirty || saving}
            onClick={onSave}
            title="保存布局 (Ctrl+S)"
          >
            <Save size={13} /> {saving ? "保存中…" : "保存布局"}
            {dirty && !saving ? (
              <kbd className="ml-0.5 rounded-[4px] bg-white/18 px-1 font-mono text-[9px] text-white/90">
                Ctrl+S
              </kbd>
            ) : null}
          </Button>
        </div>
      </div>

      {/* 一键分屏排布快捷条 */}
      {windows.length > 0 ? (
        <div className="flex flex-wrap items-center gap-1.5 border-b border-line bg-canvas/55 px-4 py-2 text-[11px]">
          <span className="mr-1 font-medium text-ink-3">一键排布</span>
          {(
            [
              { id: "split2", label: "左右平分" },
              { id: "focus65", label: "左大右小" },
              { id: "cols3", label: "三列均分" },
              { id: "grid4", label: "四宫格" },
              { id: "center", label: "居中叠放" },
            ] as const
          ).map((p) => (
            <button
              key={p.id}
              type="button"
              onClick={() => applyLayoutTemplate(p.id)}
              className="cursor-pointer rounded-[6px] border border-line bg-surface px-2 py-0.5 text-ink-2 transition-colors hover:border-accent/50 hover:text-accent"
            >
              {p.label}
            </button>
          ))}
        </div>
      ) : null}

      {/* 主编辑区 + 右侧 Inspector */}
      <div className="grid grid-cols-1 lg:grid-cols-[minmax(0,1fr)_296px]">
        {/* 画布容器 */}
        <div className="flex flex-col gap-2.5 bg-canvas/70 p-4">
          <div className="overflow-auto rounded-md border border-line bg-canvas p-3">
            <div
              ref={boxRef}
              onClick={(e) => {
                if (e.target === e.currentTarget) setSelectedId(null);
              }}
              className="relative mx-auto select-none overflow-hidden rounded-sm"
              style={{
                width: `${zoom * 100}%`,
                aspectRatio: `${bounds.w} / ${bounds.h}`,
                maxHeight: zoom === 1 ? 430 : undefined,
              }}
              onPointerMove={onMove}
              onPointerUp={endDrag}
              onPointerCancel={endDrag}
            >
              {displays.map((d) => (
                <div
                  key={d.monitorKey}
                  className="pointer-events-none absolute rounded-sm border border-line-strong/80 bg-surface/55"
                  style={{
                    left: `${((d.x - bounds.x) / bounds.w) * 100}%`,
                    top: `${((d.y - bounds.y) / bounds.h) * 100}%`,
                    width: `${(d.width / bounds.w) * 100}%`,
                    height: `${(d.height / bounds.h) * 100}%`,
                  }}
                >
                  {/* 工作区虚线框 */}
                  <div
                    className="absolute border border-dashed border-line/90"
                    style={{
                      left: `${((d.workX - d.x) / d.width) * 100}%`,
                      top: `${((d.workY - d.y) / d.height) * 100}%`,
                      width: `${(d.workWidth / d.width) * 100}%`,
                      height: `${(d.workHeight / d.height) * 100}%`,
                      backgroundImage: showGrid
                        ? "linear-gradient(to right, rgba(130,143,163,0.12) 1px, transparent 1px), linear-gradient(to bottom, rgba(130,143,163,0.12) 1px, transparent 1px)"
                        : undefined,
                      backgroundSize: showGrid ? "10% 10%" : undefined,
                    }}
                  />
                  <span className="absolute bottom-1.5 right-2 rounded-sm bg-canvas/75 px-1.5 py-0.5 font-mono text-[10px] text-ink-3">
                    {d.isPrimary ? "主屏" : "副屏"} {d.width}×{d.height} ·{" "}
                    {Math.round(d.scaleFactor * 100)}%
                  </span>
                </div>
              ))}

              {renderOrder.map((w) => {
                const isSel = w.id === selectedId;
                const isHover = w.id === hoveredId;
                const active = isSel || isHover;
                const dimmed = Boolean(selectedId && !isSel && !isHover);
                return (
                  <div
                    key={w.id}
                    onPointerDown={(e) => beginDrag(e, w, "move")}
                    onMouseEnter={() => setHoveredId(w.id)}
                    onMouseLeave={() => setHoveredId((cur) => (cur === w.id ? null : cur))}
                    className={`absolute cursor-move overflow-hidden rounded-[6px] border transition-colors duration-100 ${
                      isSel
                        ? "border-accent bg-accent/22 shadow-[0_0_0_1px_var(--color-accent),0_6px_16px_rgba(0,0,0,0.22)]"
                        : isHover
                          ? "border-accent/80 bg-accent/14"
                          : dimmed
                            ? "border-line-strong/45 bg-surface/55 opacity-60"
                            : "border-accent/45 bg-surface/85"
                    }`}
                    style={toPct(w)}
                  >
                    {/* macOS 风格窗口模拟标题栏 */}
                    <div
                      className={`pointer-events-none flex h-5 items-center gap-1 border-b px-1.5 text-[10px] ${
                        isSel
                          ? "border-accent/35 bg-accent/20 text-ink"
                          : "border-line bg-raised/75 text-ink-2"
                      }`}
                    >
                      {isSel ? (
                        <span className="mr-0.5 flex items-center gap-0.5">
                          <span className="size-1.5 rounded-full bg-[#ff5f57]" />
                          <span className="size-1.5 rounded-full bg-[#febc2e]" />
                          <span className="size-1.5 rounded-full bg-[#28c840]" />
                        </span>
                      ) : null}
                      <span className="truncate font-medium">
                        {w.processName.replace(/\.exe$/i, "")}
                      </span>
                      {active && w.title ? (
                        <span className="truncate text-ink-3">· {w.title}</span>
                      ) : null}
                      {w.showState === "maximized" ? (
                        <Maximize2 size={9} className="ml-auto shrink-0 text-ink-3" />
                      ) : w.showState === "minimized" ? (
                        <Minimize2 size={9} className="ml-auto shrink-0 text-ink-3" />
                      ) : null}
                    </div>

                    {/* 拖动/选中时在矩形内部显示精确坐标 HUD */}
                    {isSel ? (
                      <div className="pointer-events-none px-1.5 pt-1 font-mono text-[10px] text-ink-2">
                        {Math.round(w.xRatio * 100)}%, {Math.round(w.yRatio * 100)}% ·{" "}
                        {Math.round(w.widthRatio * 100)}%×{Math.round(w.heightRatio * 100)}%
                      </div>
                    ) : null}

                    {/* 仅对选中或悬停窗口显示缩放手柄，避免满屏蓝点 */}
                    {active ? (
                      <span
                        onPointerDown={(e) => {
                          e.stopPropagation();
                          beginDrag(e, w, "resize");
                        }}
                        className="absolute bottom-0 right-0 size-3 cursor-nwse-resize rounded-tl-sm border-l border-t border-accent bg-accent/80"
                        aria-label="调整窗口大小"
                        title="拖动调整窗口大小"
                      />
                    ) : null}
                  </div>
                );
              })}
            </div>
          </div>

          <div className="flex flex-wrap items-center justify-between gap-2 text-[11px] text-ink-3">
            <span>
              点击窗口选中并在右侧微调；支持方向键微调（Shift 5%）、Esc 取消、Ctrl+Z 撤销
            </span>
            <span>画布仅编辑工作空间布局，不改变系统显示器物理排列</span>
          </div>
        </div>

        {/* 右侧 Inspector 面板 */}
        <aside className="flex flex-col border-t border-line bg-surface lg:border-l lg:border-t-0">
          <div className="flex items-center justify-between border-b border-line px-4 py-2.5">
            <span className="flex items-center gap-1.5 text-aux font-medium text-ink">
              <Layers size={14} className="text-ink-3" />
              {selectedWindow ? "窗口属性 Inspector" : "窗口图层列表"}
            </span>
            {selectedWindow ? (
              <button
                type="button"
                onClick={() => setSelectedId(null)}
                className="cursor-pointer text-[11px] text-accent hover:underline"
              >
                返回列表
              </button>
            ) : (
              <span className="tabnum text-[11px] text-ink-3">{windows.length} 项</span>
            )}
          </div>

          {selectedWindow ? (
            <div className="flex flex-1 flex-col gap-4 overflow-y-auto p-4">
              <div>
                <div className="flex items-start justify-between gap-2">
                  <p className="truncate text-body font-semibold text-ink">
                    {selectedWindow.title || selectedWindow.processName}
                  </p>
                  <Badge
                    tone={
                      selectedWindow.showState === "maximized"
                        ? "accent"
                        : selectedWindow.showState === "minimized"
                          ? "warning"
                          : "neutral"
                    }
                  >
                    {selectedWindow.showState === "maximized"
                      ? "最大化"
                      : selectedWindow.showState === "minimized"
                        ? "最小化"
                        : "普通"}
                  </Badge>
                </div>
                <p className="mt-1 font-mono text-[11px] text-ink-3">
                  显示器：{selectedWindow.monitorKey || "主显示器"}
                </p>
                {selectedMonitorMissing ? (
                  <p className="mt-1.5 rounded-sm border border-warning/40 bg-warning/10 px-2 py-1 text-[11px] text-warning">
                    原显示器当前未连接，恢复时将按工作区比例映射到主屏。
                  </p>
                ) : null}
              </div>

              {/* 目标显示器与显示状态（可持久化编辑） */}
              <div className="grid grid-cols-2 gap-2">
                <label className="flex flex-col gap-1">
                  <span className="text-[11px] text-ink-3">目标显示器</span>
                  <select
                    value={selectedWindow.monitorKey}
                    onChange={(e) =>
                      updateWindow(selectedWindow.id, { monitorKey: e.target.value })
                    }
                    className="h-8 rounded-md border border-line-strong/80 bg-canvas px-2 text-aux text-ink outline-none focus:border-accent"
                  >
                    {displays.map((d, i) => (
                      <option key={d.monitorKey} value={d.monitorKey}>
                        {d.isPrimary ? `主屏 (${d.width}×${d.height})` : `副屏 ${i + 1} (${d.width}×${d.height})`}
                      </option>
                    ))}
                    {selectedMonitorMissing ? (
                      <option value={selectedWindow.monitorKey}>
                        离线：{selectedWindow.monitorKey}
                      </option>
                    ) : null}
                  </select>
                </label>

                <label className="flex flex-col gap-1">
                  <span className="text-[11px] text-ink-3">恢复显示状态</span>
                  <select
                    value={selectedWindow.showState}
                    onChange={(e) =>
                      updateWindow(selectedWindow.id, {
                        showState: e.target.value as ShowState,
                      })
                    }
                    className="h-8 rounded-md border border-line-strong/80 bg-canvas px-2 text-aux text-ink outline-none focus:border-accent"
                  >
                    <option value="normal">普通窗口</option>
                    <option value="maximized">最大化</option>
                    <option value="minimized">最小化</option>
                  </select>
                </label>
              </div>

              {/* 位置与尺寸数值输入（与画布双向同步） */}
              <div>
                <p className="mb-2 text-[11px] font-medium uppercase tracking-wider text-ink-3">
                  工作区比例坐标 (%)
                </p>
                <div className="grid grid-cols-2 gap-2">
                  <PercentField
                    label="X 左边距"
                    value={selectedWindow.xRatio}
                    max={1 - selectedWindow.widthRatio}
                    onCommit={(v) =>
                      updateWindow(selectedWindow.id, { xRatio: v, showState: "normal" })
                    }
                  />
                  <PercentField
                    label="Y 顶边距"
                    value={selectedWindow.yRatio}
                    max={1 - selectedWindow.heightRatio}
                    onCommit={(v) =>
                      updateWindow(selectedWindow.id, { yRatio: v, showState: "normal" })
                    }
                  />
                  <PercentField
                    label="宽度"
                    value={selectedWindow.widthRatio}
                    min={0.08}
                    max={1 - selectedWindow.xRatio}
                    onCommit={(v) =>
                      updateWindow(selectedWindow.id, { widthRatio: v, showState: "normal" })
                    }
                  />
                  <PercentField
                    label="高度"
                    value={selectedWindow.heightRatio}
                    min={0.08}
                    max={1 - selectedWindow.yRatio}
                    onCommit={(v) =>
                      updateWindow(selectedWindow.id, { heightRatio: v, showState: "normal" })
                    }
                  />
                </div>
              </div>

              {/* 快捷对齐预设 */}
              <div>
                <p className="mb-2 text-[11px] font-medium uppercase tracking-wider text-ink-3">
                  快捷排版
                </p>
                <div className="grid grid-cols-2 gap-1.5">
                  <Button size="xs" variant="secondary" onClick={() => applyPreset(0, 0, 0.5, 1)}>
                    左半屏
                  </Button>
                  <Button size="xs" variant="secondary" onClick={() => applyPreset(0.5, 0, 0.5, 1)}>
                    右半屏
                  </Button>
                  <Button
                    size="xs"
                    variant="secondary"
                    onClick={() => applyPreset(0.1, 0.08, 0.8, 0.84)}
                  >
                    居中 80%
                  </Button>
                  <Button size="xs" variant="secondary" onClick={() => applyPreset(0, 0, 1, 1)}>
                    铺满工作区
                  </Button>
                </div>
              </div>

              {/* 只读识别依据卡片（评审 U6） */}
              <div className="rounded-md border border-line bg-canvas/60 p-3">
                <p className="flex items-center gap-1.5 text-[11px] font-medium text-ink-2">
                  <ShieldCheck size={13} className="text-accent" />
                  窗口识别依据（只读）
                </p>
                <dl className="mt-2 flex flex-col gap-1.5 font-mono text-[11px]">
                  <div className="flex justify-between gap-2">
                    <dt className="text-ink-3">进程</dt>
                    <dd className="truncate text-ink-2">{selectedWindow.processName}</dd>
                  </div>
                  <div className="flex justify-between gap-2">
                    <dt className="text-ink-3">类名</dt>
                    <dd className="truncate text-ink-2">{selectedWindow.className}</dd>
                  </div>
                  {selectedWindow.processPath ? (
                    <div>
                      <dt className="text-ink-3">路径</dt>
                      <dd
                        className="selectable mt-0.5 truncate text-ink-3"
                        title={selectedWindow.processPath}
                      >
                        {selectedWindow.processPath}
                      </dd>
                    </div>
                  ) : null}
                </dl>
              </div>

              <div className="mt-auto border-t border-line pt-3">
                <Button
                  size="sm"
                  variant="danger"
                  className="w-full"
                  onClick={() => removeWindow(selectedWindow.id)}
                >
                  <Trash2 size={13} /> 从工作空间移除该窗口
                </Button>
              </div>
            </div>
          ) : (
            <div className="flex max-h-[450px] flex-1 flex-col overflow-y-auto p-2">
              {windows.length === 0 ? (
                <p className="px-3 py-8 text-center text-aux text-ink-3">
                  当前没有窗口布局项
                </p>
              ) : (
                <ul className="flex flex-col gap-1">
                  {windows.map((w) => (
                    <li key={w.id}>
                      <button
                        type="button"
                        onMouseEnter={() => setHoveredId(w.id)}
                        onMouseLeave={() => setHoveredId((c) => (c === w.id ? null : c))}
                        onClick={() => setSelectedId(w.id)}
                        className={`flex w-full cursor-pointer items-center gap-2 rounded-md px-2.5 py-2 text-left transition-colors ${
                          hoveredId === w.id
                            ? "bg-raised text-ink"
                            : "text-ink-2 hover:bg-raised/60"
                        }`}
                      >
                        <div className="min-w-0 flex-1">
                          <p className="truncate text-aux font-medium text-ink">
                            {w.title || w.processName}
                          </p>
                          <p className="tabnum mt-0.5 truncate font-mono text-[10px] text-ink-3">
                            {w.processName} · {Math.round(w.widthRatio * 100)}%×
                            {Math.round(w.heightRatio * 100)}%
                          </p>
                        </div>
                        {w.showState !== "normal" ? (
                          <Badge>{w.showState === "maximized" ? "最大" : "最小"}</Badge>
                        ) : null}
                      </button>
                    </li>
                  ))}
                </ul>
              )}
            </div>
          )}
        </aside>
      </div>
    </Card>
  );
}

function PercentField({
  label,
  value,
  min = 0,
  max = 1,
  onCommit,
}: {
  label: string;
  value: number;
  min?: number;
  max?: number;
  onCommit: (next: number) => void;
}) {
  const pct = (value * 100).toFixed(1);
  const [text, setText] = useState(pct);

  useEffect(() => {
    setText((value * 100).toFixed(1));
  }, [value]);

  const commit = () => {
    const parsed = Number(text);
    if (!Number.isFinite(parsed)) {
      setText(pct);
      return;
    }
    const clamped = round4(clamp(parsed / 100, min, max));
    if (Math.abs(clamped - value) > 0.0001) {
      onCommit(clamped);
    } else {
      setText((clamped * 100).toFixed(1));
    }
  };

  return (
    <label className="flex flex-col gap-1">
      <span className="text-[11px] text-ink-3">{label}</span>
      <div className="relative flex items-center">
        <input
          value={text}
          onChange={(e) => setText(e.target.value)}
          onBlur={commit}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.currentTarget.blur();
            }
          }}
          inputMode="decimal"
          className="tabnum h-8 w-full rounded-md border border-line-strong/80 bg-canvas pl-2.5 pr-6 font-mono text-aux text-ink outline-none focus:border-accent"
        />
        <span className="pointer-events-none absolute right-2 font-mono text-[11px] text-ink-3">
          %
        </span>
      </div>
    </label>
  );
}

function clamp(v: number, min: number, max: number): number {
  if (!Number.isFinite(v)) return min;
  return Math.min(Math.max(v, min), Math.max(min, max));
}

function round4(v: number): number {
  return Math.round(v * 10000) / 10000;
}

function round2(v: number): number {
  return Math.round(v * 100) / 100;
}

export function toLayoutUpdates(windows: WorkspaceWindow[]): LayoutUpdate[] {
  return windows.map((w) => ({
    windowId: w.id,
    xRatio: w.xRatio,
    yRatio: w.yRatio,
    widthRatio: w.widthRatio,
    heightRatio: w.heightRatio,
    monitorKey: w.monitorKey,
    showState: w.showState,
  }));
}
