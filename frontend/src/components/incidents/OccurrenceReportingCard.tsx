// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { updateIncident } from '../../api/incidents';
import type { Incident, ReportingStatus, UpdateOccurrenceReportingRequest } from '../../api/incidents';
import { fetchOccurrenceTaxonomy } from '../../api/occurrenceTaxonomy';
import { IsoDateTimeInput } from '../inputs/IsoDateTimeInput';
import { OccurrenceCodePicker } from './OccurrenceCodePicker';
import { ReportingStatusBadge } from '../shared/ReportingStatusBadge';
import { showToast } from '../../store/toastStore';

// Inline styles matching the IncidentDetailPage card pattern.
const cardStyle: React.CSSProperties = { background: 'var(--color-bg-surface)', border: '1px solid var(--color-border)', borderRadius: 'var(--radius-md)', padding: 'var(--space-5)', marginBottom: 'var(--space-5)' };
const sectionHeading: React.CSSProperties = { fontFamily: 'var(--font-display)', fontSize: '0.95rem', color: 'var(--color-text-primary)', letterSpacing: '0.02em' };
const fieldLabel: React.CSSProperties = { fontSize: '0.72rem', color: 'var(--color-text-muted)', fontFamily: 'var(--font-label)', textTransform: 'uppercase', letterSpacing: '0.05em', marginBottom: 'var(--space-1)' };
const fieldValue: React.CSSProperties = { fontSize: '0.9rem', color: 'var(--color-text-primary)', fontFamily: 'var(--font-body)', whiteSpace: 'pre-wrap' };
const monoStyle: React.CSSProperties = { fontFamily: 'var(--font-display)', fontSize: '0.85rem' };
const helperStyle: React.CSSProperties = { fontSize: '0.72rem', lineHeight: 1.3, color: 'var(--color-text-muted)', marginTop: '2px' };
const explanationStyle: React.CSSProperties = { fontSize: '0.85rem', color: 'var(--color-text-secondary)', margin: 'var(--space-3) 0 var(--space-4)' };
const advisoryStyle: React.CSSProperties = { fontSize: '0.8rem', color: 'var(--color-text-secondary)', background: 'rgba(99, 102, 241, 0.08)', border: '1px solid var(--color-info)', borderRadius: 'var(--radius-sm)', padding: 'var(--space-2) var(--space-3)', marginBottom: 'var(--space-4)' };
const inputStyle: React.CSSProperties = { background: 'var(--color-bg-input)', border: '1px solid var(--color-border)', borderRadius: 'var(--radius-sm)', padding: 'var(--space-2) var(--space-3)', color: 'var(--color-text-primary)', fontSize: '0.9rem', fontFamily: 'var(--font-body)', width: '100%' };
const labelStyle: React.CSSProperties = { fontSize: '0.8rem', color: 'var(--color-text-secondary)', fontFamily: 'var(--font-label)' };
const fieldStyle: React.CSSProperties = { display: 'flex', flexDirection: 'column', gap: 'var(--space-1)', marginBottom: 'var(--space-4)' };
const btnSmall: React.CSSProperties = { background: 'transparent', color: 'var(--color-text-secondary)', border: '1px solid var(--color-border)', borderRadius: 'var(--radius-sm)', padding: 'var(--space-1) var(--space-3)', cursor: 'pointer', fontSize: '0.8rem' };
const btnAccent: React.CSSProperties = { background: 'var(--color-accent)', color: 'var(--color-text-inverse)', border: 'none', borderRadius: 'var(--radius-sm)', padding: 'var(--space-2) var(--space-4)', fontSize: '0.85rem', fontWeight: 500, cursor: 'pointer' };

const AWARENESS_HELPER =
  'The 72-hour clock under Article 4(7) of Regulation (EU) No 376/2014 runs from when the organisation became aware of the occurrence, not from when it happened.';

