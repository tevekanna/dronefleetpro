// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { Link } from 'react-router-dom';
import type { DashboardStats } from '../../api/dashboard';
import styles from './dashboardLayout.module.css';

export interface FleetSummaryStripProps {
  stats: DashboardStats;
}

/** Renders an integer value, or "—" when the value is not finite. */
function formatInt(value: number): string {
  if (!Number.isFinite(value)) return '—';
  return Math.round(value).toString();
}

/** Formats lifetime seconds as fixed-1-decimal hours, or "—" when not finite. */
function formatHours(totalSeconds: number): string {
  if (!Number.isFinite(totalSeconds)) return '—';
  return (totalSeconds / 3600).toFixed(1);
}

/**
 * Compact, sparkline-free summary strip at the top of the Dashboard.
 * Six clickable tiles surfacing lifetime fleet totals. The airworthiness
 * split (airworthy / not airworthy) complements the shared "Aircraft in
 * service" KPI card, which carries the in-service (status='active') count.
 */
export function FleetSummaryStrip({ stats }: FleetSummaryStripProps) {
  const notAirworthyCritical = stats.not_airworthy_aircraft > 0;

  return (
    <div className={styles.summaryStrip}>
      <Link to="/flights" className={styles.summaryTile}>
        <span className={styles.summaryLabel}>Total flights</span>
        <span className={styles.summaryValue}>{formatInt(stats.total_flights)}</span>
      </Link>

      <Link to="/flights" className={styles.summaryTile}>
        <span className={styles.summaryLabel}>Total hours</span>
        <span className={styles.summaryValue}>
          {formatHours(stats.total_seconds)}
          <span className={styles.summaryUnit}>h</span>
        </span>
      </Link>

      <Link to="/fleet?airworthiness=airworthy" className={styles.summaryTile}>
        <span className={styles.summaryLabel}>Airworthy</span>
        <span className={styles.summaryValue}>{formatInt(stats.airworthy_aircraft)}</span>
      </Link>

      <Link
        to="/fleet?airworthiness=not_airworthy"
        className={`${styles.summaryTile}${notAirworthyCritical ? ` ${styles.summaryTileCritical}` : ''}`}
      >
        <span className={styles.summaryLabel}>Not airworthy</span>
        <span className={styles.summaryValue}>{formatInt(stats.not_airworthy_aircraft)}</span>
      </Link>

      <Link to="/pilots" className={styles.summaryTile}>
        <span className={styles.summaryLabel}>Pilots</span>
        <span className={styles.summaryValue}>{formatInt(stats.total_pilots)}</span>
      </Link>

      <Link to="/aircraft-models" className={styles.summaryTile}>
        <span className={styles.summaryLabel}>Aircraft models</span>
        <span className={styles.summaryValue}>{formatInt(stats.total_aircraft_models)}</span>
      </Link>
    </div>
  );
}
