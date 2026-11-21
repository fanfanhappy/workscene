import { HashRouter, Outlet, Route, Routes } from "react-router-dom";
import { AppShell } from "@/components/AppShell";
import { OverviewPage } from "@/pages/Overview";
import { CapturePage } from "@/pages/Capture";
import { RestorePage } from "@/pages/Restore";
import { WorkspacesPage } from "@/pages/Workspaces";

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
        </Route>
      </Routes>
    </HashRouter>
  );
}
