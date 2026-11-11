// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { CheckCircle2 } from 'lucide-react';
import type { MostUsedAircraftRow } from '../../types/fleetDashboard';
import { formatDaysAgo } from '../../pages/fleetStatsFormat';
import { EmptyState } from './EmptyState';
import styles from './fleetStatsDashboard.module.css';

export interface MostUsedAircraftWidgetProps {
  rows: MostUsedAircraftRow[];
}

function formatHours(hours: number): string {
  if (!Number.isFinite(hours) || hours <= 0) return '0.0h';
  return `${hours.toFixed(1)}h`;
}

export function MostUsedAircraftWidget({ rows }: MostUsedAircraftWidgetProps) {
  if (rows.length === 0) {
    return <EmptyState message="No active aircraft yet." />;
  }

  // Aircraft detail page does not yet exist — see App.tsx routing.
  // Rows render as non-link with a tooltip per PRD P2.5.
  return (
    <table className={styles.widgetTable}>
      <thead className={styles.widgetTableHead}>
        <tr>
          <th>ID</th>
          <th>Model</th>
          <th className={styles.cellRight}>Hours</th>
          <th className={styles.cellRight}>Flights</th>
          <th className={styles.cellRight}>Last flight</th>
        </tr>
      </thead>
      <tbody>
        {rows.map((row) => (
          <tr
            key={row.aircraft_id}
            className={`${styles.widgetTableRow} ${styles.widgetTableRowDisabled}`}
            title="Detail view not yet available"
          >
            <td className={styles.cellId}>{row.equipment_id ?? row.serial_number}</td>
            <td>{row.model_name ?? <span className={styles.cellMuted}>—</span>}</td>
            <td className={`${styles.cellRight} ${styles.cellMono}`}>
              {formatHours(row.total_hours)}
            </td>
            <td className={`${styles.cellRight} ${styles.cellMono}`}>{row.total_flights}</td>
            <td className={`${styles.cellRight} ${styles.cellMuted}`}>
              {formatDaysAgo(row.last_flight)}
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

export function AllAircraftHealthyEmpty() {
  return (
    <EmptyState
      icon={<CheckCircle2 size={24} color="var(--color-ok)" aria-hidden="true" />}
      message="All quiet — no flights yet."
      ok
    />
  );
}
