// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { fetchAircraft } from '../api/aircraft';
import { fetchPilots } from '../api/pilots';
import type { Aircraft } from '../types/aircraft';
import type { Pilot } from '../api/pilots';
import { useNavigate } from 'react-router-dom';
import { FileText, Wrench, Users, Boxes, Archive, QrCode } from 'lucide-react';
import { PdfViewer } from '../components/PdfViewer';
import { formatAircraftLabel } from '../utils/aircraftFormat';

type ViewerState = { url: string; filename: string } | null;

// --- Styles ---
const cardStyle: React.CSSProperties = {
  background: 'var(--color-bg-surface)',
  border: '1px solid var(--color-border)',
  borderRadius: 'var(--radius-md)',
  padding: 'var(--space-5)',
  transition: 'border-color 0.15s',
};
const cardTitle: React.CSSProperties = {
  fontFamily: 'var(--font-display)',
  fontSize: '1rem',
  fontWeight: 500,
  color: 'var(--color-text-primary)',
  display: 'flex',
  alignItems: 'center',
  gap: 'var(--space-2)',
  marginBottom: 'var(--space-2)',
};
const cardDesc: React.CSSProperties = {
  fontSize: '0.8rem',
  color: 'var(--color-text-muted)',
  marginBottom: 'var(--space-4)',
};
const labelStyle: React.CSSProperties = {
  fontSize: '0.75rem',
  color: 'var(--color-text-secondary)',
  fontFamily: 'var(--font-label)',
  marginBottom: 'var(--space-1)',
};
const selectStyle: React.CSSProperties = {
  background: 'var(--color-bg-input)',
  border: '1px solid var(--color-border)',
  borderRadius: 'var(--radius-sm)',
  padding: 'var(--space-2) var(--space-3)',
  color: 'var(--color-text-primary)',
  fontSize: '0.85rem',
  fontFamily: 'var(--font-body)',
  width: '100%',
};
const genBtn: React.CSSProperties = {
  background: 'var(--color-accent)',
  color: 'var(--color-text-inverse)',
  border: 'none',
  borderRadius: 'var(--radius-sm)',
  padding: 'var(--space-2) var(--space-4)',
  fontSize: '0.85rem',
  fontWeight: 500,
  cursor: 'pointer',
  width: '100%',
  marginTop: 'var(--space-3)',
};

interface CardProps {
  openViewer: (v: { url: string; filename: string }) => void;
}

// --- Report Cards ---
function FlightLogCard({ aircraft, pilots, openViewer }: CardProps & { aircraft: Aircraft[]; pilots: Pilot[] }) {
  const [aircraftId, setAircraftId] = useState('');
  const [pilotId, setPilotId] = useState('');

  const generate = () => {
    const params = new URLSearchParams();
    if (aircraftId) params.set('aircraft_id', aircraftId);
    if (pilotId) params.set('pilot_id', pilotId);
    const q = params.toString() ? `?${params}` : '';
    openViewer({ url: `/api/v1/reports/flights${q}`, filename: 'flight-log.pdf' });
  };

  return (
    <div style={cardStyle}>
      <div style={cardTitle}><FileText size={18} color="var(--color-accent)" />Flight Log Report</div>
      <p style={cardDesc}>Complete flight history with EASA categories and statistics</p>
      <div style={{ marginBottom: 'var(--space-3)' }}>
        <div style={labelStyle}>Aircraft (optional)</div>
        <select style={selectStyle} value={aircraftId} onChange={e => setAircraftId(e.target.value)}>
          <option value="">All aircraft</option>
          {aircraft.map(a => <option key={a.id} value={a.id}>{formatAircraftLabel(a)}</option>)}
        </select>
      </div>
      <div style={{ marginBottom: 'var(--space-1)' }}>
        <div style={labelStyle}>Pilot (optional)</div>
        <select style={selectStyle} value={pilotId} onChange={e => setPilotId(e.target.value)}>
          <option value="">All pilots</option>
          {pilots.filter(p => p.is_active).map(p => <option key={p.id} value={p.id}>{p.name}</option>)}
        </select>
      </div>
      <button onClick={generate} style={genBtn}>Generate PDF</button>
    </div>
  );
}

function MaintenanceCard({ aircraft, openViewer }: CardProps & { aircraft: Aircraft[] }) {
  const [aircraftId, setAircraftId] = useState('');

  const generate = () => {
    const q = aircraftId ? `?aircraft_id=${aircraftId}` : '';
    openViewer({ url: `/api/v1/reports/maintenance${q}`, filename: 'maintenance-report.pdf' });
  };

  return (
    <div style={cardStyle}>
      <div style={cardTitle}><Wrench size={18} color="var(--color-accent)" />Maintenance Report</div>
      <p style={cardDesc}>EASA OSO #3 compliant maintenance records</p>
      <div style={{ marginBottom: 'var(--space-1)' }}>
        <div style={labelStyle}>Aircraft (optional)</div>
        <select style={selectStyle} value={aircraftId} onChange={e => setAircraftId(e.target.value)}>
          <option value="">All aircraft</option>
          {aircraft.map(a => <option key={a.id} value={a.id}>{formatAircraftLabel(a)}</option>)}
        </select>
      </div>
      <button onClick={generate} style={genBtn}>Generate PDF</button>
    </div>
  );
}

