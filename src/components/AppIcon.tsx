import { useMemo } from "react";
import { useQuery } from "@tanstack/react-query";
import { api } from "@/services/api";

/** 批量拉取并长期缓存 Windows .exe 真实图标 */
export function useAppIcons(paths: (string | null | undefined)[]) {
  const unique = useMemo(() => {
    const seen = new Set<string>();
    const out: string[] = [];
    for (const p of paths) {
      if (!p) continue;
      const trimmed = p.trim();
      if (!trimmed) continue;
      const key = trimmed.toLowerCase();
      if (!seen.has(key)) {
        seen.add(key);
        out.push(trimmed);
      }
    }
    return out.sort();
  }, [paths]);

  const q = useQuery({
    queryKey: ["app-icons", unique.join("|")],
    queryFn: () => api.appIconsBatch(unique),
    enabled: unique.length > 0,
    staleTime: 10 * 60_000,
    gcTime: 30 * 60_000,
    retry: 0,
  });

  return useMemo(() => {
    const raw = q.data ?? {};
    const map: Record<string, string> = {};
    for (const [k, v] of Object.entries(raw)) {
      map[k] = v;
      map[k.toLowerCase()] = v;
    }
    return map;
  }, [q.data]);
}

/** 展示 Windows 软件真实图标，读取不到时优雅降级为苹果风首字母圆角块 */
export function AppIcon({
  path,
  name,
  icons,
  size = 18,
}: {
  path?: string | null;
  name: string;
  icons?: Record<string, string>;
  size?: number;
}) {
  const src = path && icons ? icons[path] ?? icons[path.trim().toLowerCase()] : undefined;
  const cleanName = name.replace(/\.exe$/i, "").trim() || "?";

  if (src) {
    return (
      <img
        src={src}
        alt={cleanName}
        title={cleanName}
        width={size}
        height={size}
        draggable={false}
        className="shrink-0 rounded-[4px] object-contain"
        style={{ width: size, height: size }}
      />
    );
  }

  const letter = cleanName.slice(0, 1).toUpperCase();
  const palette = ["#0071e3", "#5856d6", "#30d158", "#ff9f0a", "#ff375f", "#32ade6"];
  let hash = 0;
  for (let i = 0; i < cleanName.length; i++) {
    hash = (hash * 31 + cleanName.charCodeAt(i)) | 0;
  }
  const bg = palette[Math.abs(hash) % palette.length];

  return (
    <span
      title={cleanName}
      className="inline-grid shrink-0 place-items-center rounded-[5px] font-semibold text-white"
      style={{
        width: size,
        height: size,
        backgroundColor: bg,
        fontSize: Math.max(9, Math.round(size * 0.54)),
        lineHeight: 1,
      }}
    >
      {letter}
    </span>
  );
}

/** 卡片与侧栏里的紧凑色标圆点 */
export function WorkspaceColorDot({
  color,
  size = 8,
}: {
  color?: string | null;
  size?: number;
}) {
  if (!color) return null;
  return (
    <span
      aria-hidden="true"
      className="inline-block shrink-0 rounded-full"
      style={{
        width: size,
        height: size,
        backgroundColor: color,
        boxShadow: `0 0 0 1.5px ${color}33`,
      }}
    />
  );
}

/** 卡片上的软件图标预览条（最多展示 5 个真实软件图标 + 剩余数量） */
export function AppIconStrip({
  paths,
  icons,
  totalApps,
}: {
  paths: string[];
  icons: Record<string, string>;
  totalApps: number;
}) {
  if (paths.length === 0) return null;
  const shown = paths.slice(0, 5);
  const rest = Math.max(0, totalApps - shown.length);
  return (
    <div className="flex items-center gap-1">
      {shown.map((p) => {
        const name = p.split(/[\\/]/).pop() ?? p;
        return (
          <span
            key={p}
            className="grid size-[22px] place-items-center rounded-[6px] border border-line bg-canvas/75"
          >
            <AppIcon path={p} name={name} icons={icons} size={15} />
          </span>
        );
      })}
      {rest > 0 ? (
        <span className="tabnum rounded-[6px] bg-raised/70 px-1.5 py-0.5 text-[10px] font-medium text-ink-3">
          +{rest}
        </span>
      ) : null}
    </div>
  );
}