const STATE_EXPLANATIONS: Record<string, string> = {
  not_applicable:
    'This aircraft model does not require a design certificate or declaration, so Annex VI mandatory occurrence reporting does not apply. Classification remains available for voluntary reporting.',
  undetermined:
    'The design-approval status of the aircraft model has not been assessed. No reporting deadline can be shown until it is.',
  unclassified:
    'Mandatory reporting applies to this aircraft model, but the occurrence has not yet been classified against Annex VI.',
  awaiting_awareness:
    'The occurrence is classified, but the time the organisation became aware has not been recorded — the 72-hour reporting clock has not started.',
  reported: 'The occurrence has been reported to the competent authority.',
  due: 'A mandatory report is due within 72 hours of the organisation becoming aware of the occurrence.',
  overdue: 'The 72-hour reporting window has passed without a recorded report.',
};

function formatDateTime(d: string | null): string {
  if (!d) return '—';
  const date = d.substring(0, 10);
  const time = d.length >= 16 ? d.substring(11, 16) : '';
  return time ? `${date} ${time}` : date;
}

/** "YYYY-MM-DDTHH:mm" input value → ISO instant, '' → null (explicit clear). */
function toApiTimestamp(v: string): string | null {
  const trimmed = v.trim();
  if (!trimmed) return null;
  return trimmed.includes('T') ? `${trimmed}:00Z` : trimmed;
}

interface Props {
  incident: Incident;
  reporting: ReportingStatus;
  canEdit: boolean;
}

