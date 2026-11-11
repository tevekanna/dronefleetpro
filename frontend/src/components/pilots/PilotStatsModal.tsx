// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useQuery } from '@tanstack/react-query';
import { X } from 'lucide-react';
import { fetchPilotStats, type CategoryBreakdown, type MissionBreakdown, type QualificationEntry, type TopAircraft, type TopModel } from '../../api/pilotStats';

// Inline-styled; the rest of PilotsPage uses inline styles too, so we
// match that convention rather than introducing a new CSS module.
const overlay: React.CSSProperties = {
  position: 'fixed', inset: 0, background: 'rgba(0,0,0,0.7)',
  display: 'flex', alignItems: 'center', justifyContent: 'center', zIndex: 1000,
};
const modal: React.CSSProperties = {
  background: 'var(--color-bg-elevated)',
  border: '1px solid var(--color-border-strong)',
  borderRadius: 'var(--radius-lg)',
  padding: 'var(--space-6)',
  width: '100%', maxWidth: 880, maxHeight: '90vh', overflowY: 'auto',
  boxShadow: 'var(--shadow-modal)',
};
const header: React.CSSProperties = {
  display: 'flex', alignItems: 'baseline', justifyContent: 'space-between',
  marginBottom: 'var(--space-5)',
};
const title: React.CSSProperties = {
  fontFamily: 'var(--font-display)', fontSize: '1.1rem', fontWeight: 500,
  color: 'var(--color-text-primary)',
};
const sectionTitle: React.CSSProperties = {
  fontFamily: 'var(--font-label)', fontSize: '0.75rem', letterSpacing: '0.05em',
  textTransform: 'uppercase', color: 'var(--color-text-muted)',
  marginBottom: 'var(--space-2)', marginTop: 'var(--space-5)',
};
const overviewGrid: React.CSSProperties = {
  display: 'grid', gridTemplateColumns: 'repeat(3, 1fr)', gap: 'var(--space-3)',
};
const overviewCard: React.CSSProperties = {
  background: 'var(--color-bg-surface)', border: '1px solid var(--color-border)',
  borderRadius: 'var(--radius-md)', padding: 'var(--space-4)',
};
const bigValue: React.CSSProperties = {
  fontFamily: 'var(--font-display)', fontSize: '1.4rem', fontWeight: 500,
  color: 'var(--color-text-primary)', marginBottom: 'var(--space-1)',
};
const smallLabel: React.CSSProperties = {
  fontFamily: 'var(--font-label)', fontSize: '0.7rem',
  textTransform: 'uppercase', letterSpacing: '0.05em',
  color: 'var(--color-text-muted)',
};
const twoCol: React.CSSProperties = {
  display: 'grid', gridTemplateColumns: '1fr 1fr', gap: 'var(--space-4)',
};
const table: React.CSSProperties = {
  width: '100%', borderCollapse: 'collapse',
  fontFamily: 'var(--font-body)', fontSize: '0.85rem',
};
const th: React.CSSProperties = {
  textAlign: 'left', padding: 'var(--space-1) var(--space-2)',
  color: 'var(--color-text-muted)', fontFamily: 'var(--font-label)',
  fontSize: '0.72rem', fontWeight: 500, textTransform: 'uppercase',
  letterSpacing: '0.05em', borderBottom: '1px solid var(--color-border)',
};
const td: React.CSSProperties = {
  padding: 'var(--space-1) var(--space-2)',
  borderBottom: '1px solid var(--color-border)',
  color: 'var(--color-text-primary)',
};
const closeBtn: React.CSSProperties = {
  background: 'transparent', color: 'var(--color-text-secondary)',
  border: '1px solid var(--color-border)', borderRadius: 'var(--radius-sm)',
  padding: 'var(--space-1) var(--space-2)', cursor: 'pointer',
  display: 'flex', alignItems: 'center',
};

function fmtHours(seconds: number): string {
  if (!seconds) return '0m';
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  return h > 0 ? `${h}h ${m.toString().padStart(2, '0')}m` : `${m}m`;
}

