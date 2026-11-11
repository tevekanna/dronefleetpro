// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { fetchIncidents, updateIncident, deleteIncident } from '../api/incidents';
import type { Incident, IncidentWithReporting } from '../api/incidents';
import { fetchAircraft } from '../api/aircraft';
import { fetchPilots } from '../api/pilots';
import type { Aircraft } from '../types/aircraft';
import { useCurrentUser } from '../hooks/useCurrentUser';
import { userHasAnyRole } from '../utils/auth';
import { AircraftCell } from '../components/shared/AircraftCell';
import { SeverityBadge } from '../components/shared/SeverityBadge';
import { ReportingStatusBadge } from '../components/shared/ReportingStatusBadge';
import { ReportIncidentModal } from '../components/incidents/ReportIncidentModal';

// --- Styles ---
const tableStyle: React.CSSProperties = { width: '100%', borderCollapse: 'collapse', fontFamily: 'var(--font-body)', fontSize: '0.85rem' };
const thStyle: React.CSSProperties = { textAlign: 'left', padding: 'var(--space-2) var(--space-3)', color: 'var(--color-text-muted)', fontFamily: 'var(--font-label)', fontSize: '0.75rem', fontWeight: 500, textTransform: 'uppercase', letterSpacing: '0.05em', borderBottom: '1px solid var(--color-border)' };
const tdStyle: React.CSSProperties = { padding: 'var(--space-2) var(--space-3)', borderBottom: '1px solid var(--color-border)', color: 'var(--color-text-primary)' };
const monoStyle: React.CSSProperties = { fontFamily: 'var(--font-display)', fontSize: '0.8rem' };
const addBtnStyle: React.CSSProperties = { background: 'var(--color-accent)', color: 'var(--color-text-inverse)', border: 'none', borderRadius: 'var(--radius-sm)', padding: 'var(--space-2) var(--space-4)', fontSize: '0.85rem', fontWeight: 500, cursor: 'pointer' };
const btnSmall: React.CSSProperties = { background: 'transparent', color: 'var(--color-text-secondary)', border: '1px solid var(--color-border)', borderRadius: 'var(--radius-sm)', padding: 'var(--space-1) var(--space-2)', cursor: 'pointer', fontSize: '0.8rem', marginRight: 'var(--space-1)' };
const btnDanger: React.CSSProperties = { ...btnSmall, color: 'var(--color-critical)', borderColor: 'var(--color-critical)' };
const overlayStyle: React.CSSProperties = { position: 'fixed', inset: 0, background: 'rgba(0,0,0,0.7)', display: 'flex', alignItems: 'center', justifyContent: 'center', zIndex: 1000 };
const modalStyle: React.CSSProperties = { background: 'var(--color-bg-elevated)', border: '1px solid var(--color-border-strong)', borderRadius: 'var(--radius-lg)', padding: 'var(--space-6)', width: '100%', maxWidth: '520px', boxShadow: 'var(--shadow-modal)', maxHeight: '90vh', overflowY: 'auto' };
const fieldStyle: React.CSSProperties = { display: 'flex', flexDirection: 'column', gap: 'var(--space-1)', marginBottom: 'var(--space-4)' };
const labelStyle: React.CSSProperties = { fontSize: '0.8rem', color: 'var(--color-text-secondary)', fontFamily: 'var(--font-label)' };
const inputStyle: React.CSSProperties = { background: 'var(--color-bg-input)', border: '1px solid var(--color-border)', borderRadius: 'var(--radius-sm)', padding: 'var(--space-2) var(--space-3)', color: 'var(--color-text-primary)', fontSize: '0.9rem', fontFamily: 'var(--font-body)' };
const cancelBtnStyle: React.CSSProperties = { background: 'transparent', color: 'var(--color-text-secondary)', border: '1px solid var(--color-border)', borderRadius: 'var(--radius-sm)', padding: 'var(--space-2) var(--space-4)', cursor: 'pointer', fontSize: '0.85rem' };
const submitBtnStyle = (p: boolean): React.CSSProperties => ({ background: 'var(--color-accent)', color: 'var(--color-text-inverse)', border: 'none', borderRadius: 'var(--radius-sm)', padding: 'var(--space-2) var(--space-4)', cursor: 'pointer', fontSize: '0.85rem', fontWeight: 500, opacity: p ? 0.7 : 1 });

function statusBadge(status: string): React.ReactNode {
  const color = status === 'open' ? 'var(--color-critical)' : status === 'closed' ? 'var(--color-ok)' : 'var(--color-warning)';
  return <span style={{ color, fontWeight: 500 }}>{status}</span>;
}

