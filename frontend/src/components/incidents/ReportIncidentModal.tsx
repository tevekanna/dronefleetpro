// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useState, useMemo } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { createIncident, uploadIncidentAttachment } from '../../api/incidents';
import type { CreateIncidentRequest } from '../../api/incidents';
import { fetchFlights } from '../../api/flights';
import type { Aircraft } from '../../types/aircraft';
import type { Flight } from '../../types/flight';
import type { Pilot } from '../../api/pilots';
import { IsoDateTimeInput } from '../inputs/IsoDateTimeInput';
import { OccurrenceCodePicker } from './OccurrenceCodePicker';
import { formatAircraftLabel } from '../../utils/aircraftFormat';
import { showToast } from '../../store/toastStore';

// --- Styles (copied from IncidentsPage on extraction; kept inline to match
// the existing incident-modal pattern rather than introducing a CSS module). ---
const overlayStyle: React.CSSProperties = { position: 'fixed', inset: 0, background: 'rgba(0,0,0,0.7)', display: 'flex', alignItems: 'center', justifyContent: 'center', zIndex: 1000 };
const modalStyle: React.CSSProperties = { background: 'var(--color-bg-elevated)', border: '1px solid var(--color-border-strong)', borderRadius: 'var(--radius-lg)', padding: 'var(--space-6)', width: '100%', maxWidth: '520px', boxShadow: 'var(--shadow-modal)', maxHeight: '90vh', overflowY: 'auto' };
const fieldStyle: React.CSSProperties = { display: 'flex', flexDirection: 'column', gap: 'var(--space-1)', marginBottom: 'var(--space-4)' };
const labelStyle: React.CSSProperties = { fontSize: '0.8rem', color: 'var(--color-text-secondary)', fontFamily: 'var(--font-label)' };
const inputStyle: React.CSSProperties = { background: 'var(--color-bg-input)', border: '1px solid var(--color-border)', borderRadius: 'var(--radius-sm)', padding: 'var(--space-2) var(--space-3)', color: 'var(--color-text-primary)', fontSize: '0.9rem', fontFamily: 'var(--font-body)' };
const cancelBtnStyle: React.CSSProperties = { background: 'transparent', color: 'var(--color-text-secondary)', border: '1px solid var(--color-border)', borderRadius: 'var(--radius-sm)', padding: 'var(--space-2) var(--space-4)', cursor: 'pointer', fontSize: '0.85rem' };
const submitBtnStyle = (p: boolean): React.CSSProperties => ({ background: 'var(--color-accent)', color: 'var(--color-text-inverse)', border: 'none', borderRadius: 'var(--radius-sm)', padding: 'var(--space-2) var(--space-4)', cursor: 'pointer', fontSize: '0.85rem', fontWeight: 500, opacity: p ? 0.7 : 1 });

const SEVERITIES = ['low', 'medium', 'high', 'critical'];
const CATEGORIES = ['Equipment Failure', 'Weather', 'Airspace', 'Human Error', 'Bird Strike', 'Loss of Control', 'Other'];

function nowLocal(): string {
  const d = new Date();
  return d.toISOString().slice(0, 16);
}

// Readable flight descriptor for the optional flight picker: datetime · aircraft · #id.
function flightLabel(f: Flight): string {
  const when = f.flight_datetime ? f.flight_datetime.substring(0, 16).replace('T', ' ') : '—';
  const ac = f.aircraft ? formatAircraftLabel(f.aircraft) : (f.aircraft_name ?? 'Unknown aircraft');
  return `${when} · ${ac} · #${f.id}`;
}

export interface ReportIncidentModalProps {
  onClose: () => void;
  aircraft: Aircraft[];
  pilots: Pilot[];
  /** Prefill the flight association (e.g. from the flight-import hand-off).
   *  Included in the create payload even if the flight isn't in the picker. */
  initialFlightId?: number;
  /** Prefill the aircraft selection. */
  initialAircraftId?: number;
  /** ISO timestamp used as the initial occurred-at (sliced to the
   *  YYYY-MM-DDTHH:mm the IsoDateTimeInput expects). */
  initialOccurredAt?: string;
  /** When provided, the file is uploaded to the new incident after it is
   *  created. Upload failure surfaces a toast but never rolls back the
   *  already-created incident. */
  attachFile?: File;
  /** Called with the new incident id after a successful create (+ attach
   *  attempt). Existing callers omit this → unchanged behaviour. */
  onCreated?: (incidentId: number) => void;
}

