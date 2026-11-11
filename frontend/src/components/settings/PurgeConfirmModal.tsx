// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useState, useEffect } from 'react';
import { AlertTriangle, CheckCircle2, ShieldAlert } from 'lucide-react';
import type { PurgeConfirmation, RetentionImpact } from '../../api/purge';
import styles from './PurgeConfirmModal.module.css';

interface PurgeConfirmModalProps {
  isOpen: boolean;
  entityLabel: string;
  impact: RetentionImpact | null;
  isLoadingImpact: boolean;
  errorMessage: string | null;
  onConfirm: (confirmation?: PurgeConfirmation) => void;
  onCancel: () => void;
  isSubmitting: boolean;
  deletedCount?: number;
}

/** Human reading of RetentionDetail.effect. The backend may append an
 *  explanatory suffix (pilot qualifications do), so match on the prefix. */
function effectLabel(effect: string): string {
  if (effect.startsWith('identity_stripped')) return 'Kept, identity references removed';
  if (effect.startsWith('deleted')) return 'Permanently deleted';
  return effect;
}

export function PurgeConfirmModal({
  isOpen,
  entityLabel,
  impact,
  isLoadingImpact,
  errorMessage,
  onConfirm,
  onCancel,
  isSubmitting,
  deletedCount,
}: PurgeConfirmModalProps) {
  // All hooks must stay above every early return — do not add hooks below
  // the `if (!isOpen)` line.
  const [confirmed, setConfirmed] = useState(false);
  const [acknowledgedInput, setAcknowledgedInput] = useState('');
  const [reason, setReason] = useState('');

  // Reset form state when the modal opens/closes
  useEffect(() => {
    if (isOpen) {
      setConfirmed(false);
      setAcknowledgedInput('');
      setReason('');
    }
  }, [isOpen]);

  if (!isOpen) return null;

  // An error must never be replaced by the success view.
  const isDone = deletedCount !== undefined && errorMessage === null;

  if (isDone) {
    return (
      <div className={styles.overlay} onClick={onCancel}>
        <div className={styles.modal} onClick={e => e.stopPropagation()}>
          <div className={styles.icon}>
            <CheckCircle2 size={48} className={styles.iconSuccess} />
          </div>
          <h2 className={styles.title}>Deleted successfully</h2>
          <p className={styles.body}>
            <span className={styles.resultCount}>{deletedCount}</span> records have been permanently deleted.
          </p>
          <div className={styles.actions}>
            <button className={styles.btnClose} onClick={onCancel}>Close</button>
          </div>
        </div>
      </div>
    );
  }

  const protectedTotal =
    impact !== null ? impact.within_retention + impact.degraded_records : null;
  const isProtected = protectedTotal !== null && protectedTotal > 0;

  const acknowledgedValue = Number(acknowledgedInput);
  const acknowledgedMatches =
    acknowledgedInput.trim() !== '' &&
    Number.isInteger(acknowledgedValue) &&
    protectedTotal !== null &&
    acknowledgedValue === protectedTotal;
  const reasonValid = reason.trim().length >= 10;

  const canConfirm =
    !isLoadingImpact &&
    impact !== null &&
    confirmed &&
    (!isProtected || (acknowledgedMatches && reasonValid));

  const handleConfirm = () => {
    if (isProtected) {
      onConfirm({ acknowledged: acknowledgedValue, reason: reason.trim() });
    } else {
      onConfirm();
    }
  };

  return (
    <div className={styles.overlay} onClick={onCancel}>
      <div className={styles.modal} onClick={e => e.stopPropagation()}>
        <div className={styles.icon}>
          <AlertTriangle size={48} className={styles.iconDanger} />
        </div>
        <h2 className={styles.title}>Delete all {entityLabel}?</h2>

        {errorMessage !== null && (
          <div className={styles.errorBanner}>{errorMessage}</div>
        )}

        {isLoadingImpact && (
          <p className={styles.loadingNote}>Checking retention impact...</p>
        )}

        {isProtected && impact !== null && (
          <div className={styles.retentionBlock}>
            <div className={styles.retentionHeading}>
              <ShieldAlert size={16} className={styles.retentionHeadingIcon} />
              <span>
                Records protected by the {impact.retention_years}-year EASA retention
                period (UAS.SPEC.050(1)(g))
              </span>
            </div>
            <div className={styles.retentionCounts}>
              <div className={styles.countRow}>
                <span className={styles.countValue}>{impact.within_retention}</span>
                <span className={styles.countLabel}>
                  record(s) within the retention period would be permanently deleted
                </span>
              </div>
              <div className={styles.countRow}>
                <span className={styles.countValue}>{impact.degraded_records}</span>
                <span className={styles.countLabel}>
                  record(s) would be kept but stripped of identifying data
                  (aircraft or pilot references)
                </span>
              </div>
            </div>
            <table className={styles.breakdownTable}>
              <tbody>
                {impact.details.map(d => (
                  <tr key={`${d.table}-${d.column}`}>
                    <td className={styles.breakdownTableCell}>{d.table}</td>
                    <td className={styles.breakdownCount}>{d.count}</td>
                    <td className={styles.breakdownEffect}>{effectLabel(d.effect)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
            <div className={styles.fieldGroup}>
              <label className={styles.fieldLabel} htmlFor="purge-acknowledged">
                Type the protected total ({protectedTotal}) to confirm
              </label>
              <input
                id="purge-acknowledged"
                type="number"
                className={styles.fieldInput}
                value={acknowledgedInput}
                onChange={e => setAcknowledgedInput(e.target.value)}
                placeholder={String(protectedTotal)}
              />
              {acknowledgedInput.trim() !== '' && !acknowledgedMatches && (
                <span className={styles.fieldError}>
                  Must match the protected total of {protectedTotal}
                </span>
              )}
            </div>
            <div className={styles.fieldGroup}>
              <label className={styles.fieldLabel} htmlFor="purge-reason">
                Reason for purging protected records (minimum 10 characters)
              </label>
              <textarea
                id="purge-reason"
                className={styles.fieldTextarea}
                value={reason}
                onChange={e => setReason(e.target.value)}
                rows={3}
                placeholder="Why these records must be purged despite the retention period"
              />
              {reason.trim() !== '' && !reasonValid && (
                <span className={styles.fieldError}>
                  At least 10 characters required ({reason.trim().length}/10)
                </span>
              )}
            </div>
          </div>
        )}

        <p className={styles.body}>
          This will permanently delete all {entityLabel} for your organisation.
          This action cannot be undone and deleted data cannot be recovered.
        </p>
        <label className={styles.checkboxRow}>
          <input
            type="checkbox"
            className={styles.checkbox}
            checked={confirmed}
            onChange={e => setConfirmed(e.target.checked)}
          />
          <span className={styles.checkboxLabel}>
            I understand this action is permanent and cannot be undone
          </span>
        </label>
        <div className={styles.actions}>
          <button className={styles.btnCancel} onClick={onCancel}>Cancel</button>
          <button
            className={styles.btnDelete}
            disabled={!canConfirm || isSubmitting}
            onClick={handleConfirm}
          >
            {isSubmitting ? 'Deleting...' : 'Delete permanently'}
          </button>
        </div>
      </div>
    </div>
  );
}
