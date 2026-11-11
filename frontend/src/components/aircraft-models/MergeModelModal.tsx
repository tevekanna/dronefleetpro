// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useState } from 'react';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { AlertTriangle } from 'lucide-react';
import { mergeAircraftModel, type AircraftModelWithCount } from '../../api/aircraftModels';
import styles from '../users/UserFormModal.module.css';

interface Props {
  source: AircraftModelWithCount;
  /** Active models in the same org, excluding `source`. */
  candidates: AircraftModelWithCount[];
  onClose: () => void;
}

export function MergeModelModal({ source, candidates, onClose }: Props) {
  const queryClient = useQueryClient();
  const [targetId, setTargetId] = useState<string>('');
  const [err, setErr] = useState<string | null>(null);
  const [summary, setSummary] = useState<string | null>(null);

  const m = useMutation({
    mutationFn: () => mergeAircraftModel(source.id, Number(targetId)),
    onSuccess: res => {
      setSummary(
        `Moved ${res.aircraft_moved} aircraft and ${res.mappings_moved} mapping(s) into '${res.target_name}'.` +
        (res.mappings_deleted_as_duplicates > 0
          ? ` ${res.mappings_deleted_as_duplicates} duplicate mapping(s) deleted.`
          : '')
      );
      queryClient.invalidateQueries({ queryKey: ['aircraft-models'] });
      queryClient.invalidateQueries({ queryKey: ['aircraft'] });
    },
    onError: (e: Error) => setErr(e.message),
  });

  const submit = (e: React.FormEvent) => {
    e.preventDefault();
    setErr(null);
    if (!targetId) { setErr('Choose a target model to merge into.'); return; }
    m.mutate();
  };

  return (
    <div className={styles.overlay} onClick={onClose}>
      <div className={styles.modal} onClick={e => e.stopPropagation()}>
        <div className={styles.title}>Merge '{source.name}' into another model</div>

        {summary ? (
          <>
            <div style={{ fontSize: '0.9rem', color: 'var(--color-text-primary)', marginBottom: 'var(--space-5)' }}>
              {summary}
            </div>
            <div className={styles.actions}>
              <button type="button" className={styles.btnSubmit} onClick={onClose}>Done</button>
            </div>
          </>
        ) : (
          <form onSubmit={submit}>
            <div
              style={{
                display: 'flex',
                gap: 'var(--space-3)',
                alignItems: 'flex-start',
                padding: 'var(--space-3)',
                marginBottom: 'var(--space-4)',
                background: 'rgba(245, 158, 11, 0.08)',
                border: '1px solid var(--color-warning)',
                borderRadius: 'var(--radius-sm)',
                color: 'var(--color-warning)',
                fontSize: '0.82rem',
                lineHeight: 1.45,
              }}
            >
              <AlertTriangle size={16} style={{ marginTop: 2, flexShrink: 0 }} />
              <div>
                This will move all aircraft and type-mappings from
                <strong> '{source.name}' </strong>
                to the selected target, then delete <strong>'{source.name}'</strong>.
                This cannot be undone.
              </div>
            </div>

            <div className={styles.field}>
              <label className={styles.label}>Target model *</label>
              <select
                className={styles.select}
                value={targetId}
                onChange={e => setTargetId(e.target.value)}
              >
                <option value="">— Choose target —</option>
                {candidates.map(c => (
                  <option key={c.id} value={c.id}>{c.name}</option>
                ))}
              </select>
              {candidates.length === 0 && (
                <div className={styles.helperText}>
                  No other active models available. Create one first or unretire an existing model.
                </div>
              )}
            </div>

            {err && <div className={styles.formError}>{err}</div>}

            <div className={styles.actions}>
              <button type="button" className={styles.btnCancel} onClick={onClose}>Cancel</button>
              <button
                type="submit"
                className={styles.btnSubmit}
                disabled={m.isPending || candidates.length === 0}
              >
                {m.isPending ? 'Merging…' : 'Merge'}
              </button>
            </div>
          </form>
        )}
      </div>
    </div>
  );
}