// EASA category colour map. Falls back to accent for unrecognised values.
function easaColor(cat: string): string {
  const k = cat.toLowerCase();
  if (k.startsWith('open')) return 'var(--color-ok)';
  if (k.startsWith('specific')) return 'var(--color-warning)';
  if (k.startsWith('certified')) return 'var(--color-critical)';
  return 'var(--color-accent)';
}

function Bar({ pct, color, label, count }: { pct: number; color: string; label: string; count: number }) {
  return (
    <div style={{ marginBottom: 'var(--space-2)' }}>
      <div style={{ display: 'flex', justifyContent: 'space-between', fontSize: '0.8rem', marginBottom: 2 }}>
        <span style={{ color: 'var(--color-text-primary)' }}>{label}</span>
        <span style={{ color: 'var(--color-text-muted)', fontFamily: 'var(--font-display)' }}>
          {count} · {pct.toFixed(1)}%
        </span>
      </div>
      <div style={{
        height: 6, borderRadius: 3,
        background: 'var(--color-bg-input)', border: '1px solid var(--color-border)',
        overflow: 'hidden',
      }}>
        <div style={{
          height: '100%', width: `${Math.min(100, pct)}%`, background: color,
          transition: 'width 0.2s ease',
        }} />
      </div>
    </div>
  );
}

function qualStatusStyle(status: QualificationEntry['status']): React.CSSProperties {
  const base: React.CSSProperties = {
    display: 'inline-block', padding: '2px 8px', borderRadius: 'var(--radius-sm)',
    fontSize: '0.72rem', fontFamily: 'var(--font-display)',
    textTransform: 'uppercase', letterSpacing: '0.03em',
  };
  switch (status) {
    case 'ok':
      return { ...base, color: 'var(--color-ok)', background: 'rgba(34,197,94,0.15)', border: '1px solid rgba(34,197,94,0.3)' };
    case 'expiring_soon':
      return { ...base, color: 'var(--color-warning)', background: 'rgba(245,158,11,0.15)', border: '1px solid rgba(245,158,11,0.3)' };
    case 'expired':
      return { ...base, color: 'var(--color-critical)', background: 'rgba(239,68,68,0.15)', border: '1px solid rgba(239,68,68,0.3)' };
    case 'no_expiry':
    default:
      return { ...base, color: 'var(--color-text-muted)', background: 'var(--color-bg-input)', border: '1px solid var(--color-border)' };
  }
}

interface Props {
  pilotId: number;
  onClose: () => void;
}

