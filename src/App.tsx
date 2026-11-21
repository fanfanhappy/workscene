import { HashRouter, Navigate, Outlet, Route, Routes } from "react-router-dom";
import { AppShell } from "@/components/AppShell";
import { OverviewPage } from "@/pages/Overview";
import { WorkspacesPage } from "@/pages/Workspaces";
import { CapturePage } from "@/pages/Capture";
import { RestorePage } from "@/pages/Restore";
import { HistoryPage } from "@/pages/History";
import { SettingsPage } from "@/pages/Settings";

function Shell() {
  return (
    <AppShell>
      <Outlet />
    </AppShell>
  );
}

export default function App() {
  return (
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
  );
}
