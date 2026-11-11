// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { lazy, Suspense } from 'react';
import { useQuery } from '@tanstack/react-query';
import { Routes, Route, Navigate, NavLink, useParams } from 'react-router-dom';
import { Plane, Settings, Users, Wrench, AlertTriangle, BarChart3, Boxes, LogOut, LayoutDashboard, ScrollText, LineChart, Database, UserCog, Tag, Archive } from 'lucide-react';
import { getToken, logout } from './api/client';
import { fetchOrgBranding } from './api/organisation';
import { LoginPage } from './pages/LoginPage';
import { RedeemTokenPage } from './pages/RedeemTokenPage';
import { MaintenanceBanner } from './components/layout/MaintenanceBanner';
import { Toaster } from './components/shared/Toaster';
import { RouteFallback } from './components/shared/RouteFallback';
import { RouteErrorBoundary } from './components/shared/RouteErrorBoundary';
import { ThemeToggle } from './components/ThemeToggle';
import { VersionBadge } from './components/VersionBadge';
import { SourceCodeLink } from './components/SourceCodeLink';
import { getRolesFromToken } from './utils/auth';
import styles from './App.module.css';

// Route-level code splitting: every page under AppShell is lazy-loaded so the
// initial bundle stays small. LoginPage and RedeemTokenPage are kept eager
// (unauthenticated entry — avoids a suspense flash on first paint).
const FlightLogPage = lazy(() =>
  import('./pages/FlightLogPage').then((m) => ({ default: m.FlightLogPage })),
);
const FlightDetailPage = lazy(() =>
  import('./pages/FlightDetailPage').then((m) => ({ default: m.FlightDetailPage })),
);
const FleetPage = lazy(() =>
  import('./pages/FleetPage').then((m) => ({ default: m.FleetPage })),
);
const AircraftDetailPage = lazy(() =>
  import('./pages/AircraftDetailPage').then((m) => ({ default: m.AircraftDetailPage })),
);
const MaintenancePage = lazy(() =>
  import('./pages/MaintenancePage').then((m) => ({ default: m.MaintenancePage })),
);
const MaintenanceDetailPage = lazy(() =>
  import('./pages/MaintenanceDetailPage').then((m) => ({ default: m.MaintenanceDetailPage })),
);
const MaintenanceSchedulesPage = lazy(() =>
  import('./pages/MaintenanceSchedulesPage').then((m) => ({ default: m.MaintenanceSchedulesPage })),
);
const MaintenanceHistoryPage = lazy(() =>
  import('./pages/MaintenanceHistoryPage').then((m) => ({ default: m.MaintenanceHistoryPage })),
);
const MaintenanceTemplatesPage = lazy(() =>
  import('./pages/MaintenanceTemplatesPage').then((m) => ({ default: m.MaintenanceTemplatesPage })),
);
const MaintenanceHubPage = lazy(() =>
  import('./pages/MaintenanceHubPage').then((m) => ({ default: m.MaintenanceHubPage })),
);
const AuditLogPage = lazy(() =>
  import('./pages/AuditLogPage').then((m) => ({ default: m.AuditLogPage })),
);
const FleetStatsPage = lazy(() =>
  import('./pages/FleetStatsPage').then((m) => ({ default: m.FleetStatsPage })),
);
const BatteryStatsPage = lazy(() =>
  import('./pages/BatteryStatsPage').then((m) => ({ default: m.BatteryStatsPage })),
);
const BatteryModelComparisonPage = lazy(() =>
  import('./pages/BatteryModelComparisonPage').then((m) => ({ default: m.BatteryModelComparisonPage })),
);
const ControllerStatsPage = lazy(() =>
  import('./pages/ControllerStatsPage').then((m) => ({ default: m.ControllerStatsPage })),
);
const PilotsPage = lazy(() =>
  import('./pages/PilotsPage').then((m) => ({ default: m.PilotsPage })),
);
const PilotDetailPage = lazy(() =>
  import('./pages/PilotDetailPage').then((m) => ({ default: m.PilotDetailPage })),
);
const IncidentsPage = lazy(() =>
  import('./pages/IncidentsPage').then((m) => ({ default: m.IncidentsPage })),
);
const IncidentDetailPage = lazy(() =>
  import('./pages/IncidentDetailPage').then((m) => ({ default: m.IncidentDetailPage })),
);
const ReportsPage = lazy(() =>
  import('./pages/ReportsPage').then((m) => ({ default: m.ReportsPage })),
);
const RetiredEquipmentPage = lazy(() =>
  import('./pages/RetiredEquipmentPage').then((m) => ({ default: m.RetiredEquipmentPage })),
);
const SettingsPage = lazy(() =>
  import('./pages/SettingsPage').then((m) => ({ default: m.SettingsPage })),
);
const UserManagementPage = lazy(() =>
  import('./pages/UserManagementPage').then((m) => ({ default: m.UserManagementPage })),
);
const AircraftModelsPage = lazy(() =>
  import('./pages/AircraftModelsPage').then((m) => ({ default: m.AircraftModelsPage })),
);
const AircraftModelDetailPage = lazy(() =>
  import('./pages/AircraftModelDetailPage').then((m) => ({ default: m.AircraftModelDetailPage })),
);
const ChecklistEditorPage = lazy(() =>
  import('./pages/ChecklistEditorPage').then((m) => ({ default: m.ChecklistEditorPage })),
);
const ChecklistViewerPage = lazy(() =>
  import('./pages/ChecklistViewerPage').then((m) => ({ default: m.ChecklistViewerPage })),
);
const InstructionTemplatesPage = lazy(() =>
  import('./pages/InstructionTemplatesPage').then((m) => ({ default: m.InstructionTemplatesPage })),
);
const DashboardPage = lazy(() =>
  import('./pages/DashboardPage').then((m) => ({ default: m.DashboardPage })),
);
const DataManagementPage = lazy(() =>
  import('./pages/DataManagementPage').then((m) => ({ default: m.DataManagementPage })),
);

