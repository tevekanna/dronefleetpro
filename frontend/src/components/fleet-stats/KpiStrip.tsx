// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import type { FleetDashboardKpis } from '../../types/fleetDashboard';
import { KpiCard } from './KpiCard';
import styles from './fleetStatsDashboard.module.css';

export interface KpiStripProps {
  kpis: FleetDashboardKpis;
}

function todayMinus30dISO(): string {
  return new Date(Date.now() - 30 * 24 * 60 * 60 * 1000).toISOString().split('T')[0];
}

export function KpiStrip({ kpis }: KpiStripProps) {
  return (
    <div className={styles.kpiStrip}>
      <KpiCard label="Aircraft in service" data={kpis.active_aircraft} format="int" to="/fleet" />
      <KpiCard
        label="Flight hours · 30d"
        data={kpis.flight_hours_30d}
        format="decimal"
        to={`/flights?date_from=${todayMinus30dISO()}`}
      />
      <KpiCard label="Open defects" data={kpis.open_defects} format="int" to="/maintenance?tab=defects" />
      <KpiCard label="Overdue maintenance" data={kpis.overdue_maintenance} format="int" to="/maintenance-schedules" />
      <KpiCard label="Batteries near EOL" data={kpis.batteries_near_eol} format="int" to="/fleet-stats?tab=batteries" />
    </div>
  );
}
