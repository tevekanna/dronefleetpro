// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useState } from 'react';
import { useMutation } from '@tanstack/react-query';
import { createDefect } from '../../api/maintenanceAircraft';
import { IsoDateInput } from '../inputs/IsoDateInput';
import styles from './AddDefectModal.module.css';

interface Props {
  aircraftId: number;
  aircraftLabel: string;
  onClose: () => void;
  onSuccess: () => void;
}

const MIN_DESCRIPTION = 10;
const CODE_PATTERN = /^[A-Z]{3}$/;

function todayIso(): string {
  return new Date().toISOString().slice(0, 10);
}

export function AddDefectModal({
  aircraftId,
  aircraftLabel,
  onClose,
  onSuccess,
}: Props) {
  const [defectDate, setDefectDate] = useState<string>(todayIso());
  const [reportedBy, setReportedBy] = useState<string>('');
  const [description, setDescription] = useState<string>('');
  const [submitAttempted, setSubmitAttempted] = useState<boolean>(false);

  const mutation = useMutation({
    mutationFn: () =>
      createDefect({
        aircraft_id: aircraftId,
        defect_date: defectDate,
        reported_by: reportedBy,
        description: description.trim(),
      }),
    onSuccess: () => {
      onSuccess();
    },
  });

  const trimmedDescription = description.trim();
  const dateOk = defectDate.length > 0;
  const codeOk = CODE_PATTERN.test(reportedBy);
  const descriptionOk = trimmedDescription.length >= MIN_DESCRIPTION;
  const canSubmit = dateOk && codeOk && descriptionOk && !mutation.isPending;

  function handleSubmit(e: React.FormEvent) {
    e.preventDefault();
    setSubmitAttempted(true);
    if (!canSubmit) return;
    mutation.mutate();
  }

  const showDateError = submitAttempted && !dateOk;
  const showCodeError = submitAttempted && !codeOk;
  const showDescriptionError = submitAttempted && !descriptionOk;

  return (
    <div className={styles.overlay} onClick={onClose} role="presentation">
      <div
        className={styles.modal}
        onClick={(e) => e.stopPropagation()}
        role="dialog"
        aria-modal="true"
        aria-labelledby="add-defect-modal-title"
      >
        <h2 id="add-defect-modal-title" className={styles.title}>
          Report Defect — {aircraftLabel}
        </h2>

        {mutation.isError && (
          <div className={styles.errorBanner}>
            {(mutation.error as Error).message ?? 'Failed to report defect'}
          </div>
        )}

        <form onSubmit={handleSubmit}>
          <div className={styles.field}>
            <label className={styles.label} htmlFor="defect-date">
              Defect date <span className={styles.required}>*</span>
            </label>
            <IsoDateInput
              id="defect-date"
              className={styles.input}
              value={defectDate}
              onChange={(e) => setDefectDate(e.target.value)}
              max={todayIso()}
            />
            {showDateError && (
              <span className={styles.errorText}>Date is required</span>
            )}
          </div>

          <div className={styles.field}>
            <label className={styles.label} htmlFor="defect-reported-by">
              Reported by <span className={styles.required}>*</span> (3-letter code)
            </label>
            <input
              id="defect-reported-by"
              type="text"
              className={`${styles.input} ${styles.inputMono}`}
              maxLength={3}
              value={reportedBy}
              onChange={(e) =>
                setReportedBy(e.target.value.toUpperCase().replace(/[^A-Z]/g, '').slice(0, 3))
              }
              placeholder="ABC"
              autoComplete="off"
            />
            {showCodeError && (
              <span className={styles.errorText}>Must be exactly 3 uppercase letters</span>
            )}
          </div>

          <div className={styles.field}>
            <label className={styles.label} htmlFor="defect-description">
              Description <span className={styles.required}>*</span>
            </label>
            <textarea
              id="defect-description"
              className={styles.textarea}
              placeholder="Describe the defect clearly — what was observed, where, and when."
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              minLength={MIN_DESCRIPTION}
              rows={4}
            />
            <span className={styles.hint}>
              {trimmedDescription.length === 0
                ? `Minimum ${MIN_DESCRIPTION} characters.`
                : descriptionOk
                ? `${trimmedDescription.length} chars`
                : `Must be at least ${MIN_DESCRIPTION} characters (currently ${trimmedDescription.length})`}
            </span>
            {showDescriptionError && (
              <span className={styles.errorText}>
                Description must be at least {MIN_DESCRIPTION} characters
              </span>
            )}
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
              {mutation.isPending ? 'Saving…' : 'Report Defect'}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
