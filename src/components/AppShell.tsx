import { useEffect, useState } from "react";
import { NavLink, useLocation } from "react-router-dom";
import {
  Boxes,
  Camera,
  LayoutDashboard,
  Moon,
  RotateCcw,
  Sun,
} from "lucide-react";
import { api } from "@/services/api";
import type { AppInfo } from "@/types/contract";

const NAV = [
  { to: "/", label: "总览", icon: LayoutDashboard, end: true },
  { to: "/workspaces", label: "工作空间", icon: Boxes },
  { to: "/capture", label: "现场捕获", icon: Camera },
  { to: "/restore", label: "恢复中心", icon: RotateCcw },
];

function useTheme() {
  const [theme, setTheme] = useState<"dark" | "light">(() => {
    const saved = localStorage.getItem("workscene.theme");
    return saved === "light" ? "light" : "dark";
  });

  useEffect(() => {
    document.documentElement.dataset.theme = theme;
    localStorage.setItem("workscene.theme", theme);
  }, [theme]);

  return { theme, toggle: () => setTheme((t) => (t === "dark" ? "light" : "dark")) };
}

export function AppShell({ children }: { children: React.ReactNode }) {
  const { theme, toggle } = useTheme();
  const [info, setInfo] = useState<AppInfo | null>(null);
  const location = useLocation();

  useEffect(() => {
    api
      .getAppInfo()
      .then(setInfo)
      .catch(() => setInfo(null));
  }, []);

  const current = NAV.find((n) => (n.end ? location.pathname === n.to : location.pathname.startsWith(n.to)));

  return (
    <div className="flex h-full">
      <aside className="flex w-[216px] shrink-0 flex-col border-r border-line bg-sidebar">
        <div className="flex h-14 items-center gap-2.5 px-4">
          <span className="grid size-7 place-items-center rounded-md bg-accent text-accent-ink">
            <Boxes size={16} strokeWidth={2.2} />
          </span>
          <div className="leading-tight">
            <p className="text-body font-semibold tracking-tight">WorkScene</p>
            <p className="text-[11px] text-ink-3">工作现场管理器</p>
          </div>
        </div>

        <nav className="flex flex-1 flex-col gap-0.5 px-2 pt-2" aria-label="主导航">
          {NAV.map(({ to, label, icon: Icon, end }) => (
            <NavLink
              key={to}
              to={to}
              end={end}
              className={({ isActive }) =>
                `flex h-9 items-center gap-2.5 rounded-md px-2.5 text-body transition-colors duration-150 ${
                  isActive
                    ? "bg-raised text-ink"
                    : "text-ink-2 hover:bg-raised/60 hover:text-ink"
                }`
              }
            >
              <Icon size={16} strokeWidth={2} />
              {label}
            </NavLink>
          ))}
        </nav>

        <div className="border-t border-line p-3">
          <button
            onClick={toggle}
            className="flex h-8 w-full items-center gap-2 rounded-md px-2 text-aux text-ink-3 hover:bg-raised hover:text-ink"
          >
            {theme === "dark" ? <Sun size={14} /> : <Moon size={14} />}
            {theme === "dark" ? "切换到浅色" : "切换到深色"}
          </button>
          <p className="mt-2 px-2 text-[11px] text-ink-3">
            v{info?.version ?? "0.1.0"} · Tauri {info?.tauriVersion ?? "2"}
          </p>
        </div>
      </aside>

      <main className="flex min-w-0 flex-1 flex-col">
        <header className="flex h-14 shrink-0 items-center justify-between border-b border-line px-6">
          <h1 className="text-card font-semibold tracking-tight">
            {current?.label ?? "WorkScene"}
          </h1>
          <p className="max-w-[52%] truncate font-mono text-[11px] text-ink-3" title={info?.dataDir}>
            {info?.dataDir ?? ""}
          </p>
        </header>
        <div className="min-h-0 flex-1 overflow-y-auto px-6 py-5">{children}</div>
      </main>
    </div>
  );
}
