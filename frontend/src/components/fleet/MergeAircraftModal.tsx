// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useMemo, useState } from 'react';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { AlertTriangle } from 'lucide-react';
import { mergeAircraft, type AircraftMergeResult } from '../../api/aircraft';
import type { Aircraft } from '../../types/aircraft';
import { formatAircraftLabel } from '../../utils/aircraftFormat';
import { showToast } from '../../store/toastStore';

const overlayStyle: React.CSSProperties = {
  position: 'fixed',
  inset: 0,
  background: 'rgba(0,0,0,0.7)',
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'center',
  zIndex: 1100,
};

const modalStyle: React.CSSProperties = {
  background: 'var(--color-bg-elevated)',
  border: '1px solid var(--color-border-strong)',
  borderRadius: 'var(--radius-lg)',
  padding: 'var(--space-6)',
  width: '100%',
  maxWidth: '460px',
  boxShadow: 'var(--shadow-modal)',
};

const titleStyle: React.CSSProperties = {
  fontFamily: 'var(--font-display)',
  fontSize: '1.1rem',
  color: 'var(--color-text-primary)',
  marginBottom: 'var(--space-5)',
};

const fieldStyle: React.CSSProperties = {
  display: 'flex',
  flexDirection: 'column',
  gap: 'var(--space-1)',
  marginBottom: 'var(--space-4)',
};

const labelStyle: React.CSSProperties = {
  fontSize: '0.8rem',
  color: 'var(--color-text-secondary)',
  fontFamily: 'var(--font-label)',
};

const inputStyle: React.CSSProperties = {
  background: 'var(--color-bg-input)',
  border: '1px solid var(--color-border)',
  borderRadius: 'var(--radius-sm)',
  padding: 'var(--space-2) var(--space-3)',
  color: 'var(--color-text-primary)',
  fontSize: '0.9rem',
  fontFamily: 'var(--font-body)',
};

const cancelBtnStyle: React.CSSProperties = {
  background: 'transparent',
  color: 'var(--color-text-secondary)',
  border: '1px solid var(--color-border)',
  borderRadius: 'var(--radius-sm)',
  padding: 'var(--space-2) var(--space-4)',
  cursor: 'pointer',
  fontSize: '0.85rem',
};

const primaryBtnStyle = (disabled: boolean): React.CSSProperties => ({
  background: 'var(--color-accent)',
  color: 'var(--color-text-inverse)',
  border: 'none',
  borderRadius: 'var(--radius-sm)',
  padding: 'var(--space-2) var(--space-4)',
  cursor: disabled ? 'not-allowed' : 'pointer',
  fontSize: '0.85rem',
  fontWeight: 500,
  opacity: disabled ? 0.5 : 1,
});

const destructiveBtnStyle = (disabled: boolean): React.CSSProperties => ({
  background: 'var(--color-critical)',
  color: 'var(--color-text-inverse)',
  border: 'none',
  borderRadius: 'var(--radius-sm)',
  padding: 'var(--space-2) var(--space-4)',
  cursor: disabled ? 'not-allowed' : 'pointer',
  fontSize: '0.85rem',
  fontWeight: 500,
  opacity: disabled ? 0.6 : 1,
});

const actionRowStyle: React.CSSProperties = {
  display: 'flex',
  gap: 'var(--space-3)',
  justifyContent: 'flex-end',
};

const criticalBannerStyle: React.CSSProperties = {
  display: 'flex',
  gap: 'var(--space-3)',
  alignItems: 'flex-start',
  padding: 'var(--space-3)',
  marginBottom: 'var(--space-4)',
  background: 'rgba(239, 68, 68, 0.08)',
  border: '1px solid var(--color-critical)',
  borderRadius: 'var(--radius-sm)',
  color: 'var(--color-critical)',
  fontSize: '0.82rem',
  lineHeight: 1.45,
};

const bodyTextStyle: React.CSSProperties = {
  fontSize: '0.88rem',
  color: 'var(--color-text-primary)',
  lineHeight: 1.5,
  marginBottom: 'var(--space-4)',
};