type NavItem = {
  path: string;
  label: string;
  icon: typeof LayoutDashboard;
  adminOnly?: boolean;
  allowedRoles?: string[];
};

type NavSection = {
  id: string;
  label: string;
  items: NavItem[];
};

const NAV_SECTIONS: NavSection[] = [
  {
    id: 'operations',
    label: 'Operations',
    items: [
      { path: '/dashboard', label: 'Dashboard', icon: LayoutDashboard },
      { path: '/flights', label: 'Flight Log', icon: Plane },
      { path: '/incidents', label: 'Incidents', icon: AlertTriangle },
    ],
  },
  {
    id: 'crew-fleet',
    label: 'Crew & Fleet',
    items: [
      { path: '/pilots', label: 'Pilots', icon: Users },
      { path: '/fleet', label: 'Fleet', icon: Boxes },
      { path: '/fleet-stats', label: 'Fleet Statistics', icon: LineChart },
      { path: '/aircraft-models', label: 'Aircraft Models', icon: Tag },
    ],
  },
  {
    id: 'maintenance-compliance',
    label: 'Maintenance & Compliance',
    items: [
      { path: '/maintenance', label: 'Maintenance', icon: Wrench, allowedRoles: ['admin', 'technician', 'safety_officer', 'pilot'] },
      { path: '/audit-log', label: 'Audit Log', icon: ScrollText, allowedRoles: ['admin', 'safety_officer'] },
      { path: '/reports', label: 'Reports', icon: BarChart3 },
      { path: '/retired-equipment', label: 'Retired Equipment', icon: Archive },
    ],
  },
  {
    id: 'admin',
    label: 'Admin',
    items: [
      { path: '/users', label: 'Users', icon: UserCog, adminOnly: true },
      { path: '/data-management', label: 'Data Management', icon: Database, adminOnly: true },
      { path: '/settings', label: 'Settings', icon: Settings, adminOnly: true },
    ],
  },
];

// Legacy per-aircraft maintenance page has been merged into the aircraft
// workspace. Preserve inbound links (Hub row click, schedule deep links,
// bookmarks) by redirecting to the workspace's Maintenance tab.
function AircraftMaintenanceRedirect() {
  const { id } = useParams<{ id: string }>();
  return <Navigate to={`/aircraft/${id}?tab=maintenance`} replace />;
}

