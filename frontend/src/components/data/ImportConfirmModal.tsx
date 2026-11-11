// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useEffect, useState } from 'react';
import type { ConflictStrategy, ImportSummary } from '../../types/dataTransfer';
import styles from './ImportConfirmModal.module.css';

interface ImportConfirmModalProps {
  isOpen: boolean;
  strategy: ConflictStrategy;
  preview: ImportSummary | null;
  isSubmitting: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}

export function ImportConfirmModal({
  isOpen,
  strategy,
  preview,
  isSubmitting,
  onConfirm,
  onCancel,
}: ImportConfirmModalProps) {
  const [acknowledged, setAcknowledged] = useState(false);

  // Reset checkbox each time the modal opens.
  useEffect(() => {
    if (isOpen) setAcknowledged(false);
  }, [isOpen]);

  if (!isOpen) return null;

  const created = preview?.total_created ?? 0;
  const updated = preview?.total_updated ?? 0;
  const skipped = preview?.total_skipped ?? 0;
  const failed = preview?.total_failed ?? 0;

  const strategyLabel: Record<ConflictStrategy, string> = {
    skip: 'Skip existing',
    update: 'Update existing',
    fail: 'Fail on conflict',
  };

  const requireAck = strategy === 'update';
  const confirmDisabled = isSubmitting || (requireAck && !acknowledged);

  const confirmLabel =
    strategy === 'update' ? 'Confirm overwrite' : 'Apply import';

  return (
    <div className={styles.overlay} onClick={onCancel}>
      <div className={styles.modal} onClick={(e) => e.stopPropagation()}>
        <h2 className={styles.title}>Confirm Import</h2>

        {strategy === 'skip' && (
          <>
            <p className={styles.body}>
              You are about to import data using the{' '}
              <strong>{strategyLabel.skip}</strong> strategy. Existing records in
              your organisation will not be changed; only new ones will be created.
            </p>
            <ul className={styles.bulletList}>
              <li>{created} record{created === 1 ? '' : 's'} will be created</li>
              <li>{skipped} record{skipped === 1 ? '' : 's'} will be skipped (already exist)</li>
              {failed > 0 && (
                <li className={styles.warningText}>
                  {failed} record{failed === 1 ? '' : 's'} will fail (FK or constraint)
                </li>
              )}
            </ul>
          </>
        )}

        {strategy === 'update' && (
          <>
            <p className={styles.warningPara}>
              ⚠ You are about to import data using the{' '}
              <strong>{strategyLabel.update}</strong> strategy. Existing records that
              match the imported data will be <strong>overwritten</strong>. This
              action is recorded in the audit log but cannot be automatically undone.
            </p>
            <ul className={styles.bulletList}>
              <li>{created} record{created === 1 ? '' : 's'} will be created</li>
              <li>
                {updated + skipped} record{updated + skipped === 1 ? '' : 's'} will be{' '}
                <strong>overwritten</strong>
                {' '}
                <span className={styles.note}>
                  (scrubbed-secret values are preserved)
                </span>
              </li>
              {failed > 0 && (
                <li className={styles.warningText}>
                  {failed} record{failed === 1 ? '' : 's'} will fail (FK or constraint)
                </li>
              )}
            </ul>
            <label className={styles.ackRow}>
              <input
                type="checkbox"
                checked={acknowledged}
                onChange={(e) => setAcknowledged(e.target.checked)}
              />
              <span>I understand existing data will be overwritten</span>
            </label>
          </>
        )}

        {strategy === 'fail' && (
          <>
            <p className={styles.body}>
              You are about to import data using the{' '}
              <strong>{strategyLabel.fail}</strong> strategy. If any record already
              exists, the entire import will be aborted with no changes made.
            </p>
            <ul className={styles.bulletList}>
              <li>{created} record{created === 1 ? '' : 's'} will be created</li>
              {skipped + updated + failed > 0 && (
                <li className={styles.warningText}>
                  If {skipped + updated + failed} conflict{skipped + updated + failed === 1 ? '' : 's'}{' '}
                  exist, the import will fail.
                </li>
              )}
            </ul>
          </>
        )}

        <div className={styles.actions}>
          <button className={styles.btnCancel} onClick={onCancel} disabled={isSubmitting}>
            Cancel
          </button>
          <button
            className={`${styles.btnConfirm} ${strategy === 'update' ? styles.btnConfirmDanger : ''}`}
            onClick={onConfirm}
            disabled={confirmDisabled}
          >
            {isSubmitting ? 'Importing…' : confirmLabel}
          </button>
        </div>
      </div>
    </div>
  );
}