// --- Report Incident Modal ---
export function ReportIncidentModal({
  onClose,
  aircraft,
  pilots,
  initialFlightId,
  initialAircraftId,
  initialOccurredAt,
  attachFile,
  onCreated,
}: ReportIncidentModalProps) {
  const queryClient = useQueryClient();
  const [occurredAt, setOccurredAt] = useState(
    initialOccurredAt ? initialOccurredAt.slice(0, 16) : nowLocal(),
  );
  const [severity, setSeverity] = useState('');
  const [category, setCategory] = useState('');
  const [aircraftId, setAircraftId] = useState(
    initialAircraftId != null ? String(initialAircraftId) : '',
  );
  const [pilotId, setPilotId] = useState('');
  const [flightId, setFlightId] = useState(
    initialFlightId != null ? String(initialFlightId) : '',
  );
  const [locationDesc, setLocationDesc] = useState('');
  const [description, setDescription] = useState('');
  const [immediateAction, setImmediateAction] = useState('');
  const [reportedToAuth, setReportedToAuth] = useState(false);
  // Optional Annex VI classification — an operator reporting in the moment
  // must never be blocked by a classification decision.
  const [occurrenceCode, setOccurrenceCode] = useState('');
  const [becameAwareAt, setBecameAwareAt] = useState(nowLocal());
  const [error, setError] = useState('');

  const { data: flightData } = useQuery({
    queryKey: ['flights', { perPage: 100 }],
    queryFn: () => fetchFlights({ perPage: 100 }),
  });
  const flights = flightData?.flights ?? [];

  // Flight picker only offers flights belonging to the currently selected aircraft.
  const aircraftFlights = useMemo(
    () => (aircraftId ? flights.filter(f => f.aircraft_id === Number(aircraftId)) : []),
    [flights, aircraftId],
  );

  const mutation = useMutation({
    mutationFn: (data: CreateIncidentRequest) => createIncident(data),
    onSuccess: async (incident) => {
      // Best-effort attachment: the incident already exists, so an upload
      // failure is surfaced but must not block or roll back.
      if (attachFile) {
        try {
          await uploadIncidentAttachment(incident.id, attachFile);
        } catch (err) {
          showToast(
            err instanceof Error
              ? err.message
              : 'Failed to attach log file to incident',
            'error',
          );
        }
      }
      queryClient.invalidateQueries({ queryKey: ['incidents'] });
      onCreated?.(incident.id);
      onClose();
    },
    onError: (err: Error) => setError(err.message),
  });

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (!occurredAt) { setError('Date/Time is required'); return; }
    if (!severity) { setError('Severity is required'); return; }
    if (!category) { setError('Category is required'); return; }
    if (description.trim().length < 10) { setError('Description must be at least 10 characters'); return; }
    setError('');
    mutation.mutate({
      occurred_at: occurredAt.includes('T') ? occurredAt + ':00Z' : occurredAt,
      severity,
      category,
      description: description.trim(),
      aircraft_id: aircraftId ? Number(aircraftId) : undefined,
      pilot_id: pilotId ? Number(pilotId) : undefined,
      flight_id: flightId ? Number(flightId) : undefined,
      location_description: locationDesc.trim() || undefined,
      immediate_action: immediateAction.trim() || undefined,
      reported_to_authority: reportedToAuth,
      occurrence_code: occurrenceCode || undefined,
      became_aware_at: becameAwareAt
        ? (becameAwareAt.includes('T') ? becameAwareAt + ':00Z' : becameAwareAt)
        : undefined,
    });
  };

  return (
    <div style={overlayStyle} onClick={onClose}>
      <div style={modalStyle} onClick={e => e.stopPropagation()}>
        <h2 style={{ fontFamily: 'var(--font-display)', fontSize: '1.1rem', color: 'var(--color-text-primary)', marginBottom: 'var(--space-5)' }}>Report Incident</h2>
        <form onSubmit={handleSubmit}>
          <div style={{ display: 'flex', gap: 'var(--space-4)' }}>
            <div style={{ ...fieldStyle, flex: 1 }}>
              <label style={labelStyle}>Date/Time *</label>
              <IsoDateTimeInput style={inputStyle} value={occurredAt} onChange={e => setOccurredAt(e.target.value)} required />
            </div>
            <div style={{ ...fieldStyle, flex: 1 }}>
              <label style={labelStyle}>Severity *</label>
              <select style={inputStyle} value={severity} onChange={e => setSeverity(e.target.value)} required>
                <option value="">— Select —</option>
                {SEVERITIES.map(s => <option key={s} value={s}>{s.charAt(0).toUpperCase() + s.slice(1)}</option>)}
              </select>
            </div>
          </div>
          <div style={fieldStyle}>
            <label style={labelStyle}>Category *</label>
            <select style={inputStyle} value={category} onChange={e => setCategory(e.target.value)} required>
              <option value="">— Select —</option>
              {CATEGORIES.map(c => <option key={c} value={c}>{c}</option>)}
            </select>
          </div>
          <div style={{ display: 'flex', gap: 'var(--space-4)' }}>
            <div style={{ ...fieldStyle, flex: 1 }}>
              <label style={labelStyle}>Aircraft</label>
              <select style={inputStyle} value={aircraftId} onChange={e => { setAircraftId(e.target.value); setFlightId(''); }}>
                <option value="">— None —</option>
                {aircraft.map(a => <option key={a.id} value={a.id}>{formatAircraftLabel(a)}</option>)}
              </select>
            </div>
            <div style={{ ...fieldStyle, flex: 1 }}>
              <label style={labelStyle}>Pilot</label>
              <select style={inputStyle} value={pilotId} onChange={e => setPilotId(e.target.value)}>
                <option value="">— None —</option>
                {pilots.filter(p => p.is_active).map(p => <option key={p.id} value={p.id}>{p.name}</option>)}
              </select>
            </div>
          </div>
          <div style={fieldStyle}>
            <label style={labelStyle}>Flight (optional)</label>
            <select style={inputStyle} value={flightId} onChange={e => setFlightId(e.target.value)} disabled={aircraftId === ''}>
              {aircraftId === '' ? (
                <option value="">— Select an aircraft first —</option>
              ) : aircraftFlights.length === 0 ? (
                <option value="">— No flights for this aircraft —</option>
              ) : (
                <>
                  <option value="">— None —</option>
                  {aircraftFlights.map(f => <option key={f.id} value={f.id}>{flightLabel(f)}</option>)}
                </>
              )}
            </select>
          </div>
          <div style={fieldStyle}>
            <label style={labelStyle}>Location Description</label>
            <input style={inputStyle} value={locationDesc} onChange={e => setLocationDesc(e.target.value)} placeholder="e.g. Near Gothenburg Airport, 200m AGL" />
          </div>
          <div style={fieldStyle}>
            <label style={labelStyle}>Description * (min 10 chars)</label>
            <textarea style={{ ...inputStyle, minHeight: '80px', resize: 'vertical' }} value={description} onChange={e => setDescription(e.target.value)} placeholder="Describe what happened in detail..." required />
          </div>
          <div style={fieldStyle}>
            <label style={labelStyle}>Immediate Action Taken</label>
            <textarea style={{ ...inputStyle, minHeight: '60px', resize: 'vertical' }} value={immediateAction} onChange={e => setImmediateAction(e.target.value)} placeholder="What was done immediately after the incident..." />
          </div>
          <div style={fieldStyle}>
            <label style={labelStyle}>Annex VI classification (optional)</label>
            <OccurrenceCodePicker value={occurrenceCode} onChange={setOccurrenceCode} />
          </div>
          <div style={fieldStyle}>
            <label style={labelStyle}>Became aware (optional — defaults to now)</label>
            <IsoDateTimeInput style={inputStyle} value={becameAwareAt} onChange={e => setBecameAwareAt(e.target.value)} />
            <span style={{ fontSize: '0.72rem', lineHeight: 1.3, color: 'var(--color-text-muted)' }}>
              The 72-hour clock under Article 4(7) of Regulation (EU) No 376/2014 runs from when the
              organisation became aware of the occurrence, not from when it happened.
            </span>
          </div>
          <div style={{ display: 'flex', alignItems: 'center', gap: 'var(--space-2)', marginBottom: 'var(--space-4)' }}>
            <input type="checkbox" id="reportedAuth" checked={reportedToAuth} onChange={e => setReportedToAuth(e.target.checked)} style={{ accentColor: 'var(--color-accent)' }} />
            <label htmlFor="reportedAuth" style={{ ...labelStyle, cursor: 'pointer' }}>Reported to aviation authority</label>
          </div>
          {error && <p style={{ color: 'var(--color-critical)', fontSize: '0.8rem', marginBottom: 'var(--space-3)' }}>{error}</p>}
          <div style={{ display: 'flex', gap: 'var(--space-3)', justifyContent: 'flex-end' }}>
            <button type="button" onClick={onClose} style={cancelBtnStyle}>Cancel</button>
            <button type="submit" disabled={mutation.isPending} style={submitBtnStyle(mutation.isPending)}>{mutation.isPending ? 'Saving...' : 'Report Incident'}</button>
          </div>
        </form>
      </div>
    </div>
  );
}