function ProtectedRoute({ children }: { children: React.ReactNode }) {
  if (!getToken()) {
    return <Navigate to="/login" replace />;
  }
  return <>{children}</>;
}

function RoleRoute({
  children,
  allowedRoles,
}: {
  children: React.ReactNode;
  allowedRoles: string[];
}) {
  const roles = getRolesFromToken();
  if (!roles.some((r) => allowedRoles.includes(r))) {
    return <Navigate to="/dashboard" replace />;
  }
  return <>{children}</>;
}

async function handleLogout() {
  await logout();
  window.location.href = import.meta.env.BASE_URL + 'login';
}

function AppShell({ children }: { children: React.ReactNode }) {
  const { data: branding } = useQuery({
    queryKey: ['orgBranding'],
    queryFn: fetchOrgBranding,
    staleTime: 5 * 60 * 1000,
  });
  // Fallback: if only one variant is set, use it for both themes.
  const darkSrc = branding?.dark?.data_url ?? branding?.light?.data_url ?? null;
  const lightSrc = branding?.light?.data_url ?? branding?.dark?.data_url ?? null;

  return (
    <div className={styles.shell}>
      <aside className={styles.sidebar}>
        {(darkSrc || lightSrc) && (
          <div className={styles.orgLogoWrap}>
            {darkSrc && (
              <img src={darkSrc} alt="Organisation logo" className={styles.orgLogoDark} />
            )}
            {lightSrc && (
              <img src={lightSrc} alt="Organisation logo" className={styles.orgLogoLight} />
            )}
          </div>
        )}
        <div className={styles.logo}>
          <Plane size={20} />
          <span>DroneFleetPro</span>
        </div>
        <div className={styles.nav}>
          {NAV_SECTIONS.map((section) => {
            const roles = getRolesFromToken();
            const visibleItems = section.items.filter((item) => {
              if (item.adminOnly && !roles.includes('admin')) return false;
              if (item.allowedRoles && !roles.some((r) => item.allowedRoles!.includes(r))) return false;
              return true;
            });
            if (visibleItems.length === 0) return null;
            const headerId = `nav-section-${section.id}`;
            return (
              <nav
                key={section.id}
                aria-labelledby={headerId}
                className={styles.navSection}
              >
                <h3 id={headerId} className={styles.navSectionHeader}>
                  {section.label}
                </h3>
                {visibleItems.map(({ path, label, icon: Icon }) => (
                  <NavLink
                    key={path}
                    to={path}
                    className={({ isActive }) =>
                      `${styles.navLink} ${isActive ? styles.navLinkActive : ''}`
                    }
                  >
                    <Icon size={18} />
                    <span>{label}</span>
                  </NavLink>
                ))}
              </nav>
            );
          })}
        </div>
        <div className={styles.sidebarFooter}>
          <button onClick={handleLogout} className={styles.logoutButton}>
            <LogOut size={18} />
            <span>Sign Out</span>
          </button>
          <div className={styles.sidebarSource}>
            <SourceCodeLink />
          </div>
        </div>
      </aside>
      <main className={styles.main}>
        <header className={styles.topBar}>
          <div className={styles.topBarLeft}>
            <div className={styles.orgName}>DroneFleetPro</div>
            <span className={styles.topBarSeparator} aria-hidden="true" />
            <VersionBadge />
          </div>
          <div className={styles.topBarRight}>
            <ThemeToggle />
            <div className={styles.userMenu}>ADM</div>
          </div>
        </header>
        <MaintenanceBanner />
        <div className={styles.content}>
          <RouteErrorBoundary>
            <Suspense fallback={<RouteFallback />}>
              {children}
            </Suspense>
          </RouteErrorBoundary>
        </div>
      </main>
      <Toaster />
    </div>
  );
}

