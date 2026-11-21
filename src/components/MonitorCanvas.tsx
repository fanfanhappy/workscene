import type { DisplayInfo, WindowInfo } from "@/types/contract";

/**
 * 只读可视化画布：把显示器与窗口矩形按真实相对位置绘制。
 * 与 UI 规范 §7.4 对应；M1 只做只读（编辑能力见评审 UI-06b）。
 */
export function MonitorCanvas({
  displays,
  windows,
  height = 200,
  highlightMonitor,
}: {
  displays: DisplayInfo[];
  windows: WindowInfo[];
  height?: number;
  highlightMonitor?: string | null;
}) {
  if (displays.length === 0) {
    return (
      <div className="grid h-[120px] place-items-center rounded-md border border-dashed border-line text-aux text-ink-3">
        无显示器信息
      </div>
    );
  }

  const minX = Math.min(...displays.map((d) => d.x));
  const minY = Math.min(...displays.map((d) => d.y));
  const maxX = Math.max(...displays.map((d) => d.x + d.width));
  const maxY = Math.max(...displays.map((d) => d.y + d.height));

  const pad = 8;
  const vbX = minX - pad;
  const vbY = minY - pad;
  const vbW = maxX - minX + pad * 2;
  const vbH = maxY - minY + pad * 2;
  const fontSize = Math.max(10, vbW / 46);

  return (
    <svg
      viewBox={`${vbX} ${vbY} ${vbW} ${vbH}`}
      style={{ width: "100%", height }}
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
              rx={vbW / 200}
              fill="var(--color-surface)"
              stroke={active ? "var(--color-accent)" : "var(--color-line-strong)"}
              strokeWidth={vbW / 400}
            />
            <rect
              x={d.workX}
              y={d.workY}
              width={d.workWidth}
              height={d.workHeight}
              fill="none"
              stroke="var(--color-line)"
              strokeDasharray={`${vbW / 200} ${vbW / 200}`}
              strokeWidth={vbW / 800}
            />
            <text
              x={d.x + fontSize}
              y={d.y + fontSize * 1.5}
              fill="var(--color-ink-3)"
              fontSize={fontSize}
              fontFamily="var(--font-mono)"
            >
              {d.isPrimary ? "主屏" : "副屏"} {d.width}×{d.height}
            </text>
          </g>
        );
      })}

      {windows.map((w, i) => (
        <rect
          key={`${w.hwnd}-${i}`}
          x={w.x}
          y={w.y}
          width={w.width}
          height={w.height}
          rx={vbW / 400}
          fill="var(--color-accent)"
          fillOpacity={0.16}
          stroke="var(--color-accent)"
          strokeWidth={vbW / 700}
        >
          <title>{w.title || w.processName}</title>
        </rect>
      ))}
    </svg>
  );
}
