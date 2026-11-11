// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useRef, useState } from 'react';
import { useParams, useNavigate, Link } from 'react-router-dom';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import {
  fetchIncidentDetail,
  addIncidentNote,
  deleteIncidentAttachment,
  uploadIncidentAttachment,
  downloadIncidentAttachment,
} from '../api/incidents';
import type { IncidentAttachment, IncidentAttachmentKind } from '../api/incidents';
import { fetchAircraft } from '../api/aircraft';
import { fetchPilots } from '../api/pilots';
import { useCurrentUser } from '../hooks/useCurrentUser';
import { userHasAnyRole } from '../utils/auth';
import { AircraftCell } from '../components/shared/AircraftCell';
import { OccurrenceReportingCard } from '../components/incidents/OccurrenceReportingCard';
import { showToast } from '../store/toastStore';

// --- Styles (mirrors IncidentsPage inline-style + token approach) ---
const cardStyle: React.CSSProperties = { background: 'var(--color-bg-surface)', border: '1px solid var(--color-border)', borderRadius: 'var(--radius-md)', padding: 'var(--space-5)', marginBottom: 'var(--space-5)' };
const sectionHeading: React.CSSProperties = { fontFamily: 'var(--font-display)', fontSize: '0.95rem', color: 'var(--color-text-primary)', marginBottom: 'var(--space-4)', letterSpacing: '0.02em' };
const fieldLabel: React.CSSProperties = { fontSize: '0.72rem', color: 'var(--color-text-muted)', fontFamily: 'var(--font-label)', textTransform: 'uppercase', letterSpacing: '0.05em', marginBottom: 'var(--space-1)' };
const fieldValue: React.CSSProperties = { fontSize: '0.9rem', color: 'var(--color-text-primary)', fontFamily: 'var(--font-body)', whiteSpace: 'pre-wrap' };
const monoStyle: React.CSSProperties = { fontFamily: 'var(--font-display)', fontSize: '0.85rem' };
const btnSmall: React.CSSProperties = { background: 'transparent', color: 'var(--color-text-secondary)', border: '1px solid var(--color-border)', borderRadius: 'var(--radius-sm)', padding: 'var(--space-1) var(--space-3)', cursor: 'pointer', fontSize: '0.8rem' };
const btnDanger: React.CSSProperties = { ...btnSmall, color: 'var(--color-critical)', borderColor: 'var(--color-critical)' };
const btnAccent: React.CSSProperties = { background: 'var(--color-accent)', color: 'var(--color-text-inverse)', border: 'none', borderRadius: 'var(--radius-sm)', padding: 'var(--space-2) var(--space-4)', fontSize: '0.85rem', fontWeight: 500, cursor: 'pointer' };
const inputStyle: React.CSSProperties = { background: 'var(--color-bg-input)', border: '1px solid var(--color-border)', borderRadius: 'var(--radius-sm)', padding: 'var(--space-2) var(--space-3)', color: 'var(--color-text-primary)', fontSize: '0.9rem', fontFamily: 'var(--font-body)' };
const linkStyle: React.CSSProperties = { color: 'var(--color-accent)', textDecoration: 'none', fontWeight: 500 };

const ATTACHMENT_ACCEPT = '.pdf,.txt,.log,.csv,.json,.kml,.gpx,.bin,.tlog,.ulg,.png,.jpg,.jpeg,.gif,.webp';

const KIND_LABELS: Record<IncidentAttachmentKind, string> = {
  ardupilot_bin: 'ArduPilot .bin',
  ardupilot_tlog: 'ArduPilot .tlog',
  log: 'Log',
  document: 'Document',
  image: 'Image',
  other: 'Other',
};

