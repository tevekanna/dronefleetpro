// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useQuery } from '@tanstack/react-query';
import { fetchFlightStats } from '../../api/flights';
import styles from './StatsBar.module.css';

function formatDate(dateStr: string | null): string {
  if (!dateStr) return '—';
  return dateStr.substring(0, 10);
}

export function StatsBar() {
  const { data: stats } = useQuery({
    queryKey: ['flight-stats'],
    queryFn: fetchFlightStats,
  });

  const distance = stats
    ? stats.total_distance_km < 1
      ? `${(stats.total_distance_km * 1000).toFixed(0)} m`
      : `${stats.total_distance_km.toFixed(1)} km`
    : '—';

  return (
    <div className={styles.statsBar}>
      <div className={styles.statCard}>
        <div className={styles.statValue}>{stats?.total_flights ?? '—'}</div>
        <div className={styles.statLabel}>Total Flights</div>
      </div>
      <div className={styles.statCard}>
        <div className={styles.statValue}>
          {stats ? `${stats.total_hours.toFixed(1)}h` : '—'}
        </div>
        <div className={styles.statLabel}>Total Hours</div>
      </div>
      <div className={styles.statCard}>
        <div className={styles.statValue}>
          {stats ? `${stats.max_altitude_m.toFixed(0)} m` : '—'}
        </div>
        <div className={styles.statLabel}>Max Altitude</div>
      </div>
      <div className={styles.statCard}>
        <div className={styles.statValue}>
          {stats ? formatDate(stats.last_flight) : '—'}
        </div>
        <div className={styles.statLabel}>Last Flight</div>
      </div>
      <div className={styles.statCard}>
        <div className={styles.statValue}>{distance}</div>
        <div className={styles.statLabel}>Total Distance</div>
      </div>
    </div>
  );
}
