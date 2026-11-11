// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useEffect, useState } from 'react';
import { useMutation } from '@tanstack/react-query';
import { patchAirworthinessStatus } from '../../api/maintenanceAircraft';
import styles from './AirworthinessModal.module.css';

type Airworthiness = 'airworthy' | 'not_airworthy';

interface Props {
  aircraftId: number;
  aircraftLabel: string;
  currentStatus: Airworthiness;
  onClose: () => void;
  onSuccess: () => void;
}

const STATUS_LABEL: Record<Airworthiness, string> = {
  airworthy: 'AIRWORTHY',
  not_airworthy: 'NOT AIRWORTHY',
};

const MIN_COMMENT = 5;

export function AirworthinessModal({
  aircraftId,
  aircraftLabel,
  currentStatus,
  onClose,
  onSuccess,
}: Props) {
  const [status, setStatus] = useState<Airworthiness>(currentStatus);
  const [comment, setComment] = useState<string>('');

  const mutation = useMutation({
    mutationFn: () => patchAirworthinessStatus(aircraftId, status, comment.trim()),
    onSuccess: () => {
      onSuccess();
    },
  });

  // Reset transient state when the aircraft target changes.
  useEffect(() => {
    setStatus(currentStatus);
    setComment('');
    mutation.reset();
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [aircraftId, currentStatus]);

  const trimmedComment = comment.trim();
  const commentOk = trimmedComment.length >= MIN_COMMENT;
  const statusChanged = status !== currentStatus;
  const canSubmit = commentOk && statusChanged && !mutation.isPending;

  function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    if (!canSubmit) return;
    mutation.mutate();
  }

  return (
    <div className={styles.overlay} onClick={onClose} role="presentation">
      <div
        className={styles.modal}
        onClick={(e) => e.stopPropagation()}
        role="dialog"
        aria-modal="true"
        aria-labelledby="airworthiness-modal-title"
      >
        <h2 id="airworthiness-modal-title" className={styles.title}>
          Change Airworthiness Status
        </h2>

        <div className={styles.contextStrip}>
          <span className={styles.contextLabel}>Aircraft</span>
          <strong className={styles.contextValue}>{aircraftLabel}</strong>
          <span className={styles.contextSep}>·</span>
          <span className={styles.contextLabel}>Current</span>
          <span
            className={`${styles.badge} ${
              currentStatus === 'airworthy' ? styles.badgeOk : styles.badgeCritical
            }`}
          >
            {STATUS_LABEL[currentStatus]}
          </span>
        </div>

        {mutation.isError && (
          <div className={styles.errorBanner}>
            {(mutation.error as Error).message ?? 'Failed to update airworthiness status'}
          </div>
        )}

        <form onSubmit={handleSubmit}>
          <fieldset className={styles.fieldset}>
            <legend className={styles.legend}>New status</legend>
            <label className={styles.radioRow}>
              <input
                type="radio"
                name="airworthiness"
                value="airworthy"
                checked={status === 'airworthy'}
                onChange={() => setStatus('airworthy')}
              />
              <span className={`${styles.badge} ${styles.badgeOk}`}>AIRWORTHY</span>
              <span className={styles.radioHint}>Cleared for flight</span>
            </label>
            <label className={styles.radioRow}>
              <input
                type="radio"
                name="airworthiness"
                value="not_airworthy"
                checked={status === 'not_airworthy'}
                onChange={() => setStatus('not_airworthy')}
              />
              <span className={`${styles.badge} ${styles.badgeCritical}`}>NOT AIRWORTHY</span>
              <span className={styles.radioHint}>Grounded pending maintenance</span>
            </label>
          </fieldset>

          <div className={styles.field}>
            <label className={styles.label} htmlFor="airworthiness-comment">
              Reason / Comment <span className={styles.required}>*</span>
            </label>
            <textarea
              id="airworthiness-comment"
              className={styles.textarea}
              placeholder="e.g. Awaiting propeller kit, ETA 3 days"
              value={comment}
              onChange={(e) => setComment(e.target.value)}
              rows={3}
            />
            <span className={styles.hint}>
              {trimmedComment.length === 0
                ? 'A short reason is required and recorded in the audit log.'
                : commentOk
                ? `${trimmedComment.length} chars · stored in the audit log`
                : `Must be at least ${MIN_COMMENT} characters`}
            </span>
          </div>

          <div className={styles.actions}>
            <button
              type="button"
              className={styles.btnCancel}
              onClick={onClose}
              disabled={mutation.isPending}
            >
              Cancel
            </button>
            <button type="submit" className={styles.btnPrimary} disabled={!canSubmit}>
              {mutation.isPending ? 'Saving…' : 'Save Status'}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
