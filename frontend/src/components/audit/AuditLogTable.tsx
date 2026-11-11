// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { formatDistanceToNow, parseISO } from 'date-fns';
import type { AuditLogListItem } from '../../types/auditLog';
import { AuditActionBadge } from './AuditActionBadge';
import styles from './AuditLogTable.module.css';

interface Props {
  items: AuditLogListItem[];
  selectedId: number | null;
  onRowClick: (id: number) => void;
  isLoading: boolean;
}

function formatAbsolute(iso: string): string {
  // Display: 2026-04-20 12:34:56 UTC
  const d = parseISO(iso);
  const pad = (n: number) => n.toString().padStart(2, '0');
  return (
    `${d.getUTCFullYear()}-${pad(d.getUTCMonth() + 1)}-${pad(d.getUTCDate())} ` +
    `${pad(d.getUTCHours())}:${pad(d.getUTCMinutes())}:${pad(d.getUTCSeconds())} UTC`
  );
}

function formatRelative(iso: string): string {
  try {
    return formatDistanceToNow(parseISO(iso), { addSuffix: true });
  } catch {
    return '';
  }
}

function renderChangedCell(item: AuditLogListItem) {
  if (item.action === 'INSERT') return <span className={styles.muted}>new record</span>;
  if (item.action === 'DELETE') return <span className={styles.muted}>removed</span>;
  // UPDATE
  if (!item.changed_fields || item.changed_fields.length === 0) {
    return <span className={styles.muted}>—</span>;
  }
  const first = item.changed_fields.slice(0, 2);
  const extra = item.changed_fields.length - first.length;
  return (
    <span>
      {first.join(', ')}
      {extra > 0 && <span className={styles.changedMore}>+{extra} more</span>}
    </span>
  );
}

function renderUserCell(item: AuditLogListItem) {
  if (item.user_id === null || !item.user_name) {
    return <span className={styles.muted}>System</span>;
  }
  return (
    <span>
      {item.user_name} <span className={styles.muted}>({item.user_code ?? '---'})</span>
    </span>
  );
}

export function AuditLogTable({ items, selectedId, onRowClick, isLoading }: Props) {
  if (!isLoading && items.length === 0) {
    return <div className={styles.empty}>No audit entries match the current filters.</div>;
  }

  return (
    <div className={styles.tableWrapper}>
      <table className={styles.table}>
        <thead>
          <tr>
            <th className={styles.th}>Time</th>
            <th className={styles.th}>Action</th>
            <th className={styles.th}>Table</th>
            <th className={styles.th}>Record</th>
            <th className={styles.th}>User</th>
            <th className={styles.th}>Changed</th>
            <th className={styles.th}>IP</th>
          </tr>
        </thead>
        <tbody>
          {isLoading && items.length === 0
            ? Array.from({ length: 6 }).map((_, i) => (
                <tr key={`sk-${i}`}>
                  <td colSpan={7} className={styles.skeletonRow} />
                </tr>
              ))
            : items.map((item) => (
                <tr
                  key={item.id}
                  className={styles.tr}
                  data-active={item.id === selectedId}
                  onClick={() => onRowClick(item.id)}
                >
                  <td className={`${styles.td} ${styles.timeCell}`}>
                    <div className={styles.timeAbsolute}>{formatAbsolute(item.created_at)}</div>
                    <div className={styles.timeRelative}>{formatRelative(item.created_at)}</div>
                  </td>
                  <td className={styles.td}>
                    <AuditActionBadge action={item.action} />
                  </td>
                  <td className={`${styles.td} ${styles.mono}`}>{item.table_name}</td>
                  <td className={`${styles.td} ${styles.mono}`}>
                    {item.record_id !== null ? (
                      `#${item.record_id}`
                    ) : (
                      <span className={styles.muted}>—</span>
                    )}
                  </td>
                  <td className={styles.td}>{renderUserCell(item)}</td>
                  <td className={styles.td}>{renderChangedCell(item)}</td>
                  <td className={`${styles.td} ${styles.ipCell}`}>
                    {item.ip_address ?? '—'}
                  </td>
                </tr>
              ))}
        </tbody>
      </table>
    </div>
  );
}
