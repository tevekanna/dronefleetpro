// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import type { Flight } from '../../types/flight';
import styles from '../../pages/FlightDetailPage.module.css';
import { formatCoords } from './flightFormatters';

export function FlightEasaCard({ flight }: { flight: Flight }) {
  return (
    <div className={styles.section}>
      <div className={styles.sectionHeading}>EASA &amp; operation</div>
      <div className={styles.detailRowList}>
        <div className={styles.detailRow}>
          <span className={styles.detailLabel}>EASA category</span>
          <span className={styles.detailValue}>
            {flight.easa_category ?? <span className={styles.detailMuted}>—</span>}
          </span>
        </div>
        <div className={styles.detailRow}>
          <span className={styles.detailLabel}>Mission type</span>
          <span className={styles.detailValue}>
            {flight.mission_type ?? <span className={styles.detailMuted}>—</span>}
          </span>
        </div>
        <div className={styles.detailRow}>
          <span className={styles.detailLabel}>Operation</span>
          <span className={styles.detailValue}>
            {flight.operation_name ?? <span className={styles.detailMuted}>—</span>}
          </span>
        </div>
        {flight.operation_description && (
          <div className={styles.detailRow}>
            <span className={styles.detailLabel}>Description</span>
            <span className={styles.detailValue}>{flight.operation_description}</span>
          </div>
        )}
        <div className={styles.detailRow}>
          <span className={styles.detailLabel}>Preflight check</span>
          <span
            className={`${styles.detailValue} ${
              flight.preflight_done ? styles.preflightOk : styles.preflightMissing
            }`}
          >
            {flight.preflight_done ? '✓ Done' : '✗ Not recorded'}
          </span>
        </div>
        <div className={styles.detailRow}>
          <span className={styles.detailLabel}>Home position</span>
          <span className={styles.detailValue}>
            {formatCoords(flight.home_lat, flight.home_lon)}
          </span>
        </div>
      </div>
    </div>
  );
}
