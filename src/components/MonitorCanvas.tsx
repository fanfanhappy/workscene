import { useId, useState } from "react";
import type { DisplayInfo, WindowInfo } from "@/types/contract";

/**
 * 只读可视化画布（UI 规范 §7.4 / UI-06a）：
 * 按真实物理相对位置绘制显示器、工作区与窗口矩形；
 * 支持悬停高亮与可选点击选中，解决多窗口重叠时看不清的问题。
 */
export function MonitorCanvas({
  displays,
  windows,
  height = 210,
  highlightMonitor,
  selectedHwnd,
  onSelectWindow,
}: {
  displays: DisplayInfo[];
  windows: WindowInfo[];
  height?: number;
  highlightMonitor?: string | null;
  selectedHwnd?: number | null;
  onSelectWindow?: (w: WindowInfo) => void;
}) {
  const canvasId = useId();
  const [hoveredHwnd, setHoveredHwnd] = useState<number | null>(null);

  if (displays.length === 0) {
    return (
      <div className="grid h-[140px] place-items-center rounded-md border border-dashed border-line bg-canvas/50 text-aux text-ink-3">
        无显示器信息
      </div>
    );
  }

  const minX = Math.min(...displays.map((d) => d.x));
  const minY = Math.min(...displays.map((d) => d.y));
  const maxX = Math.max(...displays.map((d) => d.x + d.width));
  const maxY = Math.max(...displays.map((d) => d.y + d.height));

  const spanW = Math.max(1, maxX - minX);
  const pad = Math.max(24, spanW * 0.02);
  const fontSize = Math.max(12, spanW / 52);
  // 显示器标签画在屏幕下方的留白里，避免与窗口标签互相遮挡
  const labelBand = fontSize * 1.9;
  const vbX = minX - pad;
  const vbY = minY - pad;
  const vbW = Math.max(1, spanW + pad * 2);
  const vbH = Math.max(1, maxY - minY + pad * 2 + labelBand);
  const gap = vbW / 520;
  const activeHwnd = hoveredHwnd ?? selectedHwnd ?? null;

  // 把激活窗口排到最后绘制，避免被其他窗口遮挡
  const orderedWindows = [...windows].sort((a, b) => {
    const aActive = a.hwnd === activeHwnd ? 1 : 0;
    const bActive = b.hwnd === activeHwnd ? 1 : 0;
    return aActive - bActive;
  });

  const inspected = windows.find((w) => w.hwnd === activeHwnd) ?? null;

  return (
    <div className="flex min-w-0 flex-col gap-2">
      <div className="overflow-hidden rounded-[10px] border border-line bg-canvas/80 p-2">
        <svg
          viewBox={`${vbX} ${vbY} ${vbW} ${vbH}`}
          style={{ width: "100%", height, display: "block" }}
          preserveAspectRatio="xMidYMid meet"
          role="img"
          aria-label="显示器与窗口布局预览"
        >
          {displays.map((d) => {
            const active = highlightMonitor === d.monitorKey;
            return (
              <g key={d.monitorKey}>
                <rect
                  x={d.x}
                  y={d.y}
                  width={d.width}
                  height={d.height}
                  rx={vbW / 180}
                  fill="var(--color-surface)"
                  stroke={active ? "var(--color-accent)" : "var(--color-line-strong)"}
                  strokeWidth={vbW / 380}
                />
                <rect
                  x={d.workX}
                  y={d.workY}
                  width={d.workWidth}
                  height={d.workHeight}
                  fill="none"
                  stroke="var(--color-line)"
                  strokeDasharray={`${vbW / 180} ${vbW / 180}`}
                  strokeWidth={vbW / 700}
                />
                <text
                  x={d.x + d.width / 2}
                  y={d.y + d.height + fontSize * 1.35}
                  textAnchor="middle"
                  fill={active ? "var(--color-accent)" : "var(--color-ink-3)"}
                  fontSize={fontSize}
                  fontFamily="var(--font-sans)"
                  fontWeight={d.isPrimary ? 600 : 500}
                >
                  {d.isPrimary ? "主屏" : "副屏"} · {d.width}×{d.height} ·{" "}
                  {Math.round(d.scaleFactor * 100)}%
                </text>
              </g>
            );
          })}

          {orderedWindows.map((w, i) => {
            const isSel = w.hwnd === selectedHwnd;
            const isHover = w.hwnd === hoveredHwnd;
            const highlighted = isSel || isHover;
            const rx0 = w.x + gap;
            const ry0 = w.y + gap;
            const rw = Math.max(40, w.width - gap * 2);
            const rh = Math.max(30, w.height - gap * 2);
            const avgDispW = vbW / Math.max(1, displays.length);
            const labelFont = Math.max(22, Math.min(avgDispW / 38, rh * 0.22));
            const barH = Math.min(rh * 0.3, labelFont * 1.7);
            const appLabel = (w.processName || w.title || "窗口").replace(/\.exe$/i, "");
            const stateTag = w.isMaximized ? " [全屏]" : w.isMinimized ? " [最小化]" : "";
            const showLabel = rw >= labelFont * 3.2 && rh >= labelFont * 1.8;
            const radius = vbW / 340;
            const clipId = `mc-${canvasId}-${i}-${w.hwnd}`;
            return (
              <g
                key={`${w.hwnd}-${i}`}
                onMouseEnter={() => setHoveredHwnd(w.hwnd)}
                onMouseLeave={() => setHoveredHwnd((cur) => (cur === w.hwnd ? null : cur))}
                onClick={() => onSelectWindow?.(w)}
                className={onSelectWindow ? "cursor-pointer" : undefined}
                style={{ transition: "opacity 150ms" }}
                opacity={activeHwnd !== null && !highlighted ? 0.55 : 1}
              >
                <defs>
                  <clipPath id={clipId}>
                    <rect x={rx0} y={ry0} width={rw} height={rh} rx={radius} />
                  </clipPath>
                </defs>
                <rect
                  x={rx0}
                  y={ry0}
                  width={rw}
                  height={rh}
                  rx={radius}
                  fill="var(--color-accent)"
                  fillOpacity={highlighted ? 0.3 : 0.12}
                >
                  <title>{`${w.title || w.processName} (${w.width}×${w.height})`}</title>
                </rect>
                {/* 模拟标题栏，让矩形更像真实窗口 */}
                <rect
                  x={rx0}
                  y={ry0}
                  width={rw}
                  height={barH}
                  clipPath={`url(#${clipId})`}
                  fill="var(--color-accent)"
                  fillOpacity={highlighted ? 0.5 : 0.24}
                  style={{ pointerEvents: "none" }}
                />
                <rect
                  x={rx0}
                  y={ry0}
                  width={rw}
                  height={rh}
                  rx={radius}
                  fill="none"
                  stroke="var(--color-accent)"
                  strokeOpacity={highlighted ? 1 : 0.55}
                  strokeWidth={highlighted ? vbW / 320 : vbW / 720}
                  style={{ pointerEvents: "none" }}
                />
                {showLabel ? (
                  <text
                    x={rx0 + labelFont * 0.55}
                    y={ry0 + barH / 2}
                    dominantBaseline="central"
                    clipPath={`url(#${clipId})`}
                    fill={highlighted ? "var(--color-ink)" : "var(--color-ink-2)"}
                    fontSize={labelFont}
                    fontWeight={highlighted ? 600 : 500}
                    fontFamily="var(--font-sans)"
                    style={{ pointerEvents: "none" }}
                  >
                    {appLabel}
                    {stateTag}
                  </text>
                ) : null}
              </g>
            );
          })}
        </svg>
      </div>

      <div className="flex min-h-6 min-w-0 items-center justify-between gap-3 overflow-hidden px-0.5 text-aux text-ink-3">
        {inspected ? (
          <>
            <span className="min-w-0 flex-1 truncate text-ink-2">
              <span className="font-medium text-ink">
                {inspected.title || inspected.processName}
              </span>{" "}
              <span className="text-ink-3">({inspected.processName})</span>
            </span>
            <span className="tabnum shrink-0 font-mono text-[11px] text-ink-3">
              ({inspected.x}, {inspected.y}) · {inspected.width}×{inspected.height}
              {inspected.isMaximized ? " · 最大化" : inspected.isMinimized ? " · 最小化" : ""}
            </span>
          </>
        ) : (
          <span className="min-w-0 flex-1 truncate">
            悬停窗口矩形查看详情 · 共 {displays.length} 台显示器、{windows.length} 个窗口
          </span>
        )}
      </div>
    </div>
  );
}
