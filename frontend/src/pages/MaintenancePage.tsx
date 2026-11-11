// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useCallback, useEffect, useState } from 'react';
import { useNavigate, useSearchParams } from 'react-router-dom';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { Plane, Gamepad2 } from 'lucide-react';
import {
  fetchMaintenance, deleteMaintenance,
  fetchDefects, createDefect, closeDefect, deleteDefect,
} from '../api/maintenance';
import type { MaintenanceRecord, DefectRecord } from '../api/maintenance';
import { fetchAircraft } from '../api/aircraft';
import type { Aircraft } from '../types/aircraft';
import { fetchFlights } from '../api/flights';
import type { Flight } from '../types/flight';
import { formatDuration } from '../components/flight-detail/flightFormatters';
import { IsoDateInput } from '../components/inputs/IsoDateInput';
import { formatAircraftLabel } from '../utils/aircraftFormat';
import { AddMaintenanceModal } from '../components/maintenance/AddMaintenanceModal';
import { MaintenanceSubNav } from '../components/maintenance/MaintenanceSubNav';

type Tab = 'maintenance' | 'defects';

const VALID_TABS: Tab[] = ['maintenance', 'defects'];

function parseTab(raw: string | null): Tab {
  if (raw && (VALID_TABS as string[]).includes(raw)) {
    return raw as Tab;
  }
  return 'maintenance';
}

const today = new Date().toISOString().slice(0, 10);

// --- Shared styles ---
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

function truncate(s: string | null, len: number): string {
  if (!s) return '—';
  return s.length > len ? s.substring(0, len) + '...' : s;
}

function categoryBadge(cat: string | null): React.ReactNode {
  if (!cat) return '—';
  const color = cat === 'Scheduled' ? 'var(--color-info)' : 'var(--color-warning)';
  return <span style={{ color, fontWeight: 500 }}>{cat}</span>;
}

function statusBadge(status: string): React.ReactNode {
  const color = status === 'open' ? 'var(--color-critical)' : status === 'closed' ? 'var(--color-ok)' : 'var(--color-warning)';
  return <span style={{ color, fontWeight: 500 }}>{status}</span>;
}

function TargetCell({ record }: { record: MaintenanceRecord }): React.ReactElement {
  const iconStyle: React.CSSProperties = {
    flexShrink: 0,
    color: 'var(--color-text-muted)',
  };
  const wrap: React.CSSProperties = {
    display: 'inline-flex',
    alignItems: 'center',
    gap: 'var(--space-2)',
  };
  if (record.controller_id) {
    return (
      <span style={wrap} title="Controller">
        <Gamepad2 size={14} style={iconStyle} />
        <span>{record.controller_name ?? `Controller #${record.controller_id}`}</span>
      </span>
    );
  }
  if (record.aircraft_id) {
    return (
      <span style={wrap} title="Aircraft">
        <Plane size={14} style={iconStyle} />
        <span>{record.aircraft_name ?? `Aircraft #${record.aircraft_id}`}</span>
      </span>
    );
  }
  // Audit-trail edge case: both target FKs nulled by ON DELETE SET NULL on
  // retired equipment. The handler enforces "exactly one" on insert, so this
  // only happens to historical rows.
  return (
    <span style={{ ...wrap, color: 'var(--color-text-muted)' }} title="Target retired">
      <span>—</span>
    </span>
  );
}

// --- Empty State ---
function EmptyState({ message, hint }: { message: string; hint: string }) {
  return (
    <div style={{ display: 'flex', flexDirection: 'column', alignItems: 'center', justifyContent: 'center', padding: 'var(--space-10) var(--space-5)', background: 'var(--color-bg-surface)', border: '1px dashed var(--color-border)', borderRadius: 'var(--radius-md)' }}>
      <p style={{ fontFamily: 'var(--font-display)', fontSize: '1rem', color: 'var(--color-text-secondary)', marginBottom: 'var(--space-2)' }}>{message}</p>
      <p style={{ fontSize: '0.85rem', color: 'var(--color-text-muted)' }}>{hint}</p>
    </div>
  );
}