const emptyStateTextStyle: React.CSSProperties = {
  fontSize: '0.88rem',
  color: 'var(--color-text-secondary)',
  lineHeight: 1.5,
  marginBottom: 'var(--space-5)',
};

const errorTextStyle: React.CSSProperties = {
  color: 'var(--color-critical)',
  fontSize: '0.8rem',
  marginBottom: 'var(--space-3)',
};

const summaryListStyle: React.CSSProperties = {
  background: 'var(--color-bg-surface)',
  border: '1px solid var(--color-border)',
  borderRadius: 'var(--radius-sm)',
  padding: 'var(--space-3) var(--space-4)',
  marginBottom: 'var(--space-5)',
  display: 'flex',
  flexDirection: 'column',
  gap: 'var(--space-2)',
};

const summaryRowStyle: React.CSSProperties = {
  display: 'flex',
  justifyContent: 'space-between',
  alignItems: 'baseline',
  fontSize: '0.85rem',
  color: 'var(--color-text-primary)',
  fontFamily: 'var(--font-body)',
};

const summaryValueStyle: React.CSSProperties = {
  fontFamily: 'var(--font-display)',
  color: 'var(--color-accent)',
};

type Stage = 'pick' | 'confirm' | 'done';

interface Props {
  source: Aircraft;
  allAircraft: Aircraft[];
  onClose: () => void;
  /** Closes both this modal AND the parent EditAircraftModal once the
   *  merge has completed and the user dismisses the summary. */
  onSuccessClose: () => void;
}

