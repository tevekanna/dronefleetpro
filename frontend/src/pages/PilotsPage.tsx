// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useState } from 'react';
import { Link } from 'react-router-dom';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { fetchPilots, createPilot, updatePilot, deactivatePilot } from '../api/pilots';
import type { Pilot, CreatePilotRequest } from '../api/pilots';
import { PilotStatsModal } from '../components/pilots/PilotStatsModal';
import { IsoDateInput } from '../components/inputs/IsoDateInput';
import { useCurrentUser } from '../hooks/useCurrentUser';
import { userHasAnyRole } from '../utils/auth';

// --- Styles ---
const tableStyle: React.CSSProperties = { width: '100%', borderCollapse: 'collapse', fontFamily: 'var(--font-body)', fontSize: '0.85rem' };
const thStyle: React.CSSProperties = { textAlign: 'left', padding: 'var(--space-2) var(--space-3)', color: 'var(--color-text-muted)', fontFamily: 'var(--font-label)', fontSize: '0.75rem', fontWeight: 500, textTransform: 'uppercase', letterSpacing: '0.05em', borderBottom: '1px solid var(--color-border)' };
const tdStyle: React.CSSProperties = { padding: 'var(--space-2) var(--space-3)', borderBottom: '1px solid var(--color-border)', color: 'var(--color-text-primary)' };
const monoStyle: React.CSSProperties = { fontFamily: 'var(--font-display)', fontSize: '0.8rem' };
const addBtnStyle: React.CSSProperties = { background: 'var(--color-accent)', color: 'var(--color-text-inverse)', border: 'none', borderRadius: 'var(--radius-sm)', padding: 'var(--space-2) var(--space-4)', fontSize: '0.85rem', fontWeight: 500, cursor: 'pointer' };
const btnSmall: React.CSSProperties = { background: 'transparent', color: 'var(--color-text-secondary)', border: '1px solid var(--color-border)', borderRadius: 'var(--radius-sm)', padding: 'var(--space-1) var(--space-2)', cursor: 'pointer', fontSize: '0.8rem', marginRight: 'var(--space-1)' };
const btnDanger: React.CSSProperties = { background: 'transparent', color: 'var(--color-critical)', border: '1px solid var(--color-critical)', borderRadius: 'var(--radius-sm)', padding: 'var(--space-1) var(--space-2)', cursor: 'pointer', fontSize: '0.8rem' };
const overlayStyle: React.CSSProperties = { position: 'fixed', inset: 0, background: 'rgba(0,0,0,0.7)', display: 'flex', alignItems: 'center', justifyContent: 'center', zIndex: 1000 };
const modalStyle: React.CSSProperties = { background: 'var(--color-bg-elevated)', border: '1px solid var(--color-border-strong)', borderRadius: 'var(--radius-lg)', padding: 'var(--space-6)', width: '100%', maxWidth: '520px', boxShadow: 'var(--shadow-modal)', maxHeight: '90vh', overflowY: 'auto' };
const fieldStyle: React.CSSProperties = { display: 'flex', flexDirection: 'column', gap: 'var(--space-1)', marginBottom: 'var(--space-4)' };
const labelStyle: React.CSSProperties = { fontSize: '0.8rem', color: 'var(--color-text-secondary)', fontFamily: 'var(--font-label)' };
const inputStyle: React.CSSProperties = { background: 'var(--color-bg-input)', border: '1px solid var(--color-border)', borderRadius: 'var(--radius-sm)', padding: 'var(--space-2) var(--space-3)', color: 'var(--color-text-primary)', fontSize: '0.9rem', fontFamily: 'var(--font-body)' };
const cancelBtnStyle: React.CSSProperties = { background: 'transparent', color: 'var(--color-text-secondary)', border: '1px solid var(--color-border)', borderRadius: 'var(--radius-sm)', padding: 'var(--space-2) var(--space-4)', cursor: 'pointer', fontSize: '0.85rem' };
const submitBtnStyle = (p: boolean): React.CSSProperties => ({ background: 'var(--color-accent)', color: 'var(--color-text-inverse)', border: 'none', borderRadius: 'var(--radius-sm)', padding: 'var(--space-2) var(--space-4)', cursor: 'pointer', fontSize: '0.85rem', fontWeight: 500, opacity: p ? 0.7 : 1 });

const LICENSE_TYPES = [
  { value: 'A1/A3', label: 'A1/A3 (Open Category)' },
  { value: 'A2', label: 'A2 (Open Category)' },
  { value: 'SPEC', label: 'SPEC (Specific Category)' },
];

