// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import type { MaintenanceAuditEntry } from '../../api/maintenance';
import styles from '../../pages/MaintenanceDetailPage.module.css';

interface Props {
  audit: MaintenanceAuditEntry[];
}

function formatTimestamp(iso: string): string {
  // Render in monospaced UTC: YYYY-MM-DD HH:MM:SS UTC
  if (!iso) return '—';
  const date = iso.substring(0, 10);
  const time = iso.substring(11, 19);
  return `${date} ${time} UTC`;
}

function ActionBadge({ action }: { action: string }) {
  let cls = styles.auditActionBadge;
  if (action === 'INSERT') cls = `${styles.auditActionBadge} ${styles.auditActionInsert}`;
  else if (action === 'UPDATE') cls = `${styles.auditActionBadge} ${styles.auditActionUpdate}`;
  else if (action === 'DELETE') cls = `${styles.auditActionBadge} ${styles.auditActionDelete}`;
  return <span className={cls}>{action}</span>;
}

function userLabel(entry: MaintenanceAuditEntry): string {
  if (entry.user_name && entry.user_code) return `${entry.user_name} (${entry.user_code})`;
  if (entry.user_name) return entry.user_name;
  if (entry.user_code) return entry.user_code;
  return 'system';
}

export function MaintenanceAuditCard({ audit }: Props) {
  return (
    <details className={styles.auditDetails}>
      <summary className={styles.auditSummary}>Audit trail ({audit.length})</summary>
      <div className={styles.auditBody}>
        {audit.length === 0 ? (
          <span className={styles.detailMuted}>No audit history yet.</span>
        ) : (
          audit.map((entry) => (
            <div key={entry.id} className={styles.auditEntry}>
              <div className={styles.auditEntryHeader}>
                <span className={styles.auditTimestamp}>{formatTimestamp(entry.created_at)}</span>
                <ActionBadge action={entry.action} />
                <span className={styles.auditUser}>{userLabel(entry)}</span>
              </div>
              {entry.diff_summary && (
                <span className={styles.auditDiffSummary}>{entry.diff_summary}</span>
              )}
              {entry.changed_fields.length > 0 && (
                <div className={styles.auditChips}>
                  {entry.changed_fields.map((field) => (
                    <span key={field} className={styles.auditChip}>
                      {field}
                    </span>
                  ))}
                </div>
              )}
            </div>
          ))
        )}
      </div>
    </details>
  );
}
