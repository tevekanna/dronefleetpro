// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import type { PilotDetailStats } from '../../api/pilotDetail';
import styles from '../../pages/PilotDetailPage.module.css';

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

export function PilotStatsCards({ stats }: { stats: PilotDetailStats }) {
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
      label: 'Last 90 days',
      value: formatDuration(stats.last_90_days_seconds),
      secondary: `${stats.last_90_days_flights} flight${stats.last_90_days_flights === 1 ? '' : 's'} · EASA recency`,
    },
    {
      label: 'Last 12 months',
      value: formatDuration(stats.last_12_months_seconds),
      secondary: `${stats.last_12_months_flights} flight${stats.last_12_months_flights === 1 ? '' : 's'}`,
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