// --- Maintenance Log Tab ---
function MaintenanceTab({ onAdd, aircraftId }: { onAdd: () => void; aircraftId?: number }) {
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  const { data: records, isLoading, error } = useQuery({
    queryKey: ['maintenance', aircraftId ?? null],
    queryFn: () => fetchMaintenance(aircraftId != null ? { aircraft_id: aircraftId } : {}),
  });
  const delMutation = useMutation({
    mutationFn: deleteMaintenance,
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['maintenance'] }),
  });

  const handleDelete = (id: number) => {
    if (window.confirm('Delete this maintenance record?')) delMutation.mutate(id);
  };

  if (isLoading) return <p style={{ color: 'var(--color-text-secondary)' }}>Loading maintenance records...</p>;
  if (error) return <p style={{ color: 'var(--color-critical)' }}>Failed to load: {(error as Error).message}</p>;

  return (
    <div>
      <div style={{ display: 'flex', justifyContent: 'flex-end', marginBottom: 'var(--space-4)' }}>
        <button onClick={onAdd} style={addBtnStyle}>Add Maintenance Record</button>
      </div>
      {!records || records.length === 0 ? (
        <EmptyState message="No maintenance records yet." hint="Create your first EASA-compliant maintenance record." />
      ) : (
        <table style={tableStyle}>
          <thead><tr>
            <th style={thStyle}>Date</th>
            <th style={thStyle}>Target</th>
            <th style={thStyle}>Category</th>
            <th style={thStyle}>Type</th>
            <th style={thStyle}>Created By</th>
            <th style={thStyle}>Remark</th>
            <th style={thStyle}>Sign Off</th>
            <th style={thStyle}>Actions</th>
          </tr></thead>
          <tbody>
            {records.map((r: MaintenanceRecord) => {
              return (
                <tr
                  key={r.id}
                  onClick={() => navigate(`/maintenance/${r.id}`)}
                  style={{ cursor: 'pointer' }}
                >
                  <td style={{ ...tdStyle, ...monoStyle }}>{r.performed_date.substring(0, 10)}</td>
                  <td style={{ ...tdStyle, whiteSpace: 'nowrap' }}>
                    <TargetCell record={r} />
                  </td>
                  <td style={tdStyle}>{categoryBadge(r.maint_category)}</td>
                  <td style={tdStyle}>{r.maint_type ?? '—'}</td>
                  <td style={{ ...tdStyle, ...monoStyle }}>{r.created_by}</td>
                  <td style={tdStyle}>{truncate(r.remark, 40)}</td>
                  <td style={{ ...tdStyle, ...monoStyle }}>{r.sign_off}</td>
                  <td style={tdStyle}>
                    <button
                      style={btnDanger}
                      onClick={(e) => {
                        e.stopPropagation();
                        handleDelete(r.id);
                      }}
                    >
                      Delete
                    </button>
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

// --- Defect Log Tab ---
function DefectTab({ onAdd, onClose, aircraft, aircraftId }: { onAdd: () => void; onClose: (d: DefectRecord) => void; aircraft: Aircraft[]; aircraftId?: number }) {
  const queryClient = useQueryClient();
  // fetchDefects now accepts an optional filters arg; wrap so TanStack
  // Query doesn't pass its context object as the first parameter.
  const { data: defects, isLoading, error } = useQuery({
    queryKey: ['defects', aircraftId ?? null],
    queryFn: () => fetchDefects(aircraftId != null ? { aircraft_id: aircraftId } : {}),
  });
  const delMutation = useMutation({
    mutationFn: deleteDefect,
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['defects'] }),
  });

  const handleDelete = (id: number) => {
    if (window.confirm('Delete this defect record?')) delMutation.mutate(id);
  };

  const findAircraft = (id: number | null) => aircraft.find(a => a.id === id);

  if (isLoading) return <p style={{ color: 'var(--color-text-secondary)' }}>Loading defects...</p>;
  if (error) return <p style={{ color: 'var(--color-critical)' }}>Failed to load: {(error as Error).message}</p>;

  return (
    <div>
      <div style={{ display: 'flex', justifyContent: 'flex-end', marginBottom: 'var(--space-4)' }}>
        <button onClick={onAdd} style={addBtnStyle}>Report Defect</button>
      </div>
      {!defects || defects.length === 0 ? (
        <EmptyState message="No defects reported." hint="Report equipment defects here for tracking and resolution." />
      ) : (
        <table style={tableStyle}>
          <thead><tr>
            <th style={thStyle}>Date</th>
            <th style={thStyle}>Aircraft</th>
            <th style={thStyle}>Description</th>
            <th style={thStyle}>Status</th>
            <th style={thStyle}>Reported By</th>
            <th style={thStyle}>Actions</th>
          </tr></thead>
          <tbody>
            {defects.map((d: DefectRecord) => {
              const ac = findAircraft(d.aircraft_id);
              return (
                <tr key={d.id}>
                  <td style={{ ...tdStyle, ...monoStyle }}>{d.defect_date.substring(0, 10)}</td>
                  <td style={tdStyle}>{ac ? formatAircraftLabel(ac) : d.aircraft_id ? `Aircraft #${d.aircraft_id}` : '—'}</td>
                  <td style={tdStyle}>{truncate(d.description, 50)}</td>
                  <td style={tdStyle}>{statusBadge(d.status)}</td>
                  <td style={{ ...tdStyle, ...monoStyle }}>{d.reported_by}</td>
                  <td style={tdStyle}>
                    {d.status !== 'closed' && (
                      <button style={{ ...btnSmall, color: 'var(--color-ok)', borderColor: 'var(--color-ok)' }} onClick={() => onClose(d)}>Close</button>
                    )}
                    <button style={btnDanger} onClick={() => handleDelete(d.id)}>Delete</button>
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

function flightPickerLabel(f: Flight): string {
  const dt = f.flight_datetime ?? '';
  const date = dt ? dt.substring(0, 10) : '—';
  const time = dt ? dt.substring(11, 16) : '';
  const duration = formatDuration(f.duration_seconds);
  // Prefer pic_name (actual pilot) → operation_name → mission_type → '—'
  const context = f.pic_name || f.operation_name || f.mission_type || '—';
  const datetime = time ? `${date} ${time}` : date;
  return `${datetime} · ${duration} · ${context}`;
}

// AddMaintenanceModal lives in components/maintenance/AddMaintenanceModal.tsx
// so it can be reused by ControllerStatsPage. This page passes
// target={{ kind: 'aircraft', aircraft }} below.

// --- Add Defect Modal ---
function AddDefectModal({ onClose, aircraft }: { onClose: () => void; aircraft: Aircraft[] }) {
  const queryClient = useQueryClient();
  const [aircraftId, setAircraftId] = useState('');
  const [defectDate, setDefectDate] = useState(today);
  const [flightId, setFlightId] = useState<string>('');
  const [reportedBy, setReportedBy] = useState('');
  const [description, setDescription] = useState('');
  const [error, setError] = useState('');

  useEffect(() => {
    setFlightId('');
  }, [aircraftId]);

  const mutation = useMutation({
    mutationFn: (data: object) => createDefect(data),
    onSuccess: () => { queryClient.invalidateQueries({ queryKey: ['defects'] }); onClose(); },
    onError: (err: Error) => setError(err.message),
  });

  const flightsQuery = useQuery({
    queryKey: ['flights', 'for-aircraft', aircraftId],
    queryFn: () => fetchFlights({ aircraftId: Number(aircraftId), perPage: 50 }),
    enabled: !!aircraftId,
    staleTime: 60_000,
  });

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (!aircraftId) { setError('Aircraft is required'); return; }
    if (!reportedBy || reportedBy.length !== 3) { setError('Reported By must be exactly 3 uppercase letters'); return; }
    if (description.trim().length < 5) { setError('Description must be at least 5 characters'); return; }
    setError('');
    mutation.mutate({
      aircraft_id: Number(aircraftId),
      defect_date: defectDate,
      flight_id: flightId ? Number(flightId) : null,
      reported_by: reportedBy.toUpperCase(),
      description: description.trim(),
    });
  };

  return (
    <div style={overlayStyle} onClick={onClose}>
      <div style={modalStyle} onClick={e => e.stopPropagation()}>
        <h2 style={{ fontFamily: 'var(--font-display)', fontSize: '1.1rem', color: 'var(--color-text-primary)', marginBottom: 'var(--space-5)' }}>Report Defect</h2>
        <form onSubmit={handleSubmit}>
          <div style={{ display: 'flex', gap: 'var(--space-4)' }}>
            <div style={{ ...fieldStyle, flex: 1 }}>
              <label style={labelStyle}>Aircraft *</label>
              <select style={inputStyle} value={aircraftId} onChange={e => setAircraftId(e.target.value)} required>
                <option value="">— Select —</option>
                {aircraft.map(a => <option key={a.id} value={a.id}>{formatAircraftLabel(a)}</option>)}
              </select>
            </div>
            <div style={{ ...fieldStyle, flex: 1 }}>
              <label style={labelStyle}>Date *</label>
              <IsoDateInput style={inputStyle} value={defectDate} onChange={e => setDefectDate(e.target.value)} required />
            </div>
          </div>
          <div style={fieldStyle}>
            <label style={labelStyle}>Flight (optional)</label>
            <select
              style={inputStyle}
              value={flightId}
              onChange={e => setFlightId(e.target.value)}
              disabled={!aircraftId}
            >
              <option value="">— No specific flight —</option>
              {flightsQuery.data?.flights?.map(item => (
                <option key={item.id} value={item.id}>
                  {flightPickerLabel(item)}
                </option>
              ))}
            </select>
            {aircraftId && flightsQuery.isLoading && (
              <small style={{ color: 'var(--color-text-muted)', display: 'block', marginTop: 'var(--space-1)' }}>
                Loading flights…
              </small>
            )}
            {aircraftId && flightsQuery.isError && (
              <small style={{ color: 'var(--color-critical)', display: 'block', marginTop: 'var(--space-1)' }}>
                Failed to load flights
              </small>
            )}
            {aircraftId && !flightsQuery.isLoading && flightsQuery.data && flightsQuery.data.flights.length === 0 && (
              <small style={{ color: 'var(--color-text-muted)', display: 'block', marginTop: 'var(--space-1)' }}>
                No flights yet for this aircraft
              </small>
            )}
          </div>
          <div style={fieldStyle}>
            <label style={labelStyle}>Reported By * (3-letter code)</label>
            <input style={{ ...inputStyle, ...monoStyle, textTransform: 'uppercase' }} value={reportedBy} onChange={e => setReportedBy(e.target.value.toUpperCase().slice(0, 3))} maxLength={3} placeholder="e.g. LAR" required />
          </div>
          <div style={fieldStyle}>
            <label style={labelStyle}>Description *</label>
            <textarea style={{ ...inputStyle, minHeight: '80px', resize: 'vertical' }} value={description} onChange={e => setDescription(e.target.value)} placeholder="Describe the defect in detail..." required />
          </div>
          {error && <p style={{ color: 'var(--color-critical)', fontSize: '0.8rem', marginBottom: 'var(--space-3)' }}>{error}</p>}
          <div style={{ display: 'flex', gap: 'var(--space-3)', justifyContent: 'flex-end' }}>
            <button type="button" onClick={onClose} style={cancelBtnStyle}>Cancel</button>
            <button type="submit" disabled={mutation.isPending} style={submitBtnStyle(mutation.isPending)}>{mutation.isPending ? 'Saving...' : 'Report Defect'}</button>
          </div>
        </form>
      </div>
    </div>
  );
}

// --- Close Defect Modal ---
function CloseDefectModal({ defect, onClose }: { defect: DefectRecord; onClose: () => void }) {
  const queryClient = useQueryClient();
  const [closedBy, setClosedBy] = useState('');
  const [actionTaken, setActionTaken] = useState('');
  const [error, setError] = useState('');

  const mutation = useMutation({
    mutationFn: () => closeDefect(defect.id, closedBy.toUpperCase(), actionTaken.trim()),
    onSuccess: () => { queryClient.invalidateQueries({ queryKey: ['defects'] }); onClose(); },
    onError: (err: Error) => setError(err.message),
  });

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (!closedBy || closedBy.length !== 3) { setError('Closed By must be exactly 3 uppercase letters'); return; }
    if (actionTaken.trim().length < 5) { setError('Action Taken must be at least 5 characters'); return; }
    setError('');
    mutation.mutate();
  };

  return (
    <div style={overlayStyle} onClick={onClose}>
      <div style={{ ...modalStyle, maxWidth: '420px' }} onClick={e => e.stopPropagation()}>
        <h2 style={{ fontFamily: 'var(--font-display)', fontSize: '1.1rem', color: 'var(--color-text-primary)', marginBottom: 'var(--space-2)' }}>Close Defect</h2>
        <p style={{ fontSize: '0.8rem', color: 'var(--color-text-secondary)', marginBottom: 'var(--space-5)' }}>
          {truncate(defect.description, 80)}
        </p>
        <form onSubmit={handleSubmit}>
          <div style={fieldStyle}>
            <label style={labelStyle}>Action Taken *</label>
            <textarea style={{ ...inputStyle, minHeight: '80px', resize: 'vertical' }} value={actionTaken} onChange={e => setActionTaken(e.target.value)} placeholder="Describe what was done to resolve the defect..." required />
          </div>
          <div style={fieldStyle}>
            <label style={labelStyle}>Closed By * (3-letter code)</label>
            <input style={{ ...inputStyle, ...monoStyle, textTransform: 'uppercase' }} value={closedBy} onChange={e => setClosedBy(e.target.value.toUpperCase().slice(0, 3))} maxLength={3} placeholder="e.g. LAR" required />
          </div>
          {error && <p style={{ color: 'var(--color-critical)', fontSize: '0.8rem', marginBottom: 'var(--space-3)' }}>{error}</p>}
          <div style={{ display: 'flex', gap: 'var(--space-3)', justifyContent: 'flex-end' }}>
            <button type="button" onClick={onClose} style={cancelBtnStyle}>Cancel</button>
            <button type="submit" disabled={mutation.isPending} style={submitBtnStyle(mutation.isPending)}>{mutation.isPending ? 'Closing...' : 'Close Defect'}</button>
          </div>
        </form>
      </div>
    </div>
  );
}

// --- Main Maintenance Page ---
export function MaintenancePage() {
  const [searchParams, setSearchParams] = useSearchParams();
  const activeTab = parseTab(searchParams.get('tab'));
  const aircraftParam = searchParams.get('aircraft');
  const scopedAircraftId = aircraftParam ? Number(aircraftParam) : undefined;

  const setActiveTab = useCallback(
    (id: Tab) => {
      const next = new URLSearchParams(searchParams);
      if (id === 'maintenance') {
        next.delete('tab');
      } else {
        next.set('tab', id);
      }
      setSearchParams(next, { replace: false });
    },
    [searchParams, setSearchParams],
  );

  const [showAddMaint, setShowAddMaint] = useState(false);
  const [showAddDefect, setShowAddDefect] = useState(false);
  const [closingDefect, setClosingDefect] = useState<DefectRecord | null>(null);

  // TODO: queryKey intent says active-only but we pass include_retired=true
  // to preserve historical behavior. Per-site audit needed: should this dropdown
  // include retired items (for historical records) or active only (for new entries)?
  // Tracked separately. Reverting to active-only would change UX and may hide
  // aircraft from edit forms of historical records.
  const { data: aircraft } = useQuery({ queryKey: ['aircraft', true], queryFn: () => fetchAircraft(true) });
  const aircraftList = aircraft ?? [];

  const tabStyle = (tab: Tab): React.CSSProperties => ({
    background: 'transparent',
    border: 'none',
    borderBottom: activeTab === tab ? '2px solid var(--color-accent)' : '2px solid transparent',
    color: activeTab === tab ? 'var(--color-accent)' : 'var(--color-text-secondary)',
    padding: 'var(--space-2) var(--space-4)',
    cursor: 'pointer',
    fontSize: '0.9rem',
    fontWeight: activeTab === tab ? 600 : 400,
    fontFamily: 'var(--font-body)',
    transition: 'color 0.15s, border-color 0.15s',
  });

  const scopedAircraft =
    scopedAircraftId != null
      ? aircraftList.find((a) => a.id === scopedAircraftId)
      : undefined;

  const clearScope = () => {
    const next = new URLSearchParams(searchParams);
    next.delete('aircraft');
    setSearchParams(next, { replace: true });
  };

  return (
    <div>
      <MaintenanceSubNav />
      <div style={{ display: 'flex', alignItems: 'center', gap: 'var(--space-3)', marginBottom: 'var(--space-4)' }}>
        <h1 style={{ fontFamily: 'var(--font-display)', fontSize: '1.25rem', fontWeight: 500, color: 'var(--color-text-primary)', letterSpacing: '0.02em', margin: 0 }}>
          Maintenance
        </h1>
        {scopedAircraftId != null && (
          <span
            style={{
              display: 'inline-flex',
              alignItems: 'center',
              gap: 'var(--space-2)',
              background: 'var(--color-bg-surface)',
              border: '1px solid var(--color-border)',
              borderRadius: 'var(--radius-sm)',
              padding: 'var(--space-1) var(--space-3)',
              fontSize: '0.78rem',
              color: 'var(--color-text-secondary)',
            }}
          >
            Showing:{' '}
            <strong style={{ color: 'var(--color-text-primary)', fontWeight: 500 }}>
              {scopedAircraft ? formatAircraftLabel(scopedAircraft) : `Aircraft #${scopedAircraftId}`}
            </strong>
            <button
              type="button"
              onClick={clearScope}
              title="Clear aircraft filter"
              style={{
                background: 'transparent',
                border: 'none',
                color: 'var(--color-text-muted)',
                cursor: 'pointer',
                padding: 0,
                lineHeight: 1,
                fontSize: '0.9rem',
              }}
            >
              ✕
            </button>
          </span>
        )}
      </div>

      <div style={{ display: 'flex', gap: 'var(--space-1)', borderBottom: '1px solid var(--color-border)', marginBottom: 'var(--space-5)' }}>
        <button style={tabStyle('maintenance')} onClick={() => setActiveTab('maintenance')}>Maintenance Log</button>
        <button style={tabStyle('defects')} onClick={() => setActiveTab('defects')}>Defect Log</button>
      </div>

      {activeTab === 'maintenance' && <MaintenanceTab onAdd={() => setShowAddMaint(true)} aircraftId={scopedAircraftId} />}
      {activeTab === 'defects' && <DefectTab onAdd={() => setShowAddDefect(true)} onClose={(d) => setClosingDefect(d)} aircraft={aircraftList} aircraftId={scopedAircraftId} />}

      {showAddMaint && (
        <AddMaintenanceModal
          target={{ kind: 'aircraft', aircraft: aircraftList }}
          onClose={() => setShowAddMaint(false)}
        />
      )}
      {showAddDefect && <AddDefectModal onClose={() => setShowAddDefect(false)} aircraft={aircraftList} />}
      {closingDefect && <CloseDefectModal defect={closingDefect} onClose={() => setClosingDefect(null)} />}
    </div>
  );
}
