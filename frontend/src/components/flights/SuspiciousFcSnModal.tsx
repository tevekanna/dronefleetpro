// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useState } from 'react';
import { AlertTriangle } from 'lucide-react';

const overlayStyle: React.CSSProperties = {
  position: 'fixed',
  inset: 0,
  background: 'rgba(0,0,0,0.7)',
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'center',
  // z-index sits above ImportFlightLogModal so this opens *over* the preview
  // stage rather than replacing it. Matches MergeAircraftModal's idiom of
  // overlaying on top of its parent EditAircraftModal.
  zIndex: 1200,
};

const modalStyle: React.CSSProperties = {
  background: 'var(--color-bg-elevated)',
  border: '1px solid var(--color-border-strong)',
  borderRadius: 'var(--radius-lg)',
  padding: 'var(--space-6)',
  width: '100%',
  maxWidth: '520px',
  boxShadow: 'var(--shadow-modal)',
  maxHeight: '90vh',
  overflowY: 'auto',
};

const titleStyle: React.CSSProperties = {
  fontFamily: 'var(--font-display)',
  fontSize: '1.1rem',
  color: 'var(--color-text-primary)',
  marginBottom: 'var(--space-2)',
};

const sublineStyle: React.CSSProperties = {
  fontFamily: 'var(--font-display)',
  fontSize: '0.82rem',
  color: 'var(--color-text-muted)',
  marginBottom: 'var(--space-5)',
};

