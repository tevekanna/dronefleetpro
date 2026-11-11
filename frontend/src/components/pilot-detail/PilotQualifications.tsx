// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import type { PilotDetailQualification } from '../../api/pilotDetail';
import styles from '../../pages/PilotDetailPage.module.css';

const STATUS_CLASS: Record<PilotDetailQualification['status'], string> = {
  ok: styles.qualOk,
  expiring_soon: styles.qualExpiring,
  expired: styles.qualExpired,
  no_expiry: styles.qualNoExpiry,
};

const STATUS_LABEL: Record<PilotDetailQualification['status'], string> = {
  ok: 'Valid',
  expiring_soon: 'Expiring',
  expired: 'Expired',
  no_expiry: 'No expiry',
};

function formatStatusDetail(q: PilotDetailQualification): string {
  if (q.status === 'no_expiry') return 'No expiry tracked';
  if (q.expiry_date) {
    const d = q.expiry_date.substring(0, 10);
    if (q.status === 'expired' && q.days_until_expiry != null) {
      return `${d} · expired ${Math.abs(q.days_until_expiry)} days ago`;
    }
    if (q.status === 'expiring_soon' && q.days_until_expiry != null) {
      return `${d} · ${q.days_until_expiry} days left`;
    }
    return d;
  }
  return '—';
}

export function PilotQualifications({
  qualifications,
}: {
  qualifications: PilotDetailQualification[];
}) {
  return (
    <div className={styles.section}>
      <div className={styles.sectionHeading}>Qualifications</div>
      {qualifications.length === 0 ? (
        <div className={styles.muted} style={{ fontSize: '0.85rem' }}>
          No license or qualifications on file.
        </div>
      ) : (
        <div className={styles.qualList}>
          {qualifications.map(q => (
            <div key={`${q.id ?? 'license'}-${q.type_name}`} className={styles.qualRow}>
              <div className={styles.qualLeft}>
                <span className={styles.qualName}>
                  {q.type_name}
                  {q.is_primary_license && (
                    <span style={{ color: 'var(--color-text-muted)', fontSize: '0.75rem', marginLeft: 'var(--space-2)' }}>
                      Primary license
                    </span>
                  )}
                </span>
                <span className={styles.qualMeta}>{formatStatusDetail(q)}</span>
              </div>
              <span className={`${styles.qualStatus} ${STATUS_CLASS[q.status]}`}>
                {STATUS_LABEL[q.status]}
              </span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