/// Occurrence-classification and 72-hour-reporting card on the incident
/// detail page. A deadline / countdown is rendered ONLY inside the
/// `reporting.deadline_at != null` branch — mirroring the backend guarantee
/// that no deadline exists unless applicability is established and the
/// occurrence is classified.
export function OccurrenceReportingCard({ incident, reporting, canEdit }: Props) {
  // HOOKS — all before any early return (Rules of Hooks).
  const queryClient = useQueryClient();
  const [editing, setEditing] = useState(false);
  const [code, setCode] = useState(incident.occurrence_code ?? '');
  const [awareAt, setAwareAt] = useState(incident.became_aware_at?.slice(0, 16) ?? '');
  const [reportedAt, setReportedAt] = useState(incident.reported_at?.slice(0, 16) ?? '');
  const [delayReason, setDelayReason] = useState(incident.reporting_delay_reason ?? '');
  const { data: taxonomy } = useQuery({
    queryKey: ['occurrence-taxonomy'],
    queryFn: fetchOccurrenceTaxonomy,
    staleTime: Infinity,
  });

  const mutation = useMutation({
    mutationFn: (payload: UpdateOccurrenceReportingRequest) => updateIncident(incident.id, payload),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['incident-detail', incident.id] });
      queryClient.invalidateQueries({ queryKey: ['incidents'] });
      setEditing(false);
      showToast('Occurrence reporting updated', 'success');
    },
    onError: (err: Error) => showToast(err.message, 'error'),
  });

  const startEdit = () => {
    setCode(incident.occurrence_code ?? '');
    setAwareAt(incident.became_aware_at?.slice(0, 16) ?? '');
    setReportedAt(incident.reported_at?.slice(0, 16) ?? '');
    setDelayReason(incident.reporting_delay_reason ?? '');
    setEditing(true);
  };

  const handleSave = () => {
    // All four fields are tri-state on the backend: a cleared value must
    // reach it as an explicit null, never as an omitted key.
    mutation.mutate({
      occurrence_code: code || null,
      became_aware_at: toApiTimestamp(awareAt),
      reported_at: toApiTimestamp(reportedAt),
      reporting_delay_reason: delayReason.trim() || null,
    });
  };

  const classification = taxonomy?.categories.find(c => c.code === incident.occurrence_code) ?? null;
  const explanation = STATE_EXPLANATIONS[reporting.state] ?? '';
  const deadlineColour =
    reporting.state === 'overdue' ? 'var(--color-critical)'
    : reporting.state === 'due' ? 'var(--color-warning)'
    : 'var(--color-text-primary)';

  return (
    <div style={cardStyle}>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 'var(--space-3)' }}>
        <span style={sectionHeading}>Occurrence Reporting</span>
        <div style={{ display: 'flex', alignItems: 'center', gap: 'var(--space-3)' }}>
          <ReportingStatusBadge state={reporting.state} />
          {canEdit && !editing && <button style={btnSmall} onClick={startEdit}>Edit</button>}
        </div>
      </div>

      {explanation && <p style={explanationStyle}>{explanation}</p>}

      {/* Deadline block — rendered ONLY when the backend emitted a deadline.
          No countdown, date or urgency styling exists outside this branch. */}
      {reporting.deadline_at != null && (
        <div style={{ marginBottom: 'var(--space-4)' }}>
          <div style={fieldLabel}>72-hour reporting deadline</div>
          <div style={{ ...monoStyle, color: deadlineColour }}>
            {formatDateTime(reporting.deadline_at)}
            {reporting.hours_remaining != null && (
              reporting.hours_remaining >= 0
                ? ` · ${reporting.hours_remaining} h remaining`
                : ` · ${Math.abs(reporting.hours_remaining)} h past deadline`
            )}
          </div>
        </div>
      )}

      {reporting.record_incomplete && (
        <div style={advisoryStyle}>
          The report is recorded, but the record cannot evidence that it was timely or which
          Annex VI item it covered. Consider adding the missing classification or awareness time.
        </div>
      )}

      {!editing ? (
        <div>
          <div style={{ marginBottom: 'var(--space-4)' }}>
            <div style={fieldLabel}>Annex VI classification</div>
            <div style={fieldValue}>
              {incident.occurrence_code ? (
                <>
                  <span style={monoStyle}>{incident.occurrence_code}</span>
                  {classification && <span style={{ color: 'var(--color-text-secondary)' }}> — {classification.text}</span>}
                </>
              ) : '—'}
            </div>
          </div>
          <div style={{ display: 'flex', flexWrap: 'wrap', gap: 'var(--space-6)', marginBottom: delayReasonBlock(incident) ? 'var(--space-4)' : 0 }}>
            <div>
              <div style={fieldLabel}>Became aware</div>
              <div style={monoStyle}>{formatDateTime(incident.became_aware_at)}</div>
            </div>
            <div>
              <div style={fieldLabel}>Reported</div>
              <div style={monoStyle}>{formatDateTime(incident.reported_at)}</div>
            </div>
          </div>
          {delayReasonBlock(incident) && (
            <div>
              <div style={fieldLabel}>Reporting delay reason</div>
              <div style={fieldValue}>{incident.reporting_delay_reason}</div>
            </div>
          )}
        </div>
      ) : (
        <div>
          <div style={fieldStyle}>
            <label style={labelStyle}>Annex VI classification</label>
            <OccurrenceCodePicker value={code} onChange={setCode} disabled={mutation.isPending} />
          </div>
          <div style={fieldStyle}>
            <label style={labelStyle}>Became aware</label>
            <IsoDateTimeInput style={inputStyle} value={awareAt} onChange={e => setAwareAt(e.target.value)} />
            <div style={helperStyle}>{AWARENESS_HELPER}</div>
          </div>
          <div style={fieldStyle}>
            <label style={labelStyle}>Reported to authority at</label>
            <IsoDateTimeInput style={inputStyle} value={reportedAt} onChange={e => setReportedAt(e.target.value)} />
          </div>
          <div style={fieldStyle}>
            <label style={labelStyle}>Reporting delay reason</label>
            <textarea
              style={{ ...inputStyle, minHeight: '60px', resize: 'vertical' }}
              value={delayReason}
              onChange={e => setDelayReason(e.target.value)}
              placeholder="Exceptional circumstances preventing reporting within 72 hours (Article 4(7))…"
            />
          </div>
          <div style={{ display: 'flex', gap: 'var(--space-3)', justifyContent: 'flex-end' }}>
            <button style={btnSmall} onClick={() => setEditing(false)} disabled={mutation.isPending}>Cancel</button>
            <button style={btnAccent} onClick={handleSave} disabled={mutation.isPending}>
              {mutation.isPending ? 'Saving…' : 'Save'}
            </button>
          </div>
        </div>
      )}
    </div>
  );
}

function delayReasonBlock(incident: Incident): boolean {
  return !!(incident.reporting_delay_reason && incident.reporting_delay_reason.trim().length > 0);
}
