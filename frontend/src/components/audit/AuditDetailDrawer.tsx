// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useEffect, useState } from 'react';
import { Copy, X } from 'lucide-react';
import { useAuditLogEntry } from '../../hooks/useAuditLog';
import type {
  AuditChangedField,
  AuditLogDetail,
} from '../../types/auditLog';
import { AuditActionBadge } from './AuditActionBadge';
import { showToast } from '../../store/toastStore';
import styles from './AuditDetailDrawer.module.css';

interface Props {
  isOpen: boolean;
  entryId: number | null;
  onClose: () => void;
}

const MAX_INLINE_JSON = 80;

function renderValue(val: unknown): React.ReactNode {
  if (val === null || val === undefined) {
    return <span className={styles.fieldValueMuted}>—</span>;
  }
  if (typeof val === 'string') {
    return <span className={styles.fieldValueStr}>"{val}"</span>;
  }
  if (typeof val === 'number' || typeof val === 'boolean') {
    return <span>{String(val)}</span>;
  }
  // Object / array: compact JSON one-liner, truncated
  const s = JSON.stringify(val);
  const truncated = s.length > MAX_INLINE_JSON ? `${s.slice(0, MAX_INLINE_JSON)}…` : s;
  return (
    <span className={styles.fieldValue} title={s}>
      {truncated}
    </span>
  );
}

function toChangedFields(obj: Record<string, unknown>): AuditChangedField[] {
  return Object.entries(obj).map(([field, value]) => ({
    field,
    before: null,
    after: value,
  }));
}

function extractFullRecordFields(detail: AuditLogDetail): AuditChangedField[] | null {
  if (!detail.diff) return null;
  if (detail.action === 'INSERT') {
    const after = detail.diff.after as unknown;
    if (after && typeof after === 'object' && !Array.isArray(after)) {
      return toChangedFields(after as Record<string, unknown>);
    }
  }
  if (detail.action === 'DELETE') {
    const before = detail.diff.before as unknown;
    if (before && typeof before === 'object' && !Array.isArray(before)) {
      return Object.entries(before as Record<string, unknown>).map(([field, value]) => ({
        field,
        before: value,
        after: null,
      }));
    }
  }
  return null;
}

function ChangedFieldsSection({ detail }: { detail: AuditLogDetail }) {
  const fullRecord = extractFullRecordFields(detail);
  const rows =
    detail.action === 'UPDATE' ? detail.changed_fields ?? [] : fullRecord ?? [];
  const title =
    detail.action === 'UPDATE' ? 'Changed fields' : 'Full record';

  return (
    <section>
      <h3 className={styles.sectionTitle}>{title}</h3>
      {rows.length === 0 ? (
        <div className={styles.noChanges}>No field-level changes recorded.</div>
      ) : (
        <div className={styles.fieldList}>
          {rows.map((f) => (
            <div key={f.field} className={styles.fieldRow}>
              <div className={styles.fieldName}>{f.field}</div>
              <div className={styles.fieldValue}>{renderValue(f.before)}</div>
              <div className={styles.arrow}>→</div>
              <div className={styles.fieldValue}>{renderValue(f.after)}</div>
            </div>
          ))}
        </div>
      )}
    </section>
  );
}

function RawJsonSection({ detail }: { detail: AuditLogDetail }) {
  const [show, setShow] = useState(false);
  const payload = detail.diff ?? {};
  const jsonText = JSON.stringify(payload, null, 2);

  const copyIt = async () => {
    try {
      await navigator.clipboard.writeText(jsonText);
      showToast('Raw JSON copied to clipboard', 'success');
    } catch {
      showToast('Failed to copy to clipboard', 'error');
    }
  };

  return (
    <>
      <button
        type="button"
        className={styles.toggleRawBtn}
        onClick={() => setShow((s) => !s)}
      >
        {show ? 'Hide raw JSON' : 'Show raw JSON'}
      </button>
      {show && (
        <div className={styles.rawContainer}>
          <button type="button" className={styles.copyBtn} onClick={copyIt}>
            <Copy size={12} /> Copy
          </button>
          <pre className={styles.rawJson}>{jsonText}</pre>
        </div>
      )}
    </>
  );
}

function Meta({ detail }: { detail: AuditLogDetail }) {
  const absolute = new Date(detail.created_at).toISOString().replace('T', ' ').replace('Z', ' UTC');
  return (
    <div className={styles.meta}>
      <span className={styles.metaLabel}>Time</span>
      <span className={`${styles.metaValue} ${styles.metaValueMono}`}>{absolute}</span>
      <span className={styles.metaLabel}>User</span>
      <span className={styles.metaValue}>
        {detail.user_name ? `${detail.user_name} (${detail.user_code ?? '---'})` : 'System'}
      </span>
      <span className={styles.metaLabel}>IP</span>
      <span className={`${styles.metaValue} ${styles.metaValueMono}`}>
        {detail.ip_address ?? '—'}
      </span>
      <span className={styles.metaLabel}>Audit id</span>
      <span className={`${styles.metaValue} ${styles.metaValueMono}`}>#{detail.id}</span>
    </div>
  );
}

export function AuditDetailDrawer({ isOpen, entryId, onClose }: Props) {
  const { data, isLoading, error } = useAuditLogEntry(isOpen ? entryId : null);

  useEffect(() => {
    if (!isOpen) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, [isOpen, onClose]);

  if (!isOpen || entryId === null) return null;

  return (
    <div className={styles.overlay} onClick={onClose}>
      <aside
        className={styles.drawer}
        onClick={(e) => e.stopPropagation()}
        role="dialog"
        aria-label="Audit log entry detail"
      >
        <header className={styles.header}>
          {data ? (
            <>
              <AuditActionBadge action={data.action} />
              <h2 className={styles.title}>
                <span className={styles.titleTable}>{data.table_name}</span>
                {data.record_id !== null && (
                  <span className={styles.titleRecord}>#{data.record_id}</span>
                )}
              </h2>
            </>
          ) : (
            <h2 className={styles.title}>Audit entry #{entryId}</h2>
          )}
          <button
            type="button"
            className={styles.closeBtn}
            onClick={onClose}
            aria-label="Close"
          >
            <X size={18} />
          </button>
        </header>

        {isLoading && <div className={styles.loading}>Loading…</div>}
        {error && (
          <div className={styles.error}>
            Failed to load entry: {(error as Error).message}
          </div>
        )}
        {data && (
          <>
            <Meta detail={data} />
            <div className={styles.body}>
              <ChangedFieldsSection detail={data} />
              <RawJsonSection detail={data} />
            </div>
          </>
        )}
      </aside>
    </div>
  );
}
