// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import type { PilotDetailHeader } from '../../api/pilotDetail';
import styles from '../../pages/PilotDetailPage.module.css';

function formatDate(d: string | null): string {
  if (!d) return '—';
  return d.substring(0, 10);
}

function expiryColor(expiry: string | null): string {
  if (!expiry) return 'var(--color-text-muted)';
  const days = Math.floor(
    (new Date(expiry).getTime() - new Date().setHours(0, 0, 0, 0)) /
      (1000 * 60 * 60 * 24)
  );
  if (days < 0) return 'var(--color-critical)';
  if (days <= 60) return 'var(--color-warning)';
  return 'var(--color-ok)';
}

export function PilotHeaderCard({ pilot }: { pilot: PilotDetailHeader }) {
  return (
    <div className={`${styles.section} ${styles.headerCard}`}>
      <div className={styles.headerTopRow}>
        <h1 className={styles.pilotName}>{pilot.name}</h1>
        <span
          className={`${styles.statusBadge} ${
            pilot.is_active ? styles.statusActive : styles.statusInactive
          }`}
        >
          {pilot.is_active ? 'Active' : 'Inactive'}
        </span>
      </div>
      <div className={styles.headerMeta}>
        {pilot.user_code && (
          <div className={styles.metaItem}>
            <span className={styles.metaLabel}>User code</span>
            <span className={styles.metaValue}>{pilot.user_code}</span>
          </div>
        )}
        {pilot.email && (
          <div className={styles.metaItem}>
            <span className={styles.metaLabel}>Email</span>
            <span className={styles.metaValue}>{pilot.email}</span>
          </div>
        )}
        {pilot.phone && (
          <div className={styles.metaItem}>
            <span className={styles.metaLabel}>Phone</span>
            <span className={styles.metaValue}>{pilot.phone}</span>
          </div>
        )}
        <div className={styles.metaItem}>
          <span className={styles.metaLabel}>License</span>
          <span className={styles.metaValue}>{pilot.license_type ?? '—'}</span>
        </div>
        {pilot.license_number && (
          <div className={styles.metaItem}>
            <span className={styles.metaLabel}>License #</span>
            <span className={styles.metaValue}>{pilot.license_number}</span>
          </div>
        )}
        <div className={styles.metaItem}>
          <span className={styles.metaLabel}>Expires</span>
          <span
            className={styles.metaValue}
            style={{ color: expiryColor(pilot.license_expires) }}
          >
            {formatDate(pilot.license_expires)}
          </span>
        </div>
      </div>
      {pilot.notes && (
        <div className={styles.metaItem}>
          <span className={styles.metaLabel}>Notes</span>
          <span style={{ fontSize: '0.85rem', color: 'var(--color-text-secondary)' }}>
            {pilot.notes}
          </span>
        </div>
      )}
    </div>
  );
}