const warningBannerStyle: React.CSSProperties = {
  display: 'flex',
  gap: 'var(--space-3)',
  alignItems: 'flex-start',
  padding: 'var(--space-3)',
  marginBottom: 'var(--space-5)',
  background: 'rgba(245, 158, 11, 0.08)',
  border: '1px solid var(--color-warning)',
  borderRadius: 'var(--radius-sm)',
  color: 'var(--color-warning)',
  fontSize: '0.82rem',
  lineHeight: 1.45,
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

const sectionHeadingStyle: React.CSSProperties = {
  fontFamily: 'var(--font-label)',
  fontSize: '0.78rem',
  textTransform: 'uppercase',
  letterSpacing: '0.06em',
  color: 'var(--color-text-secondary)',
  marginBottom: 'var(--space-3)',
};

const candidateListStyle: React.CSSProperties = {
  display: 'flex',
  flexDirection: 'column',
  gap: 'var(--space-2)',
  marginBottom: 'var(--space-5)',
};

const candidateRowStyle: React.CSSProperties = {
  display: 'flex',
  justifyContent: 'space-between',
  alignItems: 'center',
  gap: 'var(--space-3)',
  background: 'var(--color-bg-surface)',
  border: '1px solid var(--color-border)',
  borderRadius: 'var(--radius-sm)',
  padding: 'var(--space-3) var(--space-4)',
};

const candidatePrimaryStyle: React.CSSProperties = {
  fontFamily: 'var(--font-display)',
  fontSize: '0.95rem',
  color: 'var(--color-text-primary)',
};

const candidateSecondaryStyle: React.CSSProperties = {
  fontFamily: 'var(--font-display)',
  fontSize: '0.78rem',
  color: 'var(--color-text-muted)',
  marginTop: 'var(--space-1)',
};

const fieldStyle: React.CSSProperties = {
  display: 'flex',
  flexDirection: 'column',
  gap: 'var(--space-1)',
  marginBottom: 'var(--space-2)',
};

const labelStyle: React.CSSProperties = {
  fontSize: '0.8rem',
  color: 'var(--color-text-secondary)',
  fontFamily: 'var(--font-label)',
};

const textareaStyle: React.CSSProperties = {
  background: 'var(--color-bg-input)',
  border: '1px solid var(--color-border)',
  borderRadius: 'var(--radius-sm)',
  padding: 'var(--space-2) var(--space-3)',
  color: 'var(--color-text-primary)',
  fontSize: '0.9rem',
  fontFamily: 'var(--font-body)',
  resize: 'vertical',
  minHeight: '88px',
};

const cancelBtnStyle = (disabled: boolean): React.CSSProperties => ({
  background: 'transparent',
  color: 'var(--color-text-secondary)',
  border: '1px solid var(--color-border)',
  borderRadius: 'var(--radius-sm)',
  padding: 'var(--space-2) var(--space-4)',
  cursor: disabled ? 'not-allowed' : 'pointer',
  fontSize: '0.85rem',
  opacity: disabled ? 0.6 : 1,
});

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

const destructiveOutlineBtnStyle = (disabled: boolean): React.CSSProperties => ({
  background: 'transparent',
  color: 'var(--color-critical)',
  border: '1px solid var(--color-critical)',
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
  marginTop: 'var(--space-4)',
};

const splitActionRowStyle: React.CSSProperties = {
  display: 'flex',
  gap: 'var(--space-3)',
  justifyContent: 'space-between',
  alignItems: 'center',
  marginTop: 'var(--space-4)',
};

const errorTextStyle: React.CSSProperties = {
  color: 'var(--color-critical)',
  fontSize: '0.8rem',
  marginTop: 'var(--space-3)',
};

const helperStyle = (ok: boolean): React.CSSProperties => ({
  fontSize: '0.75rem',
  color: ok ? 'var(--color-text-secondary)' : 'var(--color-text-muted)',
  fontFamily: 'var(--font-label)',
  marginTop: 'var(--space-1)',
});

const bodyTextStyle: React.CSSProperties = {
  fontSize: '0.88rem',
  color: 'var(--color-text-primary)',
  lineHeight: 1.5,
  marginBottom: 'var(--space-4)',
};

export interface FcSnCandidate {
  id: number;
  serial_number: string;
  equipment_id: string;
  nickname: string | null;
}

export interface SuspiciousFcSnModalProps {
  sn: string;
  modelName: string;
  candidates: FcSnCandidate[];
  /** Retry the commit with link_existing_aircraft_id set. Caller is
   *  expected to throw on backend error so this modal can render the
   *  message inline; on success the caller unmounts this modal. */
  onLink: (aircraftId: number) => Promise<void>;
  /** Retry the commit with force_register_new + force_register_reason.
   *  Same throw-on-error / unmount-on-success contract as onLink. */
  onForceRegister: (reason: string) => Promise<void>;
  onCancel: () => void;
}

type Stage = 'pick' | 'force-confirm';

const MIN_REASON_LEN = 10;

export function SuspiciousFcSnModal({
  sn,
  modelName,
  candidates,
  onLink,
  onForceRegister,
  onCancel,
}: SuspiciousFcSnModalProps) {
  // When there are no candidates the modal opens straight on the
  // force-confirm stage — there's no candidate list to land on.
  const [stage, setStage] = useState<Stage>(
    candidates.length === 0 ? 'force-confirm' : 'pick',
  );
  const [reason, setReason] = useState<string>('');
  const [submitting, setSubmitting] = useState<boolean>(false);
  const [error, setError] = useState<string | null>(null);

  const handleLinkClick = async (aircraftId: number) => {
    setSubmitting(true);
    setError(null);
    try {
      await onLink(aircraftId);
      // Caller unmounts on success — no further state work needed here.
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Link failed');
      setSubmitting(false);
    }
  };

  const handleForceRegisterClick = async () => {
    const trimmed = reason.trim();
    if (trimmed.length < MIN_REASON_LEN) return;
    setSubmitting(true);
    setError(null);
    try {
      await onForceRegister(trimmed);
    } catch (e) {
      setError(e instanceof Error ? e.message : 'Force-register failed');
      setSubmitting(false);
    }
  };

  const trimmedLen = reason.trim().length;
  const reasonOk = trimmedLen >= MIN_REASON_LEN;

  return (
    <div style={overlayStyle} onClick={submitting ? undefined : onCancel}>
      <div style={modalStyle} onClick={(e) => e.stopPropagation()}>
        {stage === 'pick' && (
          <>
            <h2 style={titleStyle}>Suspicious serial number for {modelName}</h2>
            <div style={sublineStyle}>Log SN: {sn}</div>

            <div style={warningBannerStyle} role="alert">
              <AlertTriangle size={16} style={{ marginTop: 2, flexShrink: 0 }} />
              <div>
                The log SN above looks like a flight-controller SN, not the
                chassis body SN printed on the QR sticker. {modelName} airframes
                are known to record the FC SN in their logs. This drone is
                likely already registered under a different (body) SN — pick it
                from the list below to link this flight to it.
              </div>
            </div>

            <div style={sectionHeadingStyle}>
              Link this flight to an existing aircraft
            </div>
            <div style={candidateListStyle}>
              {candidates.map((c) => {
                const secondary =
                  c.nickname && c.nickname.trim().length > 0
                    ? `${c.serial_number} — ${c.nickname}`
                    : c.serial_number;
                return (
                  <div key={c.id} style={candidateRowStyle}>
                    <div>
                      <div style={candidatePrimaryStyle}>{c.equipment_id}</div>
                      <div style={candidateSecondaryStyle}>{secondary}</div>
                    </div>
                    <button
                      type="button"
                      disabled={submitting}
                      onClick={() => handleLinkClick(c.id)}
                      style={primaryBtnStyle(submitting)}
                    >
                      Link this aircraft
                    </button>
                  </div>
                );
              })}
            </div>

            <div style={splitActionRowStyle}>
              <button
                type="button"
                disabled={submitting}
                onClick={() => {
                  setError(null);
                  setStage('force-confirm');
                }}
                style={destructiveOutlineBtnStyle(submitting)}
              >
                Register as new aircraft instead
              </button>
              <button
                type="button"
                disabled={submitting}
                onClick={onCancel}
                style={cancelBtnStyle(submitting)}
              >
                Cancel
              </button>
            </div>
            {error && <div style={errorTextStyle}>{error}</div>}
          </>
        )}

        {stage === 'force-confirm' && (
          <>
            <h2 style={titleStyle}>Register new aircraft anyway</h2>
            <div style={sublineStyle}>
              Log SN: {sn} · Model: {modelName}
            </div>

            <div style={criticalBannerStyle} role="alert">
              <AlertTriangle size={16} style={{ marginTop: 2, flexShrink: 0 }} />
              <div>
                This will create a new aircraft row and may produce a duplicate
                if the SN actually belongs to an existing {modelName} in your
                fleet. Describe why this is not one of the candidates
                {candidates.length === 0
                  ? ' (no existing aircraft of this model in the fleet — confirm this is a brand-new unit)'
                  : ''}
                . The reason is audited.
              </div>
            </div>

            <p style={bodyTextStyle}>
              The new aircraft will be flagged <strong>needs_manual_sn</strong>{' '}
              until an admin verifies the SN by comparing it to the body QR
              sticker.
            </p>

            <div style={fieldStyle}>
              <label htmlFor="fc-sn-force-reason" style={labelStyle}>
                Reason *
              </label>
              <textarea
                id="fc-sn-force-reason"
                rows={4}
                style={textareaStyle}
                value={reason}
                onChange={(e) => setReason(e.target.value)}
                placeholder="e.g. New unit purchased 2026-04, not yet QR-scanned"
                disabled={submitting}
              />
              <div style={helperStyle(reasonOk)}>
                {trimmedLen} / {MIN_REASON_LEN} characters minimum
              </div>
            </div>

            <div style={actionRowStyle}>
              {candidates.length > 0 && (
                <button
                  type="button"
                  disabled={submitting}
                  onClick={() => {
                    setError(null);
                    setStage('pick');
                  }}
                  style={cancelBtnStyle(submitting)}
                >
                  Back to candidates
                </button>
              )}
              <button
                type="button"
                disabled={!reasonOk || submitting}
                onClick={handleForceRegisterClick}
                style={destructiveBtnStyle(!reasonOk || submitting)}
              >
                {submitting ? 'Registering…' : 'Confirm: register new aircraft'}
              </button>
            </div>
            {error && <div style={errorTextStyle}>{error}</div>}
          </>
        )}
      </div>
    </div>
  );
}