function severityBadge(sev: string): React.ReactNode {
  const colors: Record<string, { bg: string; fg: string }> = {
    critical: { bg: 'rgba(239,68,68,0.15)', fg: 'var(--color-critical)' },
    high: { bg: 'rgba(245,158,11,0.15)', fg: 'var(--color-warning)' },
    medium: { bg: 'rgba(99,102,241,0.15)', fg: 'var(--color-info)' },
    low: { bg: 'rgba(74,85,104,0.15)', fg: 'var(--color-text-muted)' },
  };
  const c = colors[sev] ?? colors.low;
  return (
    <span style={{ display: 'inline-block', padding: '2px 8px', borderRadius: 'var(--radius-sm)', fontSize: '0.75rem', fontFamily: 'var(--font-display)', background: c.bg, color: c.fg, fontWeight: 600, textTransform: 'uppercase' }}>
      {sev}
    </span>
  );
}

function statusBadge(status: string): React.ReactNode {
  const color = status === 'open' ? 'var(--color-critical)' : status === 'closed' ? 'var(--color-ok)' : 'var(--color-warning)';
  return <span style={{ color, fontWeight: 500 }}>{status}</span>;
}

function formatDateTime(d: string | null): string {
  if (!d) return '—';
  const date = d.substring(0, 10);
  const time = d.length >= 16 ? d.substring(11, 16) : '';
  return time ? `${date} ${time}` : date;
}

function formatSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function Field({ label, value }: { label: string; value: React.ReactNode }) {
  return (
    <div style={{ marginBottom: 'var(--space-4)' }}>
      <div style={fieldLabel}>{label}</div>
      <div style={fieldValue}>{value}</div>
    </div>
  );
}