export function PilotStatsModal({ pilotId, onClose }: Props) {
  const { data, isLoading, error, refetch } = useQuery({
    queryKey: ['pilotStats', pilotId],
    queryFn: () => fetchPilotStats(pilotId),
    retry: false,
  });

  return (
    <div style={overlay} onClick={onClose}>
      <div style={modal} onClick={e => e.stopPropagation()}>
        <div style={header}>
          <div style={title}>
            {data ? `Statistics — ${data.pilot_name}` : 'Pilot Statistics'}
          </div>
          <button style={closeBtn} onClick={onClose} aria-label="Close">
            <X size={16} />
          </button>
        </div>

        {isLoading && (
          <p style={{ color: 'var(--color-text-secondary)' }}>Loading stats…</p>
        )}
        {error && !isLoading && (
          <div>
            <p style={{ color: 'var(--color-critical)', marginBottom: 'var(--space-3)' }}>
              Failed to load statistics: {(error as Error).message}
            </p>
            <button style={closeBtn} onClick={() => refetch()}>Retry</button>
          </div>
        )}
        {data && (
          <>
            {/* Overview */}
            <div style={overviewGrid}>
              <div style={overviewCard}>
                <div style={bigValue}>{fmtHours(data.flight_time_total_seconds)}</div>
                <div style={smallLabel}>Total Flight Time · {data.flight_count_total} flights</div>
              </div>
              <div style={overviewCard}>
                <div style={bigValue}>{data.flight_count_30d}</div>
                <div style={smallLabel}>Flights · last 30 days</div>
              </div>
              <div style={overviewCard}>
                <div style={bigValue}>{data.flight_count_365d}</div>
                <div style={smallLabel}>Flights · last 365 days</div>
              </div>
            </div>

            {/* Top aircraft + models */}
            <div style={twoCol}>
              <div>
                <div style={sectionTitle}>Top Aircraft</div>
                <TopTable
                  rows={data.top_aircraft.map<TopRow>((a: TopAircraft) => ({
                    label: a.equipment_id ?? a.model ?? `#${a.aircraft_id}`,
                    sub: a.model ?? '',
                    seconds: a.flight_seconds,
                    count: a.flight_count,
                  }))}
                />
              </div>
              <div>
                <div style={sectionTitle}>Top Models</div>
                <TopTable
                  rows={data.top_models.map<TopRow>((m: TopModel) => ({
                    label: m.model ?? '—',
                    sub: '',
                    seconds: m.flight_seconds,
                    count: m.flight_count,
                  }))}
                />
              </div>
            </div>

            {/* Breakdowns */}
            <div style={twoCol}>
              <div>
                <div style={sectionTitle}>EASA Categories</div>
                {data.easa_categories.length === 0
                  ? <div style={{ color: 'var(--color-text-muted)', fontSize: '0.85rem' }}>No flights.</div>
                  : data.easa_categories.map((c: CategoryBreakdown) => (
                    <Bar
                      key={c.category}
                      pct={c.pct}
                      color={easaColor(c.category)}
                      label={c.category}
                      count={c.flight_count}
                    />
                  ))}
              </div>
              <div>
                <div style={sectionTitle}>Mission Types</div>
                {data.mission_types.length === 0
                  ? <div style={{ color: 'var(--color-text-muted)', fontSize: '0.85rem' }}>No flights.</div>
                  : data.mission_types.map((m: MissionBreakdown) => (
                    <Bar
                      key={m.mission_type}
                      pct={m.pct}
                      color="var(--color-accent)"
                      label={m.mission_type}
                      count={m.flight_count}
                    />
                  ))}
              </div>
            </div>

            {/* Qualifications */}
            <div style={sectionTitle}>Qualifications</div>
            {data.qualifications.length === 0
              ? <div style={{ color: 'var(--color-text-muted)', fontSize: '0.85rem' }}>No qualifications recorded.</div>
              : (
                <table style={table}>
                  <thead>
                    <tr>
                      <th style={th}>Name</th>
                      <th style={th}>Expires</th>
                      <th style={th}>Days</th>
                      <th style={th}>Status</th>
                    </tr>
                  </thead>
                  <tbody>
                    {data.qualifications.map((q, i) => (
                      <tr key={`${q.qualification_name}-${i}`}>
                        <td style={{ ...td, fontFamily: 'var(--font-display)' }}>{q.qualification_name}</td>
                        <td style={td}>{q.expires_on ?? '—'}</td>
                        <td style={{ ...td, fontFamily: 'var(--font-display)', color: q.days_until_expiry != null && q.days_until_expiry < 0 ? 'var(--color-critical)' : 'inherit' }}>
                          {q.days_until_expiry == null ? '—' : q.days_until_expiry}
                        </td>
                        <td style={td}>
                          <span style={qualStatusStyle(q.status)}>{q.status.replace('_', ' ')}</span>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              )}
          </>
        )}
      </div>
    </div>
  );
}

// Internal helper for the top-3 tables.
interface TopRow { label: string; sub: string; seconds: number; count: number }

function TopTable({ rows }: { rows: TopRow[] }) {
  if (rows.length === 0) {
    return <div style={{ color: 'var(--color-text-muted)', fontSize: '0.85rem' }}>No flights.</div>;
  }
  return (
    <table style={table}>
      <thead>
        <tr>
          <th style={th}>Name</th>
          <th style={th}>Time</th>
          <th style={th}>Count</th>
        </tr>
      </thead>
      <tbody>
        {rows.map((r, i) => (
          <tr key={i}>
            <td style={td}>
              <div style={{ fontFamily: 'var(--font-display)', fontSize: '0.85rem' }}>{r.label}</div>
              {r.sub && <div style={{ fontSize: '0.72rem', color: 'var(--color-text-muted)' }}>{r.sub}</div>}
            </td>
            <td style={{ ...td, fontFamily: 'var(--font-display)' }}>{fmtHours(r.seconds)}</td>
            <td style={{ ...td, fontFamily: 'var(--font-display)' }}>{r.count}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}