export function App() {
  return (
    <Routes>
      <Route path="/login" element={<LoginPage />} />
      <Route path="/redeem" element={<RedeemTokenPage />} />
      <Route
        path="*"
        element={
          <ProtectedRoute>
            <AppShell>
              <Routes>
                <Route path="/" element={<Navigate to="/dashboard" replace />} />
                <Route path="/dashboard" element={<DashboardPage />} />
                <Route path="/flights" element={<FlightLogPage />} />
                <Route path="/flights/:id" element={<FlightDetailPage />} />
                <Route path="/fleet" element={<FleetPage />} />
                <Route path="/aircraft/:id" element={<AircraftDetailPage />} />
                <Route
                  path="/maintenance"
                  element={
                    <RoleRoute allowedRoles={['admin','technician','safety_officer','pilot']}>
                      <MaintenanceHubPage />
                    </RoleRoute>
                  }
                />
                <Route
                  path="/maintenance-records"
                  element={
                    <RoleRoute allowedRoles={['admin','technician','safety_officer','pilot']}>
                      <MaintenancePage />
                    </RoleRoute>
                  }
                />
                <Route
                  path="/maintenance/aircraft/:id"
                  element={
                    <RoleRoute allowedRoles={['admin','technician','safety_officer','pilot']}>
                      <AircraftMaintenanceRedirect />
                    </RoleRoute>
                  }
                />
                <Route path="/maintenance/:id" element={<MaintenanceDetailPage />} />
                <Route
                  path="/maintenance-schedules"
                  element={
                    <RoleRoute allowedRoles={['admin','technician','safety_officer','pilot']}>
                      <MaintenanceSchedulesPage />
                    </RoleRoute>
                  }
                />
                <Route
                  path="/maintenance-schedules/:scheduleId/history"
                  element={
                    <RoleRoute allowedRoles={['admin','technician','safety_officer','pilot']}>
                      <MaintenanceHistoryPage />
                    </RoleRoute>
                  }
                />
                <Route
                  path="/maintenance-templates"
                  element={
                    <RoleRoute allowedRoles={['admin','technician','safety_officer','pilot']}>
                      <MaintenanceTemplatesPage />
                    </RoleRoute>
                  }
                />
                <Route
                  path="/maintenance-instruction-templates"
                  element={
                    <RoleRoute allowedRoles={['admin','technician','safety_officer','pilot']}>
                      <InstructionTemplatesPage />
                    </RoleRoute>
                  }
                />
                <Route
                  path="/audit-log"
                  element={
                    <RoleRoute allowedRoles={['admin', 'safety_officer']}>
                      <AuditLogPage />
                    </RoleRoute>
                  }
                />
                <Route path="/fleet-stats" element={<FleetStatsPage />} />
                <Route path="/fleet-stats/batteries/:id" element={<BatteryStatsPage />} />
                <Route path="/fleet-stats/batteries/model/:model" element={<BatteryModelComparisonPage />} />
                <Route path="/fleet-stats/controllers/:id" element={<ControllerStatsPage />} />
                <Route path="/pilots" element={<PilotsPage />} />
                <Route path="/pilots/:id" element={<PilotDetailPage />} />
                <Route path="/incidents" element={<IncidentsPage />} />
                <Route path="/incidents/:id" element={<IncidentDetailPage />} />
                <Route path="/reports" element={<ReportsPage />} />
                <Route path="/retired-equipment" element={<RetiredEquipmentPage />} />
                <Route path="/settings" element={<RoleRoute allowedRoles={['admin']}><SettingsPage /></RoleRoute>} />
                <Route path="/users" element={<UserManagementPage />} />
                <Route path="/aircraft-models" element={<AircraftModelsPage />} />
                <Route path="/aircraft-models/:id" element={<AircraftModelDetailPage />} />
                <Route
                  path="/aircraft-models/:modelId/checklists/new"
                  element={<ChecklistEditorPage />}
                />
                <Route
                  path="/aircraft-models/:modelId/checklists/:checklistId/edit"
                  element={<ChecklistEditorPage />}
                />
                <Route path="/checklists/:id" element={<ChecklistViewerPage />} />
                <Route path="/data-management" element={<DataManagementPage />} />
              </Routes>
            </AppShell>
          </ProtectedRoute>
        }
      />
    </Routes>
  );
}