function truncate(s: string, len: number): string { return s.length > len ? s.substring(0, len) + '...' : s; }
function formatDate(d: string | null): string { return d ? d.substring(0, 10) : '—'; }

// --- Incidents Table ---
function IncidentTable({ onAdd, onClose, aircraft }: { onAdd: () => void; onClose: (i: Incident) => void; aircraft: Aircraft[] }) {
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const { data: currentUser } = useCurrentUser();
  const canReport = userHasAnyRole(currentUser, 'admin', 'safety_officer', 'technician', 'pilot');
  const canCloseIncident = userHasAnyRole(currentUser, 'admin', 'safety_officer');
  const canDelete = userHasAnyRole(currentUser, 'admin');

  const { data: incidents, isLoading, error } = useQuery({ queryKey: ['incidents'], queryFn: fetchIncidents });
  const delMutation = useMutation({
    mutationFn: deleteIncident,
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['incidents'] }),
  });

  const handleDelete = (id: number) => {
    if (window.confirm('Delete this incident record?')) delMutation.mutate(id);
  };

  const findAircraft = (id: number | null) => aircraft.find(a => a.id === id);

  if (isLoading) return <p style={{ color: 'var(--color-text-secondary)' }}>Loading incidents...</p>;
  if (error) return <p style={{ color: 'var(--color-critical)' }}>Failed to load: {(error as Error).message}</p>;

  return (
    <div>
      <div style={{ display: 'flex', justifyContent: 'flex-end', marginBottom: 'var(--space-4)' }}>
        {canReport && <button onClick={onAdd} style={addBtnStyle}>Report Incident</button>}
      </div>
      {!incidents || incidents.length === 0 ? (
        <div style={{ display: 'flex', flexDirection: 'column', alignItems: 'center', justifyContent: 'center', padding: 'var(--space-10) var(--space-5)', background: 'var(--color-bg-surface)', border: '1px dashed var(--color-border)', borderRadius: 'var(--radius-md)' }}>
          <p style={{ fontFamily: 'var(--font-display)', fontSize: '1rem', color: 'var(--color-text-secondary)', marginBottom: 'var(--space-2)' }}>No incidents reported</p>
          <p style={{ fontSize: '0.85rem', color: 'var(--color-text-muted)' }}>Report and track safety incidents here.</p>
        </div>
      ) : (
        <table style={tableStyle}>
          <thead><tr>
            <th style={thStyle}>Date</th>
            <th style={thStyle}>Severity</th>
            <th style={thStyle}>Category</th>
            <th style={thStyle}>Aircraft</th>
            <th style={thStyle}>Description</th>
            <th style={thStyle}>Status</th>
            <th style={thStyle}>Reporting</th>
            <th style={thStyle}>Reported</th>
            <th style={thStyle}>Actions</th>
          </tr></thead>
          <tbody>
            {incidents.map((i: IncidentWithReporting) => {
              const ac = findAircraft(i.aircraft_id);
              return (
                <tr key={i.id} onClick={() => navigate(`/incidents/${i.id}`)} style={{ cursor: 'pointer' }}>
                  <td style={{ ...tdStyle, ...monoStyle }}>{formatDate(i.occurred_at)}</td>
                  <td style={tdStyle}><SeverityBadge severity={i.severity} /></td>
                  <td style={tdStyle}>{i.category}</td>
                  <td style={tdStyle}><AircraftCell aircraft={ac ?? null} /></td>
                  <td style={tdStyle}>{truncate(i.description, 40)}</td>
                  <td style={tdStyle}>{statusBadge(i.status)}</td>
                  {/* Reporting badge only — deliberately no deadline column,
                      so the list never implies urgency for deadline-free states. */}
                  <td style={tdStyle}><ReportingStatusBadge state={i.reporting.state} /></td>
                  <td style={tdStyle}>
                    <span style={{ color: i.reported_to_authority ? 'var(--color-warning)' : 'var(--color-text-muted)' }}>
                      {i.reported_to_authority ? 'Yes' : 'No'}
                    </span>
                  </td>
                  <td style={tdStyle}>
                    {i.status !== 'closed' && i.status !== 'resolved' && canCloseIncident && (
                      <button style={{ ...btnSmall, color: 'var(--color-ok)', borderColor: 'var(--color-ok)' }} onClick={(e) => { e.stopPropagation(); onClose(i); }}>Close</button>
                    )}
                    {canDelete && (
                      <button style={btnDanger} onClick={(e) => { e.stopPropagation(); handleDelete(i.id); }}>Delete</button>
                    )}
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
      )}
    </div>
  );
}

// --- Close Incident Modal ---
function CloseIncidentModal({ incident, onClose }: { incident: Incident; onClose: () => void }) {
  const queryClient = useQueryClient();
  const [rootCause, setRootCause] = useState('');
  const [correctiveAction, setCorrectiveAction] = useState('');
  const [authorityRef, setAuthorityRef] = useState('');
  const [error, setError] = useState('');

  const mutation = useMutation({
    mutationFn: () => updateIncident(incident.id, {
      status: 'closed',
      root_cause: rootCause.trim(),
      corrective_action: correctiveAction.trim(),
      authority_ref: authorityRef.trim() || undefined,
    }),
    onSuccess: () => { queryClient.invalidateQueries({ queryKey: ['incidents'] }); onClose(); },
    onError: (err: Error) => setError(err.message),
  });

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (rootCause.trim().length < 5) { setError('Root cause is required (min 5 chars)'); return; }
    if (correctiveAction.trim().length < 5) { setError('Corrective action is required (min 5 chars)'); return; }
    setError('');
    mutation.mutate();
  };

  return (
    <div style={overlayStyle} onClick={onClose}>
      <div style={{ ...modalStyle, maxWidth: '460px' }} onClick={e => e.stopPropagation()}>
        <h2 style={{ fontFamily: 'var(--font-display)', fontSize: '1.1rem', color: 'var(--color-text-primary)', marginBottom: 'var(--space-2)' }}>Close Incident</h2>
        <p style={{ fontSize: '0.8rem', color: 'var(--color-text-secondary)', marginBottom: 'var(--space-5)' }}>
          <SeverityBadge severity={incident.severity} /> — {truncate(incident.description, 60)}
        </p>
        <form onSubmit={handleSubmit}>
          <div style={fieldStyle}>
            <label style={labelStyle}>Root Cause *</label>
            <textarea style={{ ...inputStyle, minHeight: '70px', resize: 'vertical' }} value={rootCause} onChange={e => setRootCause(e.target.value)} placeholder="What was the root cause of this incident..." required />
          </div>
          <div style={fieldStyle}>
            <label style={labelStyle}>Corrective Action *</label>
            <textarea style={{ ...inputStyle, minHeight: '70px', resize: 'vertical' }} value={correctiveAction} onChange={e => setCorrectiveAction(e.target.value)} placeholder="What corrective actions were taken or planned..." required />
          </div>
          {incident.reported_to_authority && (
            <div style={fieldStyle}>
              <label style={labelStyle}>Authority Reference</label>
              <input style={inputStyle} value={authorityRef} onChange={e => setAuthorityRef(e.target.value)} placeholder="e.g. SE-TSIB-2025-042" />
            </div>
          )}
          {error && <p style={{ color: 'var(--color-critical)', fontSize: '0.8rem', marginBottom: 'var(--space-3)' }}>{error}</p>}
          <div style={{ display: 'flex', gap: 'var(--space-3)', justifyContent: 'flex-end' }}>
            <button type="button" onClick={onClose} style={cancelBtnStyle}>Cancel</button>
            <button type="submit" disabled={mutation.isPending} style={submitBtnStyle(mutation.isPending)}>{mutation.isPending ? 'Closing...' : 'Close Incident'}</button>
          </div>
        </form>
      </div>
    </div>
  );
}

// --- Main Incidents Page ---
export function IncidentsPage() {
  const [showReport, setShowReport] = useState(false);
  const [closingIncident, setClosingIncident] = useState<Incident | null>(null);

  // TODO: queryKey intent says active-only but we pass include_retired=true
  // to preserve historical behavior. Per-site audit needed: should this dropdown
  // include retired items (for historical records) or active only (for new entries)?
  // Tracked separately. Reverting to active-only would change UX and may hide
  // aircraft from edit forms of historical records.
  const { data: aircraft } = useQuery({ queryKey: ['aircraft', true], queryFn: () => fetchAircraft(true) });
  const { data: pilots } = useQuery({ queryKey: ['pilots'], queryFn: fetchPilots });
  const aircraftList = aircraft ?? [];
  const pilotList = pilots ?? [];

  return (
    <div>
      <h1 style={{ fontFamily: 'var(--font-display)', fontSize: '1.25rem', fontWeight: 500, color: 'var(--color-text-primary)', letterSpacing: '0.02em', marginBottom: 'var(--space-5)' }}>
        Incidents
      </h1>
      <IncidentTable onAdd={() => setShowReport(true)} onClose={i => setClosingIncident(i)} aircraft={aircraftList} />
      {showReport && <ReportIncidentModal onClose={() => setShowReport(false)} aircraft={aircraftList} pilots={pilotList} />}
      {closingIncident && <CloseIncidentModal incident={closingIncident} onClose={() => setClosingIncident(null)} />}
    </div>
  );
}