const QUALIFICATIONS = [
  { code: 'BVLOS', label: 'BVLOS — Beyond Visual Line of Sight' },
  { code: 'NGT', label: 'NGT — Night Operations' },
  { code: 'OVER', label: 'OVER — Operations Over People' },
  { code: 'CARGO', label: 'CARGO — Cargo Transport' },
  { code: 'LOWALT', label: 'LOWALT — Low Altitude Operations' },
  { code: 'NONSEG', label: 'NONSEG — Non-Segregated Airspace' },
];

const qualBadgeStyle: React.CSSProperties = {
  display: 'inline-block',
  padding: '1px 6px',
  borderRadius: 'var(--radius-sm)',
  fontSize: '0.65rem',
  fontFamily: 'var(--font-display)',
  background: 'rgba(99,102,241,0.15)',
  color: 'var(--color-info)',
  border: '1px solid rgba(99,102,241,0.3)',
  marginRight: '3px',
  marginBottom: '2px',
};

const getDaysUntilExpiry = (dateStr: string | null): number | null => {
  if (!dateStr) return null;
  const expiry = new Date(dateStr);
  const now = new Date();
  now.setHours(0, 0, 0, 0);
  return Math.floor((expiry.getTime() - now.getTime()) / (1000 * 60 * 60 * 24));
};

function ExpiryCell({ dateStr }: { dateStr: string | null }) {
  const days = getDaysUntilExpiry(dateStr);
  if (days === null) return <span style={{ color: 'var(--color-text-muted)' }}>—</span>;
  if (days < 0) return <span style={{ color: 'var(--color-critical)', fontWeight: 500 }}>{dateStr!.substring(0, 10)} — Expired</span>;
  if (days <= 60) return <span style={{ color: 'var(--color-warning)', fontWeight: 500 }}>{dateStr!.substring(0, 10)} — Expiring soon</span>;
  return <span style={{ color: 'var(--color-ok)' }}>{dateStr!.substring(0, 10)}</span>;
}

function LicenseBadge({ type }: { type: string | null }) {
  if (!type) return <span style={{ color: 'var(--color-text-muted)' }}>—</span>;
  return (
    <span style={{ display: 'inline-block', padding: '2px 8px', borderRadius: 'var(--radius-sm)', fontSize: '0.75rem', fontFamily: 'var(--font-display)', background: 'var(--color-accent-glow)', color: 'var(--color-accent)', border: '1px solid var(--color-accent-dim)' }}>{type}</span>
  );
}

// --- Qualifications Checkboxes (shared by Add/Edit) ---
function QualificationsSection({ selected, onChange }: { selected: string[]; onChange: (q: string[]) => void }) {
  const toggle = (code: string) => {
    if (selected.includes(code)) {
      onChange(selected.filter(c => c !== code));
    } else {
      onChange([...selected, code]);
    }
  };

  return (
    <div style={{ ...fieldStyle, marginBottom: 'var(--space-3)' }}>
      <label style={{ ...labelStyle, marginBottom: 'var(--space-2)', fontWeight: 500 }}>Qualifications (SPEC only)</label>
      <div style={{ background: 'var(--color-bg-surface)', border: '1px solid var(--color-border)', borderRadius: 'var(--radius-sm)', padding: 'var(--space-3)' }}>
        {QUALIFICATIONS.map(q => (
          <div key={q.code} style={{ display: 'flex', alignItems: 'center', gap: 'var(--space-2)', marginBottom: 'var(--space-2)' }}>
            <input type="checkbox" checked={selected.includes(q.code)} onChange={() => toggle(q.code)} style={{ accentColor: 'var(--color-accent)' }} />
            <label style={{ fontSize: '0.8rem', color: 'var(--color-text-primary)', cursor: 'pointer' }} onClick={() => toggle(q.code)}>{q.label}</label>
          </div>
        ))}
      </div>
    </div>
  );
}

