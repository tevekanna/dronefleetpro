// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useEffect, useState } from 'react';
import { X } from 'lucide-react';
import { useAuditLogFilters } from '../../hooks/useAuditLog';
import type { AuditAction, AuditLogListParams } from '../../types/auditLog';
import { IsoDateInput } from '../inputs/IsoDateInput';
import styles from '../flights/FlightFilterBar.module.css';

interface Props {
  filters: AuditLogListParams;
  /**
   * Emits a partial update — only the changed filters. The page owns the state
   * and performs the merge, which avoids stale-closure races when two
   * debounced inputs (From + To) fire independently.
   */
  onChange: (patch: Partial<AuditLogListParams>) => void;
  onReset: () => void;
}

// Fallback when the /filters endpoint hasn't responded yet — keeps the
// dropdown populated on first paint. The live list comes from the API.
const FALLBACK_ACTIONS: AuditAction[] = [
  'INSERT',
  'UPDATE',
  'DELETE',
  'EXPORT',
  'IMPORT',
  'PURGE',
];

function countActive(f: AuditLogListParams): number {
  return (
    (f.from ? 1 : 0) +
    (f.to ? 1 : 0) +
    (f.user_id !== undefined ? 1 : 0) +
    (f.table_name ? 1 : 0) +
    (f.action ? 1 : 0) +
    (f.search ? 1 : 0)
  );
}

export function AuditFilterBar({ filters, onChange, onReset }: Props) {
  const { data: options } = useAuditLogFilters();

  // Debounced date inputs (matches FlightFilterBar's 400ms pattern)
  const [fromLocal, setFromLocal] = useState(filters.from ?? '');
  const [toLocal, setToLocal] = useState(filters.to ?? '');
  const [searchLocal, setSearchLocal] = useState(filters.search ?? '');
  useEffect(() => setFromLocal(filters.from ?? ''), [filters.from]);
  useEffect(() => setToLocal(filters.to ?? ''), [filters.to]);
  useEffect(() => setSearchLocal(filters.search ?? ''), [filters.search]);

  useEffect(() => {
    const t = setTimeout(() => {
      const next = fromLocal || undefined;
      if (next !== filters.from) onChange({ from: next });
    }, 400);
    return () => clearTimeout(t);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [fromLocal]);

  useEffect(() => {
    const t = setTimeout(() => {
      const next = toLocal || undefined;
      if (next !== filters.to) onChange({ to: next });
    }, 400);
    return () => clearTimeout(t);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [toLocal]);

  useEffect(() => {
    const t = setTimeout(() => {
      const next = searchLocal.trim() || undefined;
      if (next !== filters.search) onChange({ search: next });
    }, 300);
    return () => clearTimeout(t);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [searchLocal]);

  const actionOptions: AuditAction[] =
    options?.actions && options.actions.length > 0 ? options.actions : FALLBACK_ACTIONS;
  const activeCount = countActive(filters);

  return (
    <div className={styles.filterBar}>
      <div className={styles.filterHeader}>
        <span className={styles.filterLabel}>Filters</span>
        {activeCount > 0 && <span className={styles.activeBadge}>{activeCount} active</span>}
      </div>
      <div className={styles.controls}>
        <div className={styles.field}>
          <span className={styles.fieldLabel}>From</span>
          <IsoDateInput
            className={styles.input}
            value={fromLocal}
            onChange={(e) => setFromLocal(e.target.value)}
          />
        </div>
        <div className={styles.field}>
          <span className={styles.fieldLabel}>To</span>
          <IsoDateInput
            className={styles.input}
            value={toLocal}
            onChange={(e) => setToLocal(e.target.value)}
          />
        </div>
        <div className={styles.field}>
          <span className={styles.fieldLabel}>User</span>
          <select
            className={styles.select}
            value={filters.user_id ?? ''}
            onChange={(e) =>
              onChange({ user_id: e.target.value ? Number(e.target.value) : undefined })
            }
          >
            <option value="">All users</option>
            {(options?.users ?? []).map((u) => (
              <option key={u.id} value={u.id}>
                {u.name} ({u.user_code})
              </option>
            ))}
          </select>
        </div>
        <div className={styles.field}>
          <span className={styles.fieldLabel}>Table</span>
          <select
            className={styles.select}
            value={filters.table_name ?? ''}
            onChange={(e) => onChange({ table_name: e.target.value || undefined })}
          >
            <option value="">All tables</option>
            {(options?.tables ?? []).map((t) => (
              <option key={t} value={t}>
                {t}
              </option>
            ))}
          </select>
        </div>
        <div className={styles.field}>
          <span className={styles.fieldLabel}>Action</span>
          <select
            className={styles.select}
            value={filters.action ?? ''}
            onChange={(e) =>
              onChange({
                action: e.target.value ? (e.target.value as AuditAction) : undefined,
              })
            }
          >
            <option value="">All</option>
            {actionOptions.map((a) => (
              <option key={a} value={a}>
                {a}
              </option>
            ))}
          </select>
        </div>
        <div className={`${styles.field} ${styles.searchField}`}>
          <span className={styles.fieldLabel}>Search in diff</span>
          <input
            type="search"
            className={`${styles.input} ${styles.searchInput}`}
            placeholder="Serial, code, value…"
            value={searchLocal}
            onChange={(e) => setSearchLocal(e.target.value)}
          />
        </div>
        <button
          type="button"
          className={styles.resetBtn}
          onClick={onReset}
          disabled={activeCount === 0}
        >
          <X size={14} /> Clear
        </button>
      </div>
    </div>
  );
}
