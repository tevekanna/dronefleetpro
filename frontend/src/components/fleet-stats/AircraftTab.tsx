// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useAircraftByModel } from '../../hooks/useFleetStats';
import { formatDaysAgo } from '../../pages/fleetStatsFormat';
import { EmptyState } from './EmptyState';
import styles from './fleetStatsDashboard.module.css';

function formatHours(hours: number): string {
  if (!Number.isFinite(hours) || hours <= 0) return '0.0h';
  return `${hours.toFixed(1)}h`;
}

export function AircraftTab() {
  const { data, isLoading, error } = useAircraftByModel();

  if (isLoading) return <p style={{ color: 'var(--color-text-secondary)' }}>Loading…</p>;
  if (error) {
    return (
      <p style={{ color: 'var(--color-critical)' }}>
        Failed to load aircraft stats: {(error as Error).message}
      </p>
    );
  }
  if (!data) return null;

  return (
    <>
      <section className={styles.section}>
        <header className={styles.sectionHeader}>
          <h2 className={styles.sectionTitle}>By model</h2>
          <span className={styles.sectionMeta}>active aircraft only</span>
        </header>
        {data.by_model.length === 0 ? (
          <EmptyState message="No active aircraft yet." />
        ) : (
          <table className={styles.widgetTable}>
            <thead className={styles.widgetTableHead}>
              <tr>
                <th>Model</th>
                <th className={styles.cellRight}>Aircraft</th>
                <th className={styles.cellRight}>Total hours</th>
                <th className={styles.cellRight}>Total flights</th>
                <th className={styles.cellRight}>Open defects</th>
              </tr>
            </thead>
            <tbody>
              {data.by_model.map((row) => (
                <tr
                  key={row.model_id ?? row.model_name}
                  className={`${styles.widgetTableRow} ${styles.widgetTableRowDisabled}`}
                >
                  <td>{row.model_name}</td>
                  <td className={`${styles.cellRight} ${styles.cellMono}`}>
                    {row.aircraft_count}
                  </td>
                  <td className={`${styles.cellRight} ${styles.cellMono}`}>
                    {formatHours(row.total_hours)}
                  </td>
                  <td className={`${styles.cellRight} ${styles.cellMono}`}>
                    {row.total_flights}
                  </td>
                  <td
                    className={`${styles.cellRight} ${styles.cellMono}`}
                    style={{
                      color:
                        row.open_defects_count > 0
                          ? 'var(--color-critical)'
                          : 'var(--color-text-muted)',
                    }}
                  >
                    {row.open_defects_count}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </section>

      <section className={styles.section}>
        <header className={styles.sectionHeader}>
          <h2 className={styles.sectionTitle}>Top aircraft by hours</h2>
          <span className={styles.sectionMeta}>top {data.top_aircraft_by_hours.length}</span>
        </header>
        {data.top_aircraft_by_hours.length === 0 ? (
          <EmptyState message="No flight activity yet." />
        ) : (
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
              {data.top_aircraft_by_hours.map((row) => (
                <tr
                  key={row.aircraft_id}
                  className={`${styles.widgetTableRow} ${styles.widgetTableRowDisabled}`}
                  title="Detail view not yet available"
                >
                  <td className={styles.cellId}>
                    {row.equipment_id ?? row.serial_number}
                  </td>
                  <td>
                    {row.model_name ?? <span className={styles.cellMuted}>—</span>}
                  </td>
                  <td className={`${styles.cellRight} ${styles.cellMono}`}>
                    {formatHours(row.total_hours)}
                  </td>
                  <td className={`${styles.cellRight} ${styles.cellMono}`}>
                    {row.total_flights}
                  </td>
                  <td className={`${styles.cellRight} ${styles.cellMuted}`}>
                    {formatDaysAgo(row.last_flight)}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </section>
    </>
  );
}
