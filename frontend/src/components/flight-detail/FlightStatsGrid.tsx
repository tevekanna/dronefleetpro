// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import type { Flight } from '../../types/flight';
import styles from '../../pages/FlightDetailPage.module.css';
import { formatDuration, formatNumber } from './flightFormatters';

interface StatCard {
  label: string;
  value: string;
}

export function FlightStatsGrid({ flight }: { flight: Flight }) {
  const cards: StatCard[] = [
    { label: 'Max altitude', value: formatNumber(flight.max_altitude_m, 0, 'm') },
    { label: 'Max distance', value: formatNumber(flight.max_distance_m, 0, 'm') },
    {
      label: 'Total distance',
      value:
        flight.total_distance_m > 0
          ? formatNumber(flight.total_distance_m, 0, 'm')
          : '—',
    },
    { label: 'Max speed', value: formatNumber(flight.max_speed_ms, 1, 'm/s') },
    {
      label: 'Min battery',
      value: flight.min_battery_pct != null ? `${flight.min_battery_pct}%` : '—',
    },
    { label: 'Battery temp', value: formatNumber(flight.battery_temp_c, 1, '°C') },
    {
      label: 'Cell variance',
      value:
        flight.cell_variance_v != null
          ? `${(flight.cell_variance_v * 1000).toFixed(0)} mV`
          : '—',
    },
    {
      label: 'Min satellites',
      value: flight.satellite_count_min != null ? String(flight.satellite_count_min) : '—',
    },
    {
      label: 'Photos',
      value: flight.photo_count > 0 ? String(flight.photo_count) : 'None',
    },
    {
      label: 'Video',
      value: flight.video_seconds > 0 ? formatDuration(flight.video_seconds) : 'None',
    },
  ];

  return (
    <div className={styles.section}>
      <div className={styles.sectionHeading}>Telemetry</div>
      <div className={styles.statsGrid}>
        {cards.map(c => (
          <div key={c.label} className={styles.statCard}>
            <div className={styles.statValue}>{c.value}</div>
            <div className={styles.statLabel}>{c.label}</div>
          </div>
        ))}
      </div>
    </div>
  );
}
