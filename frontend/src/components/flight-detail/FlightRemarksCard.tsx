// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useState } from 'react';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import type { Flight } from '../../types/flight';
import { updateFlight } from '../../api/flights';
import { canEdit } from '../../utils/auth';
import { showToast } from '../../store/toastStore';
import styles from '../../pages/FlightDetailPage.module.css';

export function FlightRemarksCard({ flight }: { flight: Flight }) {
  const queryClient = useQueryClient();
  const editable = canEdit();
  const [isEditing, setIsEditing] = useState(false);
  const [draft, setDraft] = useState(flight.remarks ?? '');

  const mutation = useMutation({
    mutationFn: (remarks: string) => updateFlight(flight.id, { remarks }),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['flight-detail', flight.id] });
      queryClient.invalidateQueries({ queryKey: ['flights'] });
      setIsEditing(false);
      showToast('Remarks saved', 'success');
    },
    onError: (err) => {
      showToast(
        err instanceof Error ? err.message : 'Failed to save remarks',
        'error',
      );
    },
  });

  const startEdit = () => {
    setDraft(flight.remarks ?? '');
    setIsEditing(true);
  };

  const cancelEdit = () => {
    setDraft(flight.remarks ?? '');
    setIsEditing(false);
  };

  // The backend uses `remarks = COALESCE($3, remarks)`, so null keeps the
  // existing value. Always send a string; an empty string clears the field.
  const save = () => mutation.mutate(draft.trim());

  const hasRemarks = !!flight.remarks && flight.remarks.trim().length > 0;

  return (
    <div className={styles.section}>
      <div className={styles.remarksHeader}>
        <span className={styles.sectionHeading} style={{ marginBottom: 0 }}>
          Remarks
        </span>
        {editable && !isEditing && (
          <button
            type="button"
            className={styles.remarksEditBtn}
            onClick={startEdit}
          >
            {hasRemarks ? 'Edit' : 'Add remarks'}
          </button>
        )}
      </div>

      {isEditing ? (
        <div className={styles.remarksEditor}>
          <textarea
            className={styles.remarksTextarea}
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
            placeholder="Add notes about this flight…"
            rows={4}
            autoFocus
          />
          <div className={styles.remarksActions}>
            <button
              type="button"
              className={styles.remarksCancelBtn}
              onClick={cancelEdit}
              disabled={mutation.isPending}
            >
              Cancel
            </button>
            <button
              type="button"
              className={styles.remarksSaveBtn}
              onClick={save}
              disabled={mutation.isPending}
            >
              {mutation.isPending ? 'Saving…' : 'Save'}
            </button>
          </div>
        </div>
      ) : hasRemarks ? (
        <div className={styles.remarksBlock}>{flight.remarks}</div>
      ) : (
        <div className={styles.remarksEmpty}>No remarks recorded.</div>
      )}
    </div>
  );
}
