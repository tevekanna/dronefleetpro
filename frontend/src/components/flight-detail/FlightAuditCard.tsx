// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import type { Flight } from '../../types/flight';
import type { FlightAudit } from '../../api/flightDetail';
import styles from '../../pages/FlightDetailPage.module.css';
import { formatDateTime, sourceLabel } from './flightFormatters';

interface FlightAuditCardProps {
  flight: Flight;
  audit: FlightAudit | null;
}

export function FlightAuditCard({ flight, audit }: FlightAuditCardProps) {
  return (
    <div className={styles.section}>
      <div className={styles.sectionHeading}>Source &amp; metadata</div>
      <div className={styles.detailRowList}>
        <div className={styles.detailRow}>
          <span className={styles.detailLabel}>Log source</span>
          <span className={styles.detailValue}>{sourceLabel(flight.log_source)}</span>
        </div>
        <div className={styles.detailRow}>
          <span className={styles.detailLabel}>Source filename</span>
          <span className={styles.detailValue}>
            {flight.source_filename ?? <span className={styles.detailMuted}>—</span>}
          </span>
        </div>
        {flight.flight_guid && (
          <div className={styles.detailRow}>
            <span className={styles.detailLabel}>Flight GUID</span>
            <span className={styles.detailValue}>{flight.flight_guid}</span>
          </div>
        )}
        <div className={styles.detailRow}>
          <span className={styles.detailLabel}>Imported by</span>
          <span className={styles.detailValue}>
            {audit?.imported_by_user_code ?? (
              <span className={styles.detailMuted}>—</span>
            )}
          </span>
        </div>
        <div className={styles.detailRow}>
          <span className={styles.detailLabel}>Imported at</span>
          <span className={styles.detailValue}>
            {formatDateTime(audit?.imported_at ?? flight.created_at)}
          </span>
        </div>
        {flight.deleted_at && (
          <div className={styles.detailRow}>
            <span className={styles.detailLabel}>Soft-deleted at</span>
            <span className={`${styles.detailValue} ${styles.preflightMissing}`}>
              {formatDateTime(flight.deleted_at)}
            </span>
          </div>
        )}
      </div>
    </div>
  );
}
