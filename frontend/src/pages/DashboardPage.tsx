// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useQuery } from '@tanstack/react-query';
import { fetchDashboard, fetchFlightMap } from '../api/dashboard';
import type { DashboardData, FlightMapPoint } from '../api/dashboard';
import { FleetFlightsMap } from '../components/map/FleetFlightsMap';
import { useFleetDashboard } from '../hooks/useFleetStats';
import { KpiStrip } from '../components/fleet-stats/KpiStrip';
import { FleetSummaryStrip } from '../components/dashboard/FleetSummaryStrip';
import { NeedsAttentionPanel } from '../components/dashboard/NeedsAttentionPanel';
import { RecentActivityPanel } from '../components/dashboard/RecentActivityPanel';
import { todayISO } from '../utils/date';
import styles from '../components/dashboard/dashboardLayout.module.css';

export function DashboardPage() {
  const fleetDashboardQuery = useFleetDashboard();
  const dashboardQuery = useQuery<DashboardData>({
    queryKey: ['dashboard'],
    queryFn: fetchDashboard,
  });
  const flightMapQuery = useQuery<FlightMapPoint[]>({
    queryKey: ['dashboard', 'flight-map'],
    queryFn: fetchFlightMap,
  });

  const isLoading = fleetDashboardQuery.isLoading || dashboardQuery.isLoading;
  const error = fleetDashboardQuery.error ?? dashboardQuery.error;

  return (
    <div className={styles.page}>
      <header className={styles.header}>
        <h1 className={styles.title}>Dashboard</h1>
        <span className={styles.headerDate}>{todayISO()}</span>
      </header>

      {isLoading && <p className={styles.loadingText}>Loading dashboard…</p>}

      {error && !isLoading && (
        <p className={styles.errorText}>
          Failed to load dashboard: {(error as Error).message}
        </p>
      )}

      {!isLoading && !error && fleetDashboardQuery.data && dashboardQuery.data && (
        <>
          <FleetSummaryStrip stats={dashboardQuery.data.stats} />
          <KpiStrip kpis={fleetDashboardQuery.data.kpis} />

          <section className={styles.panel} aria-labelledby="flight-map-title">
            <header className={styles.panelHeader}>
              <h2 id="flight-map-title" className={styles.panelTitle}>Flight locations</h2>
            </header>
            <FleetFlightsMap points={flightMapQuery.data ?? []} height="420px" />
          </section>

          <div className={styles.grid}>
            <NeedsAttentionPanel
              maintenanceAlerts={fleetDashboardQuery.data.maintenance_alerts}
              openDefects={dashboardQuery.data.open_defects}
              batteriesNearEol={fleetDashboardQuery.data.batteries_near_eol_top}
            />
            <RecentActivityPanel flights={dashboardQuery.data.recent_flights} />
          </div>
        </>
      )}
    </div>
  );
}
