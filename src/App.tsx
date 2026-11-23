import { lazy, Suspense } from "react";
import {
  HashRouter,
  Navigate,
  Outlet,
  Route,
  Routes,
  useLocation,
} from "react-router-dom";
import { AppShell } from "@/components/AppShell";
import { ErrorBoundary } from "@/components/ErrorBoundary";
import { PageSkeleton } from "@/components/ui";
import { OverviewPage } from "@/pages/Overview";

// 路由级代码分割：主包只保留首页与外壳，其余页面按需加载，显著减小首屏体积
const WorkspacesPage = lazy(() =>
  import("@/pages/Workspaces").then((m) => ({ default: m.WorkspacesPage })),
);
const CapturePage = lazy(() =>
  import("@/pages/Capture").then((m) => ({ default: m.CapturePage })),
);
const RestorePage = lazy(() =>
  import("@/pages/Restore").then((m) => ({ default: m.RestorePage })),
);
const HistoryPage = lazy(() =>
  import("@/pages/History").then((m) => ({ default: m.HistoryPage })),
);
const SettingsPage = lazy(() =>
  import("@/pages/Settings").then((m) => ({ default: m.SettingsPage })),
);

function Shell() {
  const location = useLocation();
  return (
    <AppShell>
      {/* key 用路由路径：某个页面崩溃后，切换页面会重建边界，而不是一直卡在错误卡片上 */}
      <ErrorBoundary key={location.pathname}>
        <Suspense fallback={<PageSkeleton />}>
          <Outlet />
        </Suspense>
      </ErrorBoundary>
    </AppShell>
  );
}

export default function App() {
  return (
    <ErrorBoundary>
      <HashRouter>
        <Routes>
          <Route element={<Shell />}>
            <Route path="/" element={<OverviewPage />} />
            <Route path="/workspaces" element={<WorkspacesPage />} />
            <Route path="/capture" element={<CapturePage />} />
            <Route path="/restore" element={<RestorePage />} />
            <Route path="/history" element={<HistoryPage />} />
            <Route path="/settings" element={<SettingsPage />} />
            <Route path="*" element={<Navigate to="/" replace />} />
          </Route>
        </Routes>
      </HashRouter>
    </ErrorBoundary>
  );
}