function PilotLogbookCard({ pilots, openViewer }: CardProps & { pilots: Pilot[] }) {
  const [pilotId, setPilotId] = useState('');
  const [error, setError] = useState('');

  const generate = () => {
    if (!pilotId) { setError('Please select a pilot'); return; }
    openViewer({ url: `/api/v1/reports/pilot-logbook?pilot_id=${pilotId}`, filename: 'pilot-logbook.pdf' });
  };

  return (
    <div style={cardStyle}>
      <div style={cardTitle}><Users size={18} color="var(--color-accent)" />Pilot Logbook</div>
      <p style={cardDesc}>Individual pilot logbook with total flight hours</p>
      <div style={{ marginBottom: 'var(--space-1)' }}>
        <div style={labelStyle}>Pilot (required)</div>
        <select style={selectStyle} value={pilotId} onChange={e => { setPilotId(e.target.value); setError(''); }}>
          <option value="">— Select pilot —</option>
          {pilots.filter(p => p.is_active).map(p => <option key={p.id} value={p.id}>{p.name}</option>)}
        </select>
      </div>
      {error && <p style={{ color: 'var(--color-critical)', fontSize: '0.8rem', marginTop: 'var(--space-2)' }}>{error}</p>}
      <button onClick={generate} style={genBtn}>Generate PDF</button>
    </div>
  );
}

function RetiredEquipmentCard() {
  const navigate = useNavigate();
  return (
    <div style={cardStyle}>
      <div style={cardTitle}><Archive size={18} color="var(--color-accent)" />Retired Equipment</div>
      <p style={cardDesc}>View and reactivate retired aircraft, batteries, and controllers</p>
      <button onClick={() => navigate('/retired-equipment')} style={genBtn}>View Retired Equipment</button>
    </div>
  );
}

function FleetStatusCard({ openViewer }: CardProps) {
  return (
    <div style={cardStyle}>
      <div style={cardTitle}><Boxes size={18} color="var(--color-accent)" />Fleet Status</div>
      <p style={cardDesc}>Fleet overview with aircraft status and utilisation</p>
      <p style={{ fontSize: '0.8rem', color: 'var(--color-text-secondary)', marginBottom: 'var(--space-1)' }}>No filters — generates complete fleet overview</p>
      <button onClick={() => openViewer({ url: '/api/v1/reports/fleet', filename: 'fleet-status.pdf' })} style={genBtn}>Generate PDF</button>
    </div>
  );
}

function EquipmentIdSheetCard({ openViewer }: CardProps) {
  return (
    <div style={cardStyle}>
      <div style={cardTitle}><QrCode size={18} color="var(--color-accent)" />Equipment ID Sheet</div>
      <p style={cardDesc}>
        Printable sheet of all active aircraft, controllers, and batteries with
        their assigned Equipment IDs and QR codes. Used for physical labelling.
      </p>
      <button
        onClick={() => openViewer({ url: '/api/v1/reports/equipment-ids', filename: 'equipment-id-sheet.pdf' })}
        style={genBtn}
      >Generate PDF</button>
    </div>
  );
}

// --- Main Reports Page ---
export function ReportsPage() {
  // TODO: queryKey intent says active-only but we pass include_retired=true
  // to preserve historical behavior. Per-site audit needed: should this dropdown
  // include retired items (for historical records) or active only (for new entries)?
  // Tracked separately. Reverting to active-only would change UX and may hide
  // aircraft from edit forms of historical records.
  const { data: aircraft } = useQuery({ queryKey: ['aircraft', true], queryFn: () => fetchAircraft(true) });
  const { data: pilots } = useQuery({ queryKey: ['pilots'], queryFn: fetchPilots });
  const aircraftList = aircraft ?? [];
  const pilotList = pilots ?? [];

  const [viewer, setViewer] = useState<ViewerState>(null);
  const openViewer = (v: { url: string; filename: string }) => setViewer(v);

  return (
    <div>
      <h1 style={{
        fontFamily: 'var(--font-display)',
        fontSize: '1.25rem',
        fontWeight: 500,
        color: 'var(--color-text-primary)',
        letterSpacing: '0.02em',
        marginBottom: 'var(--space-5)',
      }}>Reports</h1>

      <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 'var(--space-4)' }}>
        <FlightLogCard aircraft={aircraftList} pilots={pilotList} openViewer={openViewer} />
        <MaintenanceCard aircraft={aircraftList} openViewer={openViewer} />
        <PilotLogbookCard pilots={pilotList} openViewer={openViewer} />
        <FleetStatusCard openViewer={openViewer} />
        <EquipmentIdSheetCard openViewer={openViewer} />
        <RetiredEquipmentCard />
      </div>

      {viewer && (
        <PdfViewer
          url={viewer.url}
          filename={viewer.filename}
          onClose={() => setViewer(null)}
        />
      )}
    </div>
  );
}