export function IncidentDetailPage() {
  // HOOKS — all before any early return (Rules of Hooks).
  const params = useParams();
  const id = Number(params.id);
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const { data: currentUser } = useCurrentUser();
  const fileInputRef = useRef<HTMLInputElement | null>(null);
  const [noteDraft, setNoteDraft] = useState('');

  const detailQuery = useQuery({
    queryKey: ['incident-detail', id],
    queryFn: () => fetchIncidentDetail(id),
    enabled: Number.isFinite(id),
  });
  const { data: aircraftList } = useQuery({ queryKey: ['aircraft', true], queryFn: () => fetchAircraft(true) });
  const { data: pilots } = useQuery({ queryKey: ['pilots'], queryFn: fetchPilots });

  const invalidate = () => queryClient.invalidateQueries({ queryKey: ['incident-detail', id] });

  const uploadMutation = useMutation({
    mutationFn: (file: File) => uploadIncidentAttachment(id, file),
    onSuccess: () => { invalidate(); showToast('Attachment uploaded', 'success'); },
    onError: (err: Error) => showToast(err.message, 'error'),
  });
  const deleteAttMutation = useMutation({
    mutationFn: (attId: number) => deleteIncidentAttachment(id, attId),
    onSuccess: () => { invalidate(); showToast('Attachment deleted', 'success'); },
    onError: (err: Error) => showToast(err.message, 'error'),
  });
  const downloadMutation = useMutation({
    mutationFn: (att: IncidentAttachment) => downloadIncidentAttachment(id, att),
    onError: (err: Error) => showToast(err.message, 'error'),
  });
  const noteMutation = useMutation({
    mutationFn: (note: string) => addIncidentNote(id, note),
    onSuccess: () => { setNoteDraft(''); invalidate(); showToast('Note added', 'success'); },
    onError: (err: Error) => showToast(err.message, 'error'),
  });

  // --- Branching in JSX (hooks already run above) ---
  if (!Number.isFinite(id)) {
    return <p style={{ color: 'var(--color-critical)' }}>Invalid incident id.</p>;
  }
  if (detailQuery.isLoading) {
    return <p style={{ color: 'var(--color-text-secondary)' }}>Loading incident…</p>;
  }
  if (detailQuery.error || !detailQuery.data) {
    return <p style={{ color: 'var(--color-critical)' }}>Failed to load incident: {(detailQuery.error as Error | undefined)?.message ?? 'not found'}</p>;
  }

  const { incident, reporting, attachments, notes } = detailQuery.data;
  const isOwner = currentUser?.id != null && incident.created_by_user_id === currentUser.id;
  const canModify = userHasAnyRole(currentUser, 'admin', 'safety_officer') || isOwner;
  const canDeleteAttachment = userHasAnyRole(currentUser, 'admin', 'safety_officer');

  const linkedAircraft = incident.aircraft_id != null ? (aircraftList ?? []).find(a => a.id === incident.aircraft_id) ?? null : null;
  const linkedPilot = incident.pilot_id != null ? (pilots ?? []).find(p => p.id === incident.pilot_id) ?? null : null;

  const handleFilePick = (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (file) uploadMutation.mutate(file);
    e.target.value = ''; // allow re-selecting the same file
  };

  const handleDeleteAttachment = (att: IncidentAttachment) => {
    if (window.confirm(`Delete attachment "${att.original_filename}"?`)) {
      deleteAttMutation.mutate(att.id);
    }
  };

  const handleAddNote = () => {
    const trimmed = noteDraft.trim();
    if (!trimmed) { showToast('Note cannot be empty', 'error'); return; }
    noteMutation.mutate(trimmed);
  };

  const optionalField = (label: string, value: string | null | undefined) =>
    value && value.trim().length > 0 ? <Field key={label} label={label} value={value} /> : null;

  return (
    <div>
      <button onClick={() => navigate('/incidents')} style={{ ...btnSmall, marginBottom: 'var(--space-4)' }}>← Back to incidents</button>

      {/* Header */}
      <div style={{ ...cardStyle, display: 'flex', flexWrap: 'wrap', gap: 'var(--space-5)', alignItems: 'center' }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 'var(--space-3)' }}>
          {severityBadge(incident.severity)}
          {statusBadge(incident.status)}
        </div>
        <div>
          <span style={fieldLabel}>Occurred</span>
          <div style={{ ...monoStyle, color: 'var(--color-text-primary)' }}>{formatDateTime(incident.occurred_at)}</div>
        </div>
        <div>
          <span style={fieldLabel}>Category</span>
          <div style={{ fontSize: '0.9rem', color: 'var(--color-text-primary)' }}>{incident.category}</div>
        </div>
        {incident.reported_to_authority && (
          <div>
            <span style={fieldLabel}>Reported to authority</span>
            <div style={{ fontSize: '0.9rem', color: 'var(--color-warning)' }}>Yes</div>
          </div>
        )}
      </div>

      {/* Details */}
      <div style={cardStyle}>
        <div style={sectionHeading}>Details</div>
        <Field label="Description" value={incident.description} />
        {optionalField('Immediate Action', incident.immediate_action)}
        {optionalField('Location', incident.location_description)}
        {optionalField('Root Cause', incident.root_cause)}
        {optionalField('Corrective Action', incident.corrective_action)}
        {optionalField('Authority Reference', incident.authority_ref)}
      </div>

      {/* Occurrence classification and 72-hour reporting */}
      <OccurrenceReportingCard incident={incident} reporting={reporting} canEdit={canModify} />

      {/* Linked entities */}
      {(linkedAircraft || linkedPilot || incident.flight_id != null) && (
        <div style={cardStyle}>
          <div style={sectionHeading}>Linked Records</div>
          {linkedAircraft && (
            <div style={{ marginBottom: 'var(--space-4)' }}>
              <div style={fieldLabel}>Aircraft</div>
              <Link to={`/aircraft/${linkedAircraft.id}`} style={{ textDecoration: 'none' }}>
                <AircraftCell aircraft={linkedAircraft} />
              </Link>
            </div>
          )}
          {linkedPilot && (
            <Field label="Pilot" value={<Link to={`/pilots/${linkedPilot.id}`} style={linkStyle}>{linkedPilot.name}</Link>} />
          )}
          {incident.flight_id != null && (
            <Field label="Flight" value={<Link to={`/flights/${incident.flight_id}`} style={linkStyle}>Flight #{incident.flight_id}</Link>} />
          )}
        </div>
      )}

      {/* Attachments */}
      <div style={cardStyle}>
        <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: 'var(--space-4)' }}>
          <span style={{ ...sectionHeading, marginBottom: 0 }}>Attachments</span>
          {canModify && (
            <>
              <input ref={fileInputRef} type="file" accept={ATTACHMENT_ACCEPT} onChange={handleFilePick} style={{ display: 'none' }} />
              <button style={btnAccent} onClick={() => fileInputRef.current?.click()} disabled={uploadMutation.isPending}>
                {uploadMutation.isPending ? 'Uploading…' : 'Upload'}
              </button>
            </>
          )}
        </div>
        {attachments.length === 0 ? (
          <p style={{ fontSize: '0.85rem', color: 'var(--color-text-muted)' }}>No attachments.</p>
        ) : (
          <div style={{ display: 'flex', flexDirection: 'column', gap: 'var(--space-2)' }}>
            {attachments.map(att => (
              <div key={att.id} style={{ display: 'flex', alignItems: 'center', gap: 'var(--space-3)', padding: 'var(--space-2) var(--space-3)', background: 'var(--color-bg-base)', border: '1px solid var(--color-border)', borderRadius: 'var(--radius-sm)' }}>
                <div style={{ flex: 1, minWidth: 0 }}>
                  <div style={{ fontSize: '0.85rem', color: 'var(--color-text-primary)', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap' }}>{att.original_filename}</div>
                  <div style={{ fontSize: '0.72rem', color: 'var(--color-text-muted)' }}>{KIND_LABELS[att.kind] ?? att.kind} · {formatSize(att.size_bytes)}</div>
                </div>
                <button style={btnSmall} onClick={() => downloadMutation.mutate(att)}>Download</button>
                {canDeleteAttachment && (
                  <button style={btnDanger} onClick={() => handleDeleteAttachment(att)}>Delete</button>
                )}
              </div>
            ))}
          </div>
        )}
      </div>

      {/* Notes (append-only) */}
      <div style={cardStyle}>
        <div style={sectionHeading}>Notes</div>
        {notes.length === 0 ? (
          <p style={{ fontSize: '0.85rem', color: 'var(--color-text-muted)', marginBottom: canModify ? 'var(--space-4)' : 0 }}>No notes yet.</p>
        ) : (
          <div style={{ display: 'flex', flexDirection: 'column', gap: 'var(--space-3)', marginBottom: canModify ? 'var(--space-4)' : 0 }}>
            {notes.map(n => (
              <div key={n.id} style={{ padding: 'var(--space-3)', background: 'var(--color-bg-base)', border: '1px solid var(--color-border)', borderRadius: 'var(--radius-sm)' }}>
                <div style={{ ...monoStyle, color: 'var(--color-text-muted)', fontSize: '0.72rem', marginBottom: 'var(--space-1)' }}>{formatDateTime(n.created_at)}</div>
                <div style={{ fontSize: '0.88rem', color: 'var(--color-text-primary)', whiteSpace: 'pre-wrap' }}>{n.note}</div>
              </div>
            ))}
          </div>
        )}
        {canModify && (
          <div style={{ display: 'flex', flexDirection: 'column', gap: 'var(--space-2)' }}>
            <textarea
              style={{ ...inputStyle, minHeight: '70px', resize: 'vertical' }}
              value={noteDraft}
              onChange={e => setNoteDraft(e.target.value)}
              placeholder="Add a note (cannot be edited or deleted once saved)…"
            />
            <div style={{ display: 'flex', justifyContent: 'flex-end' }}>
              <button style={btnAccent} onClick={handleAddNote} disabled={noteMutation.isPending}>
                {noteMutation.isPending ? 'Adding…' : 'Add note'}
              </button>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
