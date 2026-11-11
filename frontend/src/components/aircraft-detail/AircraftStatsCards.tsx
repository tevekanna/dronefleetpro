// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import type { AircraftDetailStats } from '../../api/aircraftDetail';
import styles from '../../pages/AircraftDetailPage.module.css';

function formatDuration(seconds: number): string {
  if (seconds <= 0) return '0h 0m';
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  return `${h}h ${m}m`;
}

function formatLastFlight(iso: string | null): string {
  if (!iso) return 'Never';
  return iso.substring(0, 10);
}

function formatKm(metres: number): string {
  if (metres <= 0) return '0 km';
  const km = metres / 1000;
  if (km < 10) return `${km.toFixed(1)} km`;
  return `${Math.round(km)} km`;
}

export function AircraftStatsCards({ stats }: { stats: AircraftDetailStats }) {
  const cards = [
    {
      label: 'Total flight time',
      value: formatDuration(stats.total_seconds),
      secondary: `${stats.total_flights} flight${stats.total_flights === 1 ? '' : 's'}`,
    },
    {
      label: 'Last flight',
      value: formatLastFlight(stats.last_flight),
      secondary: stats.last_flight
        ? new Date(stats.last_flight).toISOString().substring(11, 16) + ' UTC'
        : '—',
    },
    {
      label: 'Last 30 days',
      value: formatDuration(stats.seconds_last_30d),
      secondary: `${stats.flights_last_30d} flight${stats.flights_last_30d === 1 ? '' : 's'}`,
    },
    {
      label: 'Total distance',
      value: formatKm(stats.total_distance_m),
      secondary: 'Lifetime',
    },
  ];

  return (
    <div className={styles.statsGrid}>
      {cards.map(c => (
        <div key={c.label} className={styles.statCard}>
          <div className={styles.statValue}>{c.value}</div>
          <div className={styles.statLabel}>{c.label}</div>
          <div className={styles.statSecondary}>{c.secondary}</div>
        </div>
      ))}
    </div>
  );
}