// --- Pilot Table ---
function PilotTable({ onAdd, onEdit, onStats }: { onAdd: () => void; onEdit: (p: Pilot) => void; onStats: (p: Pilot) => void }) {
  const queryClient = useQueryClient();
  const { data: currentUser } = useCurrentUser();
  const isAdmin = userHasAnyRole(currentUser, 'admin');

  const { data: pilots, isLoading, error } = useQuery({ queryKey: ['pilots'], queryFn: fetchPilots });
  const deactivateMutation = useMutation({
    mutationFn: deactivatePilot,
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ['pilots'] }),
  });

  const handleDeactivate = (id: number, name: string) => {
    if (window.confirm(`Deactivate pilot ${name}?`)) deactivateMutation.mutate(id);
  };

  if (isLoading) return <p style={{ color: 'var(--color-text-secondary)' }}>Loading pilots...</p>;
  if (error) return <p style={{ color: 'var(--color-critical)' }}>Failed to load: {(error as Error).message}</p>;

  return (
    <div>
      <div style={{ display: 'flex', justifyContent: 'flex-end', marginBottom: 'var(--space-4)' }}>
        {isAdmin && <button onClick={onAdd} style={addBtnStyle}>Add Pilot</button>}
      </div>
      {!pilots || pilots.length === 0 ? (
        <div style={{ display: 'flex', flexDirection: 'column', alignItems: 'center', justifyContent: 'center', padding: 'var(--space-10) var(--space-5)', background: 'var(--color-bg-surface)', border: '1px dashed var(--color-border)', borderRadius: 'var(--radius-md)' }}>
          <p style={{ fontFamily: 'var(--font-display)', fontSize: '1rem', color: 'var(--color-text-secondary)', marginBottom: 'var(--space-2)' }}>No pilots registered yet</p>
          <p style={{ fontSize: '0.85rem', color: 'var(--color-text-muted)' }}>Add pilots to assign them as PIC on flight records.</p>
        </div>
      ) : (
        <table style={tableStyle}>
          <thead><tr>
            <th style={thStyle}>Name</th>
            <th style={thStyle}>License</th>
            <th style={thStyle}>Number</th>
            <th style={thStyle}>Expires</th>
            <th style={thStyle}>Qualifications</th>
            <th style={thStyle}>Status</th>
            <th style={thStyle}>Actions</th>
          </tr></thead>
          <tbody>
            {pilots.map((p: Pilot) => (
              <tr key={p.id}>
                <td style={{ ...tdStyle, fontWeight: 500 }}>
                  <Link
                    to={`/pilots/${p.id}`}
                    style={{ color: 'var(--color-text-primary)', textDecoration: 'none' }}
                    onMouseEnter={e => { (e.currentTarget as HTMLAnchorElement).style.color = 'var(--color-accent)'; }}
                    onMouseLeave={e => { (e.currentTarget as HTMLAnchorElement).style.color = 'var(--color-text-primary)'; }}
                  >
                    {p.name}
                  </Link>
                </td>
                <td style={tdStyle}><LicenseBadge type={p.license_type} /></td>
                <td style={{ ...tdStyle, ...monoStyle }}>{p.license_number ?? '—'}</td>
                <td style={tdStyle}><ExpiryCell dateStr={p.license_expires} /></td>
                <td style={tdStyle}>
                  {p.qualifications && p.qualifications.length > 0
                    ? p.qualifications.map(q => <span key={q.id} style={qualBadgeStyle}>{q.type_name}</span>)
                    : <span style={{ color: 'var(--color-text-muted)' }}>—</span>
                  }
                </td>
                <td style={tdStyle}>
                  <span style={{ color: p.is_active ? 'var(--color-ok)' : 'var(--color-text-muted)', fontWeight: 500 }}>
                    {p.is_active ? 'Active' : 'Inactive'}
                  </span>
                </td>
                <td style={tdStyle}>
                  <button style={btnSmall} onClick={() => onStats(p)}>Stats</button>
                  {p.is_active && isAdmin && (
                    <>
                      <button style={btnSmall} onClick={() => onEdit(p)}>Edit</button>
                      <button style={btnDanger} onClick={() => handleDeactivate(p.id, p.name)}>Deactivate</button>
                    </>
                  )}
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </div>
  );
}

// --- Add Pilot Modal ---
function AddPilotModal({ onClose }: { onClose: () => void }) {
  const queryClient = useQueryClient();
  const [name, setName] = useState('');
  const [email, setEmail] = useState('');
  const [phone, setPhone] = useState('');
  const [licenseType, setLicenseType] = useState('');
  const [licenseNumber, setLicenseNumber] = useState('');
  const [licenseExpires, setLicenseExpires] = useState('');
  const [notes, setNotes] = useState('');
  const [selectedQuals, setSelectedQuals] = useState<string[]>([]);
  const [error, setError] = useState('');

  const mutation = useMutation({
    mutationFn: (data: CreatePilotRequest) => createPilot(data),
    onSuccess: () => { queryClient.invalidateQueries({ queryKey: ['pilots'] }); onClose(); },
    onError: (err: Error) => setError(err.message),
  });

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (!name.trim()) { setError('Name is required'); return; }
    setError('');
    mutation.mutate({
      name: name.trim(),
      email: email.trim() || undefined,
      phone: phone.trim() || undefined,
      license_type: licenseType || undefined,
      license_number: licenseNumber.trim() || undefined,
      license_expires: licenseExpires || undefined,
      notes: notes.trim() || undefined,
      qualifications: licenseType === 'SPEC' && selectedQuals.length > 0 ? selectedQuals : undefined,
    });
  };

  return (
    <div style={overlayStyle} onClick={onClose}>
      <div style={modalStyle} onClick={e => e.stopPropagation()}>
        <h2 style={{ fontFamily: 'var(--font-display)', fontSize: '1.1rem', color: 'var(--color-text-primary)', marginBottom: 'var(--space-5)' }}>Add Pilot</h2>
        <form onSubmit={handleSubmit}>
          <div style={fieldStyle}><label style={labelStyle}>Full Name *</label><input style={inputStyle} value={name} onChange={e => setName(e.target.value)} placeholder="e.g. Alex Example" required /></div>
          <div style={{ display: 'flex', gap: 'var(--space-4)' }}>
            <div style={{ ...fieldStyle, flex: 1 }}><label style={labelStyle}>Email</label><input style={inputStyle} type="email" value={email} onChange={e => setEmail(e.target.value)} placeholder="alex@example.com" /></div>
            <div style={{ ...fieldStyle, flex: 1 }}><label style={labelStyle}>Phone</label><input style={inputStyle} value={phone} onChange={e => setPhone(e.target.value)} placeholder="+46 70 123 4567" /></div>
          </div>
          <div style={{ display: 'flex', gap: 'var(--space-4)' }}>
            <div style={{ ...fieldStyle, flex: 1 }}>
              <label style={labelStyle}>License Type</label>
              <select style={inputStyle} value={licenseType} onChange={e => { setLicenseType(e.target.value); if (e.target.value !== 'SPEC') setSelectedQuals([]); }}>
                <option value="">— Select —</option>
                {LICENSE_TYPES.map(t => <option key={t.value} value={t.value}>{t.label}</option>)}
              </select>
            </div>
            <div style={{ ...fieldStyle, flex: 1 }}><label style={labelStyle}>License Number</label><input style={inputStyle} value={licenseNumber} onChange={e => setLicenseNumber(e.target.value)} placeholder="e.g. SE-UAS-12345" /></div>
          </div>
          <div style={fieldStyle}><label style={labelStyle}>License Expires</label><IsoDateInput style={inputStyle} value={licenseExpires} onChange={e => setLicenseExpires(e.target.value)} /></div>
          {licenseType === 'SPEC' && <QualificationsSection selected={selectedQuals} onChange={setSelectedQuals} />}
          <div style={fieldStyle}><label style={labelStyle}>Notes</label><textarea style={{ ...inputStyle, minHeight: '50px', resize: 'vertical' }} value={notes} onChange={e => setNotes(e.target.value)} placeholder="Optional notes..." /></div>
          {error && <p style={{ color: 'var(--color-critical)', fontSize: '0.8rem', marginBottom: 'var(--space-3)' }}>{error}</p>}
          <div style={{ display: 'flex', gap: 'var(--space-3)', justifyContent: 'flex-end' }}>
            <button type="button" onClick={onClose} style={cancelBtnStyle}>Cancel</button>
            <button type="submit" disabled={mutation.isPending} style={submitBtnStyle(mutation.isPending)}>{mutation.isPending ? 'Saving...' : 'Add Pilot'}</button>
          </div>
        </form>
      </div>
    </div>
  );
}

// --- Edit Pilot Modal ---
function EditPilotModal({ pilot, onClose }: { pilot: Pilot; onClose: () => void }) {
  const queryClient = useQueryClient();
  const [name, setName] = useState(pilot.name);
  const [email, setEmail] = useState(pilot.email ?? '');
  const [phone, setPhone] = useState(pilot.phone ?? '');
  const [licenseType, setLicenseType] = useState(pilot.license_type ?? '');
  const [licenseNumber, setLicenseNumber] = useState(pilot.license_number ?? '');
  const [licenseExpires, setLicenseExpires] = useState(pilot.license_expires?.substring(0, 10) ?? '');
  const [notes, setNotes] = useState(pilot.notes ?? '');
  const [selectedQuals, setSelectedQuals] = useState<string[]>(
    pilot.qualifications?.map(q => q.type_name) ?? []
  );
  const [error, setError] = useState('');

  const mutation = useMutation({
    mutationFn: (data: Partial<CreatePilotRequest>) => updatePilot(pilot.id, data),
    onSuccess: () => { queryClient.invalidateQueries({ queryKey: ['pilots'] }); onClose(); },
    onError: (err: Error) => setError(err.message),
  });

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (!name.trim()) { setError('Name is required'); return; }
    setError('');
    mutation.mutate({
      name: name.trim(),
      email: email.trim() || undefined,
      phone: phone.trim() || undefined,
      license_type: licenseType || undefined,
      license_number: licenseNumber.trim() || undefined,
      license_expires: licenseExpires || undefined,
      notes: notes.trim() || undefined,
      qualifications: licenseType === 'SPEC' ? selectedQuals : [],
    });
  };

  return (
    <div style={overlayStyle} onClick={onClose}>
      <div style={modalStyle} onClick={e => e.stopPropagation()}>
        <h2 style={{ fontFamily: 'var(--font-display)', fontSize: '1.1rem', color: 'var(--color-text-primary)', marginBottom: 'var(--space-5)' }}>Edit Pilot</h2>
        <form onSubmit={handleSubmit}>
          <div style={fieldStyle}><label style={labelStyle}>Full Name *</label><input style={inputStyle} value={name} onChange={e => setName(e.target.value)} required /></div>
          <div style={{ display: 'flex', gap: 'var(--space-4)' }}>
            <div style={{ ...fieldStyle, flex: 1 }}><label style={labelStyle}>Email</label><input style={inputStyle} type="email" value={email} onChange={e => setEmail(e.target.value)} /></div>
            <div style={{ ...fieldStyle, flex: 1 }}><label style={labelStyle}>Phone</label><input style={inputStyle} value={phone} onChange={e => setPhone(e.target.value)} /></div>
          </div>
          <div style={{ display: 'flex', gap: 'var(--space-4)' }}>
            <div style={{ ...fieldStyle, flex: 1 }}>
              <label style={labelStyle}>License Type</label>
              <select style={inputStyle} value={licenseType} onChange={e => { setLicenseType(e.target.value); if (e.target.value !== 'SPEC') setSelectedQuals([]); }}>
                <option value="">— Select —</option>
                {LICENSE_TYPES.map(t => <option key={t.value} value={t.value}>{t.label}</option>)}
              </select>
            </div>
            <div style={{ ...fieldStyle, flex: 1 }}><label style={labelStyle}>License Number</label><input style={inputStyle} value={licenseNumber} onChange={e => setLicenseNumber(e.target.value)} /></div>
          </div>
          <div style={fieldStyle}><label style={labelStyle}>License Expires</label><IsoDateInput style={inputStyle} value={licenseExpires} onChange={e => setLicenseExpires(e.target.value)} /></div>
          {licenseType === 'SPEC' && <QualificationsSection selected={selectedQuals} onChange={setSelectedQuals} />}
          <div style={fieldStyle}><label style={labelStyle}>Notes</label><textarea style={{ ...inputStyle, minHeight: '50px', resize: 'vertical' }} value={notes} onChange={e => setNotes(e.target.value)} /></div>
          {error && <p style={{ color: 'var(--color-critical)', fontSize: '0.8rem', marginBottom: 'var(--space-3)' }}>{error}</p>}
          <div style={{ display: 'flex', gap: 'var(--space-3)', justifyContent: 'flex-end' }}>
            <button type="button" onClick={onClose} style={cancelBtnStyle}>Cancel</button>
            <button type="submit" disabled={mutation.isPending} style={submitBtnStyle(mutation.isPending)}>{mutation.isPending ? 'Saving...' : 'Save Changes'}</button>
          </div>
        </form>
      </div>
    </div>
  );
}

// --- Main Pilots Page ---
export function PilotsPage() {
  const [showAddPilot, setShowAddPilot] = useState(false);
  const [editPilot, setEditPilot] = useState<Pilot | null>(null);
  const [statsPilot, setStatsPilot] = useState<Pilot | null>(null);

  return (
    <div>
      <h1 style={{ fontFamily: 'var(--font-display)', fontSize: '1.25rem', fontWeight: 500, color: 'var(--color-text-primary)', letterSpacing: '0.02em', marginBottom: 'var(--space-5)' }}>
        Pilots
      </h1>
      <PilotTable
        onAdd={() => setShowAddPilot(true)}
        onEdit={p => setEditPilot(p)}
        onStats={p => setStatsPilot(p)}
      />
      {showAddPilot && <AddPilotModal onClose={() => setShowAddPilot(false)} />}
      {editPilot && <EditPilotModal pilot={editPilot} onClose={() => setEditPilot(null)} />}
      {statsPilot && <PilotStatsModal pilotId={statsPilot.id} onClose={() => setStatsPilot(null)} />}
    </div>
  );
}