export function MergeAircraftModal({ source, allAircraft, onClose, onSuccessClose }: Props) {
  const queryClient = useQueryClient();
  const [stage, setStage] = useState<Stage>('pick');
  const [targetId, setTargetId] = useState<string>('');
  const [error, setError] = useState<string | null>(null);
  const [result, setResult] = useState<AircraftMergeResult | null>(null);

  // Only active aircraft of the SAME model with a verified SN are
  // valid merge targets. A pending row should never absorb another
  // pending row — both still need operator review.
  const eligibleTargets = useMemo(
    () =>
      allAircraft.filter(
        (t) =>
          t.id !== source.id &&
          t.model_id !== null &&
          t.model_id === source.model_id &&
          t.status === 'active' &&
          t.needs_manual_sn === false,
      ),
    [allAircraft, source],
  );

  const target = useMemo(
    () => eligibleTargets.find((t) => String(t.id) === targetId) ?? null,
    [eligibleTargets, targetId],
  );

  const mutation = useMutation({
    mutationFn: (id: number) => mergeAircraft(source.id, id),
    onSuccess: (res) => {
      setResult(res);
      queryClient.invalidateQueries({ queryKey: ['aircraft'] });
      queryClient.invalidateQueries({ queryKey: ['aircraft-detail'] });
      queryClient.invalidateQueries({ queryKey: ['aircraft-flights'] });
      queryClient.invalidateQueries({ queryKey: ['flights'] });
      if (target) {
        showToast(`Aircraft merged into ${formatAircraftLabel(target)}`, 'success');
      }
      setStage('done');
    },
    onError: (e: Error) => setError(e.message),
  });

  const sourceLabel = formatAircraftLabel(source);
  const sourceSn = source.serial_number;
  const sourceEqId = source.equipment_id ?? source.serial_number;

  const handleConfirm = (e: React.FormEvent) => {
    e.preventDefault();
    if (!target) return;
    setError(null);
    mutation.mutate(target.id);
  };

  return (
    <div style={overlayStyle} onClick={onClose}>
      <div style={modalStyle} onClick={(e) => e.stopPropagation()}>
        {stage === 'pick' && (
          <>
            <h2 style={titleStyle}>Merge into existing aircraft</h2>
            {eligibleTargets.length === 0 ? (
              <>
                <p style={emptyStateTextStyle}>
                  No active aircraft of the same model are available to merge into.
                  Use the Serial Number field above to manually correct this
                  aircraft's SN instead.
                </p>
                <div style={actionRowStyle}>
                  <button type="button" onClick={onClose} style={cancelBtnStyle}>
                    Close
                  </button>
                </div>
              </>
            ) : (
              <form
                onSubmit={(e) => {
                  e.preventDefault();
                  if (target) setStage('confirm');
                }}
              >
                <p style={bodyTextStyle}>
                  Merge <strong>{sourceLabel}</strong> into another aircraft of the
                  same model. All flights, batteries, controllers, defects,
                  incidents, maintenance records, and identifiers will move to
                  the target. The source aircraft will be deleted.
                </p>
                <div style={fieldStyle}>
                  <label style={labelStyle}>Target aircraft *</label>
                  <select
                    style={inputStyle}
                    value={targetId}
                    onChange={(e) => setTargetId(e.target.value)}
                  >
                    <option value="">— Select target —</option>
                    {eligibleTargets.map((t) => (
                      <option key={t.id} value={t.id}>
                        {formatAircraftLabel(t)}
                      </option>
                    ))}
                  </select>
                </div>
                <div style={actionRowStyle}>
                  <button type="button" onClick={onClose} style={cancelBtnStyle}>
                    Cancel
                  </button>
                  <button type="submit" disabled={!target} style={primaryBtnStyle(!target)}>
                    Next
                  </button>
                </div>
              </form>
            )}
          </>
        )}

        {stage === 'confirm' && target && (
          <>
            <h2 style={titleStyle}>Confirm merge</h2>
            <div style={criticalBannerStyle} role="alert">
              <AlertTriangle size={16} style={{ marginTop: 2, flexShrink: 0 }} />
              <div>
                Merge <strong>{sourceEqId}</strong> ({sourceSn}) into{' '}
                <strong>{target.equipment_id ?? target.serial_number}</strong> (
                {target.serial_number}). All flights, controllers, batteries, and
                identifiers will be moved. Duplicate schedule templates on the
                source will be deleted. The source aircraft will be permanently
                removed. <strong>This cannot be undone.</strong>
              </div>
            </div>
            {error && <p style={errorTextStyle}>{error}</p>}
            <form onSubmit={handleConfirm}>
              <div style={actionRowStyle}>
                <button
                  type="button"
                  onClick={() => {
                    setError(null);
                    setStage('pick');
                  }}
                  style={cancelBtnStyle}
                  disabled={mutation.isPending}
                >
                  Back
                </button>
                <button
                  type="submit"
                  disabled={mutation.isPending}
                  style={destructiveBtnStyle(mutation.isPending)}
                >
                  {mutation.isPending ? 'Merging…' : 'Confirm Merge'}
                </button>
              </div>
            </form>
          </>
        )}

        {stage === 'done' && result && (
          <>
            <h2 style={titleStyle}>Merge complete</h2>
            <p style={bodyTextStyle}>
              Source aircraft removed. The following items were moved into the
              target:
            </p>
            {(() => {
              const rows: Array<[string, number]> = [
                ['Flights moved', result.flights_moved],
                ['Batteries moved', result.batteries_moved],
                ['Controllers moved', result.controllers_moved],
                ['Defects moved', result.defects_moved],
                ['Incidents moved', result.incidents_moved],
                ['Maintenance records moved', result.maintenance_moved],
                ['Schedules deleted as duplicates', result.schedules_deleted_as_duplicates],
                ['Identifiers migrated', result.identifiers_migrated],
              ];
              const nonzero = rows.filter(([, v]) => v > 0);
              if (nonzero.length === 0) {
                return (
                  <p style={emptyStateTextStyle}>
                    The source had no linked records — only the aircraft row itself was removed.
                  </p>
                );
              }
              return (
                <div style={summaryListStyle}>
                  {nonzero.map(([label, value]) => (
                    <div key={label} style={summaryRowStyle}>
                      <span>{label}</span>
                      <span style={summaryValueStyle}>{value}</span>
                    </div>
                  ))}
                </div>
              );
            })()}
            <div style={actionRowStyle}>
              <button
                type="button"
                onClick={onSuccessClose}
                style={primaryBtnStyle(false)}
              >
                Done
              </button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
