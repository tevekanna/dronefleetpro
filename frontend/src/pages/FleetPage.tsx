// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useMemo, useState } from 'react';
import { useQuery, useMutation, useQueryClient } from '@tanstack/react-query';
import { Link, useNavigate, useSearchParams } from 'react-router-dom';
import { Upload, LineChart as LineChartIcon } from 'lucide-react';
import { fetchAircraft, createAircraft, updateAircraft } from '../api/aircraft';
import { fetchAircraftModels } from '../api/aircraftModels';
import { fetchBatteries, createBattery, updateBattery } from '../api/batteries';
import { fetchControllers, createController, updateController } from '../api/controllers';
import { fetchAccessories, updateAccessory } from '../api/accessories';
import { BatchImportModal } from '../components/fleet/BatchImportModal';
import { AccessoryFormModal } from '../components/fleet/AccessoryFormModal';
import { MergeAircraftModal } from '../components/fleet/MergeAircraftModal';
import { FleetSearchBar, highlightMatch } from '../components/fleet/FleetSearchBar';
import { ModelAccordion } from '../components/shared/ModelAccordion';
import type { AccordionGroup } from '../components/shared/ModelAccordion';
import { RetireModal } from '../components/shared/RetireModal';
import { useDebouncedValue } from '../hooks/useDebouncedValue';
import type { Aircraft } from '../types/aircraft';
import type { Battery } from '../types/battery';
import type { Controller } from '../types/controller';
import type { Accessory } from '../types/accessory';
import type { CreateAircraftRequest } from '../types/aircraft';
import type { CreateBatteryRequest } from '../types/battery';
import type { CreateControllerRequest } from '../types/controller';
import { useCurrentUser } from '../hooks/useCurrentUser';
import { userHasAnyRole } from '../utils/auth';
import { AircraftCell } from '../components/shared/AircraftCell';
import { formatAircraftLabel } from '../utils/aircraftFormat';

type Tab = 'aircraft' | 'batteries' | 'controllers' | 'accessories';

// --- Shared inline style helpers ---
const tableStyle: React.CSSProperties = {
  width: '100%',
  borderCollapse: 'collapse' as const,
  fontFamily: 'var(--font-body)',
  fontSize: '0.85rem',
};

const thStyle: React.CSSProperties = {
  textAlign: 'left',
  padding: 'var(--space-2) var(--space-3)',
  color: 'var(--color-text-muted)',
  fontFamily: 'var(--font-label)',
  fontSize: '0.75rem',
  fontWeight: 500,
  textTransform: 'uppercase' as const,
  letterSpacing: '0.05em',
  borderBottom: '1px solid var(--color-border)',
};

const tdStyle: React.CSSProperties = {
  padding: 'var(--space-2) var(--space-3)',
  borderBottom: '1px solid var(--color-border)',
  color: 'var(--color-text-primary)',
};

const monoStyle: React.CSSProperties = {
  fontFamily: 'var(--font-display)',
  fontSize: '0.8rem',
};

// Inline-styled warning indicators for needs_manual_sn aircraft. Co-located
// with the Fleet table styles so they pick up the same design tokens. Banner
// uses an amber border with no extra background — the dark surface tokens
// keep contrast readable without introducing a new colour token.
const needsManualSnIconStyle: React.CSSProperties = {
  display: 'inline-block',
  marginRight: 'var(--space-2)',
  color: 'var(--color-warning)',
  cursor: 'help',
};

const needsManualSnBannerStyle: React.CSSProperties = {
  borderLeft: '4px solid var(--color-warning)',
  background: 'var(--color-bg-elevated)',
  color: 'var(--color-text-primary)',
  padding: 'var(--space-3) var(--space-4)',
  marginBottom: 'var(--space-4)',
  borderRadius: 'var(--radius-sm)',
  fontSize: '0.85rem',
  lineHeight: 1.4,
};

// Per-group "⚠ N pending review" pill rendered via ModelAccordion's
// headerExtras slot. Mirrors the retiredToggleActive language from
// ModelAccordion.module.css (small amber label) so the warning state
// reads consistently across the fleet UI.
const needsManualSnGroupPillStyle: React.CSSProperties = {
  fontFamily: 'var(--font-label)',
  fontSize: '0.78rem',
  color: 'var(--color-warning)',
  cursor: 'pointer',
  whiteSpace: 'nowrap',
};

// Top-level "Show pending review only" toggle next to FleetSearchBar.
// Inactive = outlined amber, active = filled amber. The active state
// follows the same chip pattern as a primary action button so the
// filtered state is unmissable.
const pendingFilterBtnStyle = (active: boolean): React.CSSProperties => ({
  background: active ? 'var(--color-warning)' : 'transparent',
  color: active ? 'var(--color-text-inverse)' : 'var(--color-warning)',
  border: '1px solid var(--color-warning)',
  borderRadius: 'var(--radius-sm)',
  padding: 'var(--space-2) var(--space-3)',
  fontSize: '0.8rem',
  fontFamily: 'var(--font-label)',
  cursor: 'pointer',
  whiteSpace: 'nowrap',
});

// Airworthiness toggle chips next to the pending-review button. Outlined at
// rest, filled when active — green for airworthy, red for not airworthy —
// so the active filter reads at a glance and matches the status colour tokens.
const airworthinessChipStyle = (active: boolean, tone: 'ok' | 'critical'): React.CSSProperties => {
  const colour = tone === 'ok' ? 'var(--color-ok)' : 'var(--color-critical)';
  return {
    background: active ? colour : 'transparent',
    color: active ? 'var(--color-text-inverse)' : colour,
    border: `1px solid ${colour}`,
    borderRadius: 'var(--radius-sm)',
    padding: 'var(--space-2) var(--space-3)',
    fontSize: '0.8rem',
    fontFamily: 'var(--font-label)',
    cursor: 'pointer',
    whiteSpace: 'nowrap',
  };
};

const btnSmall: React.CSSProperties = {
  background: 'transparent',
  color: 'var(--color-text-secondary)',
  border: '1px solid var(--color-border)',
  borderRadius: 'var(--radius-sm)',
  padding: 'var(--space-1) var(--space-2)',
  cursor: 'pointer',
  fontSize: '0.8rem',
  marginRight: 'var(--space-1)',
};

const btnDanger: React.CSSProperties = {
  ...btnSmall,
  color: 'var(--color-critical)',
  borderColor: 'var(--color-critical)',
};

const retiredRowStyle: React.CSSProperties = {
  background: 'var(--color-bg-elevated)',
  color: 'var(--color-text-muted)',
};

const addBtnStyle: React.CSSProperties = {
  background: 'var(--color-accent)',
  color: 'var(--color-text-inverse)',
  border: 'none',
  borderRadius: 'var(--radius-sm)',
  padding: 'var(--space-2) var(--space-4)',
  fontSize: '0.85rem',
  fontWeight: 500,
  cursor: 'pointer',
};

const overlayStyle: React.CSSProperties = {
  position: 'fixed',
  inset: 0,
  background: 'rgba(0,0,0,0.7)',
  display: 'flex',
  alignItems: 'center',
  justifyContent: 'center',
  zIndex: 1000,
};

const modalStyle: React.CSSProperties = {
  background: 'var(--color-bg-elevated)',
  border: '1px solid var(--color-border-strong)',
  borderRadius: 'var(--radius-lg)',
  padding: 'var(--space-6)',
  width: '100%',
  maxWidth: '420px',
  boxShadow: 'var(--shadow-modal)',
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

const submitBtnStyle = (isPending: boolean): React.CSSProperties => ({
  background: 'var(--color-accent)',
  color: 'var(--color-text-inverse)',
  border: 'none',
  borderRadius: 'var(--radius-sm)',
  padding: 'var(--space-2) var(--space-4)',
  cursor: 'pointer',
  fontSize: '0.85rem',
  fontWeight: 500,
  opacity: isPending ? 0.7 : 1,
});

function statusColor(status: string): string {
  switch (status) {
    case 'active': return 'var(--color-ok)';
    case 'maintenance': return 'var(--color-warning)';
    case 'retired': return 'var(--color-text-muted)';
    default: return 'var(--color-text-secondary)';
  }
}

function cycleColor(cycles: number, max: number): string {
  const pct = max > 0 ? cycles / max : 0;
  if (pct > 0.9) return 'var(--color-critical)';
  if (pct > 0.7) return 'var(--color-warning)';
  return 'var(--color-ok)';
}

function formatDate(dateStr: string | null): string {
  if (!dateStr) return '—';
  return dateStr.substring(0, 10);
}

// --- Grouping helpers ---

function buildStatusSummary(items: { status: string }[]): string {
  const counts: Record<string, number> = {};
  for (const item of items) {
    const label = item.status === 'maintenance' ? 'in maintenance' : item.status;
    counts[label] = (counts[label] ?? 0) + 1;
  }
  return Object.entries(counts)
    .filter(([, n]) => n > 0)
    .map(([label, n]) => `${n} ${label}`)
    .join(' \u00b7 ');
}

function groupByField<T>(items: T[], getKey: (item: T) => string | null | undefined, fallbackLabel: string, fallbackKey: string): { key: string; label: string; items: T[] }[] {
  const map = new Map<string, T[]>();
  for (const item of items) {
    const raw = getKey(item);
    const key = raw?.trim() || fallbackKey;
    const existing = map.get(key);
    if (existing) {
      existing.push(item);
    } else {
      map.set(key, [item]);
    }
  }
  const groups = Array.from(map.entries()).map(([key, groupItems]) => ({
    key,
    label: key === fallbackKey ? fallbackLabel : key,
    items: groupItems,
  }));
  groups.sort((a, b) => {
    if (a.key === fallbackKey) return 1;
    if (b.key === fallbackKey) return -1;
    return a.label.localeCompare(b.label);
  });
  return groups;
}

// --- Empty State ---
function EmptyState({ message, hint }: { message: string; hint: string }) {
  return (
    <div style={{
      display: 'flex',
      flexDirection: 'column',
      alignItems: 'center',
      justifyContent: 'center',
      padding: 'var(--space-10) var(--space-5)',
      background: 'var(--color-bg-surface)',
      border: '1px dashed var(--color-border)',
      borderRadius: 'var(--radius-md)',
    }}>
      <p style={{ fontFamily: 'var(--font-display)', fontSize: '1rem', color: 'var(--color-text-secondary)', marginBottom: 'var(--space-2)' }}>{message}</p>
      <p style={{ fontSize: '0.85rem', color: 'var(--color-text-muted)' }}>{hint}</p>
    </div>
  );
}

// --- Aircraft Table ---
function AircraftTab({ onAdd, onEdit, canEdit, searchQuery, onSearchChange }: { onAdd: () => void; onEdit: (a: Aircraft) => void; canEdit: boolean; searchQuery: string; onSearchChange: (v: string) => void }) {
  const queryClient = useQueryClient();
  const navigate = useNavigate();
  const { data: aircraft, isLoading, error } = useQuery({ queryKey: ['aircraft', true], queryFn: () => fetchAircraft(true) });
  const [retireTarget, setRetireTarget] = useState<Aircraft | null>(null);
  const [retiredVisible, setRetiredVisible] = useState<Record<string, boolean>>({});
  const [pendingOnly, setPendingOnly] = useState(false);
  const debouncedQuery = useDebouncedValue(searchQuery, 200);

  // Airworthiness filter is URL-driven (deep-linkable from the Dashboard
  // summary tiles). Only the two known values are honoured; anything else
  // (or absent) means no filter.
  const [searchParams, setSearchParams] = useSearchParams();
  const rawAir = searchParams.get('airworthiness');
  const airworthinessFilter: 'airworthy' | 'not_airworthy' | null =
    rawAir === 'airworthy' || rawAir === 'not_airworthy' ? rawAir : null;

  // Single-select toggle: re-selecting the active value clears the param.
  // Clones the existing params so any other query keys survive.
  const setAirworthinessFilter = (next: 'airworthy' | 'not_airworthy' | null) => {
    const clone = new URLSearchParams(searchParams);
    if (next === null) {
      clone.delete('airworthiness');
    } else {
      clone.set('airworthiness', next);
    }
    setSearchParams(clone);
  };

  const goToAircraft = (aircraftId: number) => navigate(`/aircraft/${aircraftId}`);

  const retireMutation = useMutation({
    mutationFn: ({ id, notes }: { id: number; notes: string }) =>
      updateAircraft(id, { status: 'retired', retirement_notes: notes || undefined }),
    onSuccess: () => { queryClient.invalidateQueries({ queryKey: ['aircraft', true] }); setRetireTarget(null); },
  });

  const allItems = useMemo(() => aircraft ?? [], [aircraft]);

  // Airworthiness chip counts — scoped to non-retired aircraft (retired
  // craft carry no meaningful airworthiness state). Computed from the
  // unfiltered list so the chip labels stay truthful under any narrowing.
  const airworthyCount = useMemo(
    () => allItems.filter(a => a.status !== 'retired' && a.airworthiness_status === 'airworthy').length,
    [allItems],
  );
  const notAirworthyCount = useMemo(
    () => allItems.filter(a => a.status !== 'retired' && a.airworthiness_status === 'not_airworthy').length,
    [allItems],
  );

  // Pending review = needs_manual_sn AND not retired. Retired aircraft
  // don't need anyone to chase their SN, so they're excluded everywhere.
  const totalPendingCount = useMemo(
    () => allItems.filter(a => a.needs_manual_sn && a.status !== 'retired').length,
    [allItems],
  );

  // Per-model pending counts are computed from the unfiltered list so the
  // "⚠ N pending review" pill stays truthful even when a search or the
  // pending-only filter is narrowing the visible rows. Keys must match
  // the fallback key used by groupByField below ('unknown').
  const pendingCountsByModel = useMemo(() => {
    const counts: Record<string, number> = {};
    for (const a of allItems) {
      if (!a.needs_manual_sn || a.status === 'retired') continue;
      const key = a.model?.trim() || 'unknown';
      counts[key] = (counts[key] ?? 0) + 1;
    }
    return counts;
  }, [allItems]);

  const filteredItems = useMemo(() => {
    let base = pendingOnly
      ? allItems.filter(a => a.needs_manual_sn && a.status !== 'retired')
      : allItems;
    if (airworthinessFilter !== null) {
      base = base.filter(a => a.airworthiness_status === airworthinessFilter && a.status !== 'retired');
    }
    const q = debouncedQuery.trim().toLowerCase();
    if (!q) return base;
    return base.filter(a =>
      a.serial_number.toLowerCase().includes(q)
      || (a.equipment_id ?? '').toLowerCase().includes(q)
      || (a.nickname ?? '').toLowerCase().includes(q)
      || (a.model ?? '').toLowerCase().includes(q),
    );
  }, [allItems, debouncedQuery, pendingOnly, airworthinessFilter]);

  // Map each accordion group key (== aircraft.model name, lowercased+trimmed)
  // back to its model_id so the "View model →" link in the header can route
  // to /aircraft-models/:id. Built from the unfiltered list so the link
  // survives search/pending filtering. Skipped for the 'unknown' fallback
  // group (no model_id to link to). MUST stay above the early returns
  // below — React's Rules of Hooks require an identical hook-call order
  // on every render, and the loading/error/empty branches return early.
  const modelIdByGroupKey = useMemo(() => {
    const m: Record<string, number> = {};
    for (const a of allItems) {
      const key = a.model?.trim();
      if (!key || a.model_id == null) continue;
      if (!(key in m)) m[key] = a.model_id;
    }
    return m;
  }, [allItems]);

  if (isLoading) return <p style={{ color: 'var(--color-text-secondary)' }}>Loading aircraft...</p>;
  if (error) return <p style={{ color: 'var(--color-critical)' }}>Failed to load aircraft: {(error as Error).message}</p>;

  if (allItems.length === 0) {
    return (
      <div>
        <div style={{ display: 'flex', justifyContent: 'flex-end', marginBottom: 'var(--space-4)' }}>
          {canEdit && <button onClick={onAdd} style={addBtnStyle}>Add Aircraft</button>}
        </div>
        <EmptyState message="No aircraft registered yet." hint="Add your first aircraft to get started." />
      </div>
    );
  }

  const groups = groupByField(filteredItems, a => a.model, 'Unknown Model', 'unknown');

  const retiredCounts: Record<string, number> = {};
  groups.forEach(g => { retiredCounts[g.key] = g.items.filter(a => a.status === 'retired').length; });

  const accordionGroups: AccordionGroup[] = groups
    .filter(g => g.items.some(a => a.status !== 'retired') || retiredVisible[g.key])
    .map(g => {
      const visibleItems = retiredVisible[g.key] ? g.items : g.items.filter(a => a.status !== 'retired');
      const pendingInGroup = pendingCountsByModel[g.key] ?? 0;
      const modelId = modelIdByGroupKey[g.key] ?? null;
      return {
        groupKey: g.key,
        label: g.label,
        count: g.items.filter(a => a.status !== 'retired').length,
        statusSummary: buildStatusSummary(g.items.filter(a => a.status !== 'retired')),
        headerExtras: (pendingInGroup > 0 || modelId !== null) ? (
          <span style={{ display: 'inline-flex', gap: 'var(--space-2)', alignItems: 'center' }}>
            {pendingInGroup > 0 && (
              <span
                style={needsManualSnGroupPillStyle}
                onClick={e => {
                  // Stop the accordion header from toggling open/closed when
                  // the pill is used to activate the top-level filter.
                  e.stopPropagation();
                  setPendingOnly(true);
                }}
                title="Show only aircraft pending manual SN correction"
                role="button"
                aria-label={`${pendingInGroup} aircraft pending review — click to filter`}
              >
                ⚠ {pendingInGroup} pending review
              </span>
            )}
            {modelId !== null && (
              <Link
                to={`/aircraft-models/${modelId}`}
                onClick={e => e.stopPropagation()}
                style={{
                  fontSize: '0.75rem',
                  color: 'var(--color-accent)',
                  textDecoration: 'none',
                  whiteSpace: 'nowrap',
                }}
                title={`Open ${g.label} model detail`}
              >
                View model →
              </Link>
            )}
          </span>
        ) : undefined,
        children: (
          <table style={tableStyle}>
            <thead>
              <tr>
                <th style={thStyle}>Equipment ID</th>
                <th style={thStyle}>Serial Number</th>
                <th style={thStyle}>Status</th>
                <th style={thStyle}>Total Flights</th>
                <th style={thStyle}>Last Flight</th>
                <th style={thStyle}>Actions</th>
              </tr>
            </thead>
            <tbody>
              {visibleItems.map((a: Aircraft) => (
                <tr
                  key={a.id}
                  className="fleet-aircraft-row-clickable"
                  style={{
                    cursor: 'pointer',
                    ...(a.status === 'retired' ? retiredRowStyle : {}),
                  }}
                  role="button"
                  tabIndex={0}
                  aria-label={`View details for aircraft ${a.equipment_id ?? a.serial_number}`}
                  onClick={() => goToAircraft(a.id)}
                  onKeyDown={e => {
                    if (e.key === 'Enter' || e.key === ' ') {
                      e.preventDefault();
                      goToAircraft(a.id);
                    }
                  }}
                >
                  <td style={{ ...tdStyle, ...monoStyle }}>{a.equipment_id ? highlightMatch(a.equipment_id, debouncedQuery) : '—'}</td>
                  <td style={{ ...tdStyle, ...monoStyle }}>
                    {a.needs_manual_sn && (
                      <span
                        title="No verified serial number — operator should correct this manually"
                        aria-label="needs manual serial number"
                        style={needsManualSnIconStyle}
                      >
                        ⚠
                      </span>
                    )}
                    {highlightMatch(a.serial_number, debouncedQuery)}
                  </td>
                  <td style={{ ...tdStyle, color: statusColor(a.status), fontWeight: 500 }}>{a.status}</td>
                  <td style={{ ...tdStyle, ...monoStyle }}>{a.total_flights}</td>
                  <td style={{ ...tdStyle, ...monoStyle }}>{formatDate(a.last_flight)}</td>
                  <td style={tdStyle}>
                    <button style={btnSmall} onClick={e => { e.stopPropagation(); onEdit(a); }}>Edit</button>
                    {a.status !== 'retired' && (
                      <button style={btnDanger} onClick={e => { e.stopPropagation(); setRetireTarget(a); }}>Retire</button>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        ),
      };
    });

  const hasActiveQuery = debouncedQuery.trim().length > 0;
  const isFiltered = hasActiveQuery || pendingOnly || airworthinessFilter !== null;

  return (
    <div>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start', gap: 'var(--space-4)', marginBottom: 'var(--space-4)' }}>
        <div style={{ flex: '1 1 auto', minWidth: 0 }}>
          <FleetSearchBar
            value={searchQuery}
            onChange={onSearchChange}
            matchCount={filteredItems.length}
            totalCount={allItems.length}
          />
        </div>
        {totalPendingCount > 0 && (
          <button
            type="button"
            onClick={() => setPendingOnly(p => !p)}
            style={pendingFilterBtnStyle(pendingOnly)}
            aria-pressed={pendingOnly}
            title={pendingOnly ? 'Clear pending-review filter' : 'Show only aircraft pending manual SN correction'}
          >
            {pendingOnly
              ? `⚠ Pending review only (${totalPendingCount}) — clear`
              : `⚠ Show pending review only (${totalPendingCount})`}
          </button>
        )}
        <button
          type="button"
          onClick={() => setAirworthinessFilter(airworthinessFilter === 'airworthy' ? null : 'airworthy')}
          style={airworthinessChipStyle(airworthinessFilter === 'airworthy', 'ok')}
          aria-pressed={airworthinessFilter === 'airworthy'}
          title="Show only airworthy aircraft"
        >
          {`✓ Airworthy (${airworthyCount})`}
        </button>
        <button
          type="button"
          onClick={() => setAirworthinessFilter(airworthinessFilter === 'not_airworthy' ? null : 'not_airworthy')}
          style={airworthinessChipStyle(airworthinessFilter === 'not_airworthy', 'critical')}
          aria-pressed={airworthinessFilter === 'not_airworthy'}
          title="Show only aircraft that are not airworthy"
        >
          {`⚠ Not airworthy (${notAirworthyCount})`}
        </button>
        {canEdit && <button onClick={onAdd} style={addBtnStyle}>Add Aircraft</button>}
      </div>
      {isFiltered && filteredItems.length === 0 ? (
        pendingOnly ? (
          <EmptyState
            message="No aircraft pending review match your search."
            hint="Clear the search or the pending-review filter to see other aircraft."
          />
        ) : airworthinessFilter !== null && !hasActiveQuery ? (
          <EmptyState
            message={`No aircraft are currently ${airworthinessFilter === 'airworthy' ? 'airworthy' : 'not airworthy'}.`}
            hint="Clear the airworthiness filter to see the rest of the fleet."
          />
        ) : (
          <EmptyState
            message="No aircraft match your search."
            hint="Try a different serial number, equipment ID, model, or nickname."
          />
        )
      ) : (
        <ModelAccordion
          groups={accordionGroups}
          storageKey="fleet-aircraft-accordion"
          retiredCounts={retiredCounts}
          retiredVisible={retiredVisible}
          onRetiredToggle={key => setRetiredVisible(prev => ({ ...prev, [key]: !prev[key] }))}
          forceExpand={isFiltered}
        />
      )}
      <RetireModal
        isOpen={retireTarget !== null}
        entityType="aircraft"
        entityLabel={retireTarget ? formatAircraftLabel(retireTarget) : ''}
        onConfirm={notes => retireTarget && retireMutation.mutate({ id: retireTarget.id, notes })}
        onCancel={() => setRetireTarget(null)}
        isSubmitting={retireMutation.isPending}
      />
    </div>
  );
}

// --- Batteries Table ---
function BatteriesTab({ onAdd, onEdit, aircraft, canEdit, searchQuery, onSearchChange }: { onAdd: () => void; onEdit: (b: Battery) => void; aircraft: Aircraft[]; canEdit: boolean; searchQuery: string; onSearchChange: (v: string) => void }) {
  const queryClient = useQueryClient();
  const { data: batteries, isLoading, error } = useQuery({ queryKey: ['batteries', true], queryFn: () => fetchBatteries(true) });
  const [retireTarget, setRetireTarget] = useState<Battery | null>(null);
  const [retiredVisible, setRetiredVisible] = useState<Record<string, boolean>>({});
  const debouncedQuery = useDebouncedValue(searchQuery, 200);

  const retireMutation = useMutation({
    mutationFn: ({ id, notes }: { id: number; notes: string }) =>
      updateBattery(id, { status: 'retired', retirement_notes: notes || undefined } as Record<string, unknown>),
    onSuccess: () => { queryClient.invalidateQueries({ queryKey: ['batteries', true] }); setRetireTarget(null); },
  });

  const findAircraft = (id: number | null) => aircraft.find(a => a.id === id);
  const allItems = useMemo(() => batteries ?? [], [batteries]);

  const filteredItems = useMemo(() => {
    const q = debouncedQuery.trim().toLowerCase();
    if (!q) return allItems;
    return allItems.filter(b =>
      b.serial_number.toLowerCase().includes(q)
      || (b.equipment_id ?? '').toLowerCase().includes(q)
      || (b.compatible_model ?? '').toLowerCase().includes(q),
    );
  }, [allItems, debouncedQuery]);

  if (isLoading) return <p style={{ color: 'var(--color-text-secondary)' }}>Loading batteries...</p>;
  if (error) return <p style={{ color: 'var(--color-critical)' }}>Failed to load batteries: {(error as Error).message}</p>;

  if (allItems.length === 0) {
    return (
      <div>
        <div style={{ display: 'flex', justifyContent: 'flex-end', marginBottom: 'var(--space-4)' }}>
          {canEdit && <button onClick={onAdd} style={addBtnStyle}>Add Battery</button>}
        </div>
        <EmptyState message="No batteries registered yet." hint="Add batteries to track cycle counts and health." />
      </div>
    );
  }

  const groups = groupByField(filteredItems, b => b.compatible_model, 'Unassigned', 'unassigned');
  const retiredCounts: Record<string, number> = {};
  groups.forEach(g => { retiredCounts[g.key] = g.items.filter(b => b.status === 'retired').length; });

  const accordionGroups: AccordionGroup[] = groups
    .filter(g => g.items.some(b => b.status !== 'retired') || retiredVisible[g.key])
    .map(g => {
      const visibleItems = retiredVisible[g.key] ? g.items : g.items.filter(b => b.status !== 'retired');
      return {
        groupKey: g.key,
        label: g.label,
        count: g.items.filter(b => b.status !== 'retired').length,
        statusSummary: buildStatusSummary(g.items.filter(b => b.status !== 'retired')),
        children: (
          <table style={tableStyle}>
            <thead>
              <tr>
                <th style={thStyle}>Equipment ID</th>
                <th style={thStyle}>Compatible Model</th>
                <th style={thStyle}>Capacity</th>
                <th style={thStyle}>Weight</th>
                <th style={thStyle}>Serial Number</th>
                <th style={thStyle}>Aircraft</th>
                <th style={thStyle}>Cycles</th>
                <th style={thStyle}>Max Cycles</th>
                <th style={thStyle}>Status</th>
                <th style={thStyle}>Actions</th>
              </tr>
            </thead>
            <tbody>
              {visibleItems.map((b: Battery) => {
                const linked = findAircraft(b.aircraft_id);
                return (
                  <tr key={b.id} style={b.status === 'retired' ? retiredRowStyle : undefined}>
                    <td style={{ ...tdStyle, ...monoStyle }}>{b.equipment_id ? highlightMatch(b.equipment_id, debouncedQuery) : '—'}</td>
                    <td style={tdStyle}>{b.compatible_model ? highlightMatch(b.compatible_model, debouncedQuery) : '—'}</td>
                    <td style={{ ...tdStyle, ...monoStyle }}>{b.capacity_wh != null ? `${b.capacity_wh.toFixed(1)} Wh` : '—'}</td>
                    <td style={{ ...tdStyle, ...monoStyle }}>{b.weight_kg != null ? `${b.weight_kg.toFixed(2)} kg` : '—'}</td>
                    <td style={{ ...tdStyle, ...monoStyle }}>{highlightMatch(b.serial_number, debouncedQuery)}</td>
                    <td style={tdStyle}><AircraftCell aircraft={linked ?? null} /></td>
                    <td style={{ ...tdStyle, ...monoStyle, color: cycleColor(b.cycle_count, b.max_cycles) }}>
                      {b.cycle_count}/{b.max_cycles}
                    </td>
                    <td style={{ ...tdStyle, ...monoStyle }}>{b.max_cycles}</td>
                    <td style={{ ...tdStyle, color: statusColor(b.status), fontWeight: 500 }}>{b.status}</td>
                    <td style={tdStyle}>
                      <Link
                        to={`/fleet-stats/batteries/${b.id}`}
                        style={{ ...btnSmall, textDecoration: 'none', display: 'inline-flex', alignItems: 'center', gap: 4 }}
                        title="View usage statistics"
                      >
                        <LineChartIcon size={12} /> Stats
                      </Link>
                      <button style={btnSmall} onClick={() => onEdit(b)}>Edit</button>
                      {b.status !== 'retired' && (
                        <button style={btnDanger} onClick={() => setRetireTarget(b)}>Retire</button>
                      )}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        ),
      };
    });

  const hasActiveQuery = debouncedQuery.trim().length > 0;

  return (
    <div>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start', gap: 'var(--space-4)', marginBottom: 'var(--space-4)' }}>
        <div style={{ flex: '1 1 auto', minWidth: 0 }}>
          <FleetSearchBar
            value={searchQuery}
            onChange={onSearchChange}
            matchCount={filteredItems.length}
            totalCount={allItems.length}
          />
        </div>
        {canEdit && <button onClick={onAdd} style={addBtnStyle}>Add Battery</button>}
      </div>
      {hasActiveQuery && filteredItems.length === 0 ? (
        <EmptyState message="No batteries match your search." hint="Try a different serial number, equipment ID, or compatible model." />
      ) : (
        <ModelAccordion
          groups={accordionGroups}
          storageKey="fleet-battery-accordion"
          retiredCounts={retiredCounts}
          retiredVisible={retiredVisible}
          onRetiredToggle={key => setRetiredVisible(prev => ({ ...prev, [key]: !prev[key] }))}
          forceExpand={hasActiveQuery}
        />
      )}
      <RetireModal
        isOpen={retireTarget !== null}
        entityType="battery"
        entityLabel={retireTarget ? `${retireTarget.equipment_id ?? ''} — ${retireTarget.compatible_model ?? retireTarget.serial_number}`.replace(/^ — /, '') : ''}
        onConfirm={notes => retireTarget && retireMutation.mutate({ id: retireTarget.id, notes })}
        onCancel={() => setRetireTarget(null)}
        isSubmitting={retireMutation.isPending}
      />
    </div>
  );
}

// --- Controllers Table ---
function ControllersTab({ onAdd, onEdit, aircraft, canEdit, searchQuery, onSearchChange }: { onAdd: () => void; onEdit: (c: Controller) => void; aircraft: Aircraft[]; canEdit: boolean; searchQuery: string; onSearchChange: (v: string) => void }) {
  const queryClient = useQueryClient();
  const { data: controllers, isLoading, error } = useQuery({ queryKey: ['controllers', true], queryFn: () => fetchControllers(true) });
  const [retireTarget, setRetireTarget] = useState<Controller | null>(null);
  const [retiredVisible, setRetiredVisible] = useState<Record<string, boolean>>({});
  const debouncedQuery = useDebouncedValue(searchQuery, 200);

  const retireMutation = useMutation({
    mutationFn: ({ id, notes }: { id: number; notes: string }) =>
      updateController(id, { status: 'retired', retirement_notes: notes || undefined } as Record<string, unknown>),
    onSuccess: () => { queryClient.invalidateQueries({ queryKey: ['controllers', true] }); setRetireTarget(null); },
  });

  const findAircraft = (id: number | null) => aircraft.find(a => a.id === id);
  const allItems = useMemo(() => controllers ?? [], [controllers]);

  const filteredItems = useMemo(() => {
    const q = debouncedQuery.trim().toLowerCase();
    if (!q) return allItems;
    return allItems.filter(c =>
      c.serial_number.toLowerCase().includes(q)
      || (c.equipment_id ?? '').toLowerCase().includes(q)
      || (c.nickname ?? '').toLowerCase().includes(q)
      || (c.model ?? '').toLowerCase().includes(q)
      || (c.compatible_models ?? '').toLowerCase().includes(q),
    );
  }, [allItems, debouncedQuery]);

  if (isLoading) return <p style={{ color: 'var(--color-text-secondary)' }}>Loading controllers...</p>;
  if (error) return <p style={{ color: 'var(--color-critical)' }}>Failed to load controllers: {(error as Error).message}</p>;

  if (allItems.length === 0) {
    return (
      <div>
        <div style={{ display: 'flex', justifyContent: 'flex-end', marginBottom: 'var(--space-4)' }}>
          {canEdit && <button onClick={onAdd} style={addBtnStyle}>Add Controller</button>}
        </div>
        <EmptyState message="No controllers registered yet." hint="Add remote controllers to track your fleet equipment." />
      </div>
    );
  }

  const groups = groupByField(filteredItems, c => c.compatible_models, 'Unassigned', 'unassigned');
  const retiredCounts: Record<string, number> = {};
  groups.forEach(g => { retiredCounts[g.key] = g.items.filter(c => c.status === 'retired').length; });

  const accordionGroups: AccordionGroup[] = groups
    .filter(g => g.items.some(c => c.status !== 'retired') || retiredVisible[g.key])
    .map(g => {
      const visibleItems = retiredVisible[g.key] ? g.items : g.items.filter(c => c.status !== 'retired');
      return {
        groupKey: g.key,
        label: g.label,
        count: g.items.filter(c => c.status !== 'retired').length,
        statusSummary: buildStatusSummary(g.items.filter(c => c.status !== 'retired')),
        children: (
          <table style={tableStyle}>
            <thead>
              <tr>
                <th style={thStyle}>Equipment ID</th>
                <th style={thStyle}>Model</th>
                <th style={thStyle}>Compatible Models</th>
                <th style={thStyle}>Serial Number</th>
                <th style={thStyle}>Linked Aircraft</th>
                <th style={thStyle}>Status</th>
                <th style={thStyle}>Actions</th>
              </tr>
            </thead>
            <tbody>
              {visibleItems.map((c: Controller) => {
                const linked = findAircraft(c.aircraft_id);
                return (
                  <tr key={c.id} style={c.status === 'retired' ? retiredRowStyle : undefined}>
                    <td style={{ ...tdStyle, ...monoStyle }}>{c.equipment_id ? highlightMatch(c.equipment_id, debouncedQuery) : '—'}</td>
                    <td style={tdStyle}>{c.model ? highlightMatch(c.model, debouncedQuery) : '—'}</td>
                    <td style={tdStyle}>{c.compatible_models ? highlightMatch(c.compatible_models, debouncedQuery) : '—'}</td>
                    <td style={{ ...tdStyle, ...monoStyle }}>{highlightMatch(c.serial_number, debouncedQuery)}</td>
                    <td style={tdStyle}><AircraftCell aircraft={linked ?? null} /></td>
                    <td style={{ ...tdStyle, color: statusColor(c.status), fontWeight: 500 }}>{c.status}</td>
                    <td style={tdStyle}>
                      <Link
                        to={`/fleet-stats/controllers/${c.id}`}
                        style={{ ...btnSmall, textDecoration: 'none', display: 'inline-flex', alignItems: 'center', gap: 4 }}
                        title="View usage statistics"
                      >
                        <LineChartIcon size={12} /> Stats
                      </Link>
                      <button style={btnSmall} onClick={() => onEdit(c)}>Edit</button>
                      {c.status !== 'retired' && (
                        <button style={btnDanger} onClick={() => setRetireTarget(c)}>Retire</button>
                      )}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        ),
      };
    });

  const hasActiveQuery = debouncedQuery.trim().length > 0;

  return (
    <div>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start', gap: 'var(--space-4)', marginBottom: 'var(--space-4)' }}>
        <div style={{ flex: '1 1 auto', minWidth: 0 }}>
          <FleetSearchBar
            value={searchQuery}
            onChange={onSearchChange}
            matchCount={filteredItems.length}
            totalCount={allItems.length}
          />
        </div>
        {canEdit && <button onClick={onAdd} style={addBtnStyle}>Add Controller</button>}
      </div>
      {hasActiveQuery && filteredItems.length === 0 ? (
        <EmptyState message="No controllers match your search." hint="Try a different serial number, equipment ID, model, or nickname." />
      ) : (
        <ModelAccordion
          groups={accordionGroups}
          storageKey="fleet-controller-accordion"
          retiredCounts={retiredCounts}
          retiredVisible={retiredVisible}
          onRetiredToggle={key => setRetiredVisible(prev => ({ ...prev, [key]: !prev[key] }))}
          forceExpand={hasActiveQuery}
        />
      )}
      <RetireModal
        isOpen={retireTarget !== null}
        entityType="controller"
        entityLabel={retireTarget ? `${retireTarget.equipment_id ?? ''} — ${retireTarget.model ?? retireTarget.serial_number}`.replace(/^ — /, '') : ''}
        onConfirm={notes => retireTarget && retireMutation.mutate({ id: retireTarget.id, notes })}
        onCancel={() => setRetireTarget(null)}
        isSubmitting={retireMutation.isPending}
      />
    </div>
  );
}

// --- Accessories Table ---
function AccessoriesTab({ onAdd, onEdit, aircraft, canEdit, searchQuery, onSearchChange }: { onAdd: () => void; onEdit: (a: Accessory) => void; aircraft: Aircraft[]; canEdit: boolean; searchQuery: string; onSearchChange: (v: string) => void }) {
  const queryClient = useQueryClient();
  const { data: accessories, isLoading, error } = useQuery({ queryKey: ['accessories', true], queryFn: () => fetchAccessories(true) });
  const [retireTarget, setRetireTarget] = useState<Accessory | null>(null);
  const [retiredVisible, setRetiredVisible] = useState<Record<string, boolean>>({});
  const debouncedQuery = useDebouncedValue(searchQuery, 200);
  const findAircraft = (id: number | null) => aircraft.find(a => a.id === id);

  const retireMutation = useMutation({
    mutationFn: ({ id, notes }: { id: number; notes: string }) =>
      updateAccessory(id, { status: 'retired', retirement_notes: notes || undefined }),
    onSuccess: () => { queryClient.invalidateQueries({ queryKey: ['accessories', true] }); setRetireTarget(null); },
  });

  const allItems = useMemo(() => accessories ?? [], [accessories]);

  const filteredItems = useMemo(() => {
    const q = debouncedQuery.trim().toLowerCase();
    if (!q) return allItems;
    return allItems.filter(a =>
      a.name.toLowerCase().includes(q)
      || (a.equipment_id ?? '').toLowerCase().includes(q)
      || (a.serial_number ?? '').toLowerCase().includes(q)
      || (a.manufacturer ?? '').toLowerCase().includes(q)
      || (a.category ?? '').toLowerCase().includes(q),
    );
  }, [allItems, debouncedQuery]);

  if (isLoading) return <p style={{ color: 'var(--color-text-secondary)' }}>Loading accessories...</p>;
  if (error) return <p style={{ color: 'var(--color-critical)' }}>Failed to load accessories: {(error as Error).message}</p>;

  if (allItems.length === 0) {
    return (
      <div>
        <div style={{ display: 'flex', justifyContent: 'flex-end', marginBottom: 'var(--space-4)' }}>
          {canEdit && <button onClick={onAdd} style={addBtnStyle}>Add Accessory</button>}
        </div>
        <EmptyState message="No accessories registered yet." hint="Add cameras, gimbals, payloads, mounts or sensors to track their weight." />
      </div>
    );
  }

  const groups = groupByField(filteredItems, a => a.category, 'Uncategorised', 'uncategorised');
  const retiredCounts: Record<string, number> = {};
  groups.forEach(g => { retiredCounts[g.key] = g.items.filter(a => a.status === 'retired').length; });

  const accordionGroups: AccordionGroup[] = groups
    .filter(g => g.items.some(a => a.status !== 'retired') || retiredVisible[g.key])
    .map(g => {
      const visibleItems = retiredVisible[g.key] ? g.items : g.items.filter(a => a.status !== 'retired');
      return {
        groupKey: g.key,
        label: g.label,
        count: g.items.filter(a => a.status !== 'retired').length,
        statusSummary: buildStatusSummary(g.items.filter(a => a.status !== 'retired')),
        children: (
          <table style={tableStyle}>
            <thead>
              <tr>
                <th style={thStyle}>Equipment ID</th>
                <th style={thStyle}>Name</th>
                <th style={thStyle}>Category</th>
                <th style={thStyle}>Manufacturer</th>
                <th style={thStyle}>Serial Number</th>
                <th style={thStyle}>Aircraft</th>
                <th style={thStyle}>Weight</th>
                <th style={thStyle}>Status</th>
                <th style={thStyle}>Actions</th>
              </tr>
            </thead>
            <tbody>
              {visibleItems.map((a: Accessory) => {
                const linked = findAircraft(a.aircraft_id);
                return (
                <tr key={a.id} style={a.status === 'retired' ? retiredRowStyle : undefined}>
                  <td style={{ ...tdStyle, ...monoStyle }}>{a.equipment_id ? highlightMatch(a.equipment_id, debouncedQuery) : '—'}</td>
                  <td style={tdStyle}>{highlightMatch(a.name, debouncedQuery)}</td>
                  <td style={tdStyle}>{a.category ? highlightMatch(a.category, debouncedQuery) : '—'}</td>
                  <td style={tdStyle}>{a.manufacturer ? highlightMatch(a.manufacturer, debouncedQuery) : '—'}</td>
                  <td style={{ ...tdStyle, ...monoStyle }}>{a.serial_number ? highlightMatch(a.serial_number, debouncedQuery) : '—'}</td>
                  <td style={tdStyle}><AircraftCell aircraft={linked ?? null} /></td>
                  <td style={{ ...tdStyle, ...monoStyle }}>{a.weight_kg != null ? `${a.weight_kg.toFixed(2)} kg` : '—'}</td>
                  <td style={{ ...tdStyle, color: statusColor(a.status), fontWeight: 500 }}>{a.status}</td>
                  <td style={tdStyle}>
                    {canEdit && <button style={btnSmall} onClick={() => onEdit(a)}>Edit</button>}
                    {canEdit && a.status !== 'retired' && (
                      <button style={btnDanger} onClick={() => setRetireTarget(a)}>Retire</button>
                    )}
                  </td>
                </tr>
              );})}
            </tbody>
          </table>
        ),
      };
    });

  const hasActiveQuery = debouncedQuery.trim().length > 0;

  return (
    <div>
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'flex-start', gap: 'var(--space-4)', marginBottom: 'var(--space-4)' }}>
        <div style={{ flex: '1 1 auto', minWidth: 0 }}>
          <FleetSearchBar
            value={searchQuery}
            onChange={onSearchChange}
            matchCount={filteredItems.length}
            totalCount={allItems.length}
          />
        </div>
        {canEdit && <button onClick={onAdd} style={addBtnStyle}>Add Accessory</button>}
      </div>
      {hasActiveQuery && filteredItems.length === 0 ? (
        <EmptyState message="No accessories match your search." hint="Try a different name, equipment ID, serial number, manufacturer, or category." />
      ) : (
        <ModelAccordion
          groups={accordionGroups}
          storageKey="fleet-accessory-accordion"
          retiredCounts={retiredCounts}
          retiredVisible={retiredVisible}
          onRetiredToggle={key => setRetiredVisible(prev => ({ ...prev, [key]: !prev[key] }))}
          forceExpand={hasActiveQuery}
        />
      )}
      <RetireModal
        isOpen={retireTarget !== null}
        entityType="accessory"
        entityLabel={retireTarget ? `${retireTarget.equipment_id ?? ''} — ${retireTarget.name}`.replace(/^ — /, '') : ''}
        onConfirm={notes => retireTarget && retireMutation.mutate({ id: retireTarget.id, notes })}
        onCancel={() => setRetireTarget(null)}
        isSubmitting={retireMutation.isPending}
      />
    </div>
  );
}

// --- Add Aircraft Modal ---
function AddAircraftModal({ onClose }: { onClose: () => void }) {
  const queryClient = useQueryClient();
  const [serialNumber, setSerialNumber] = useState('');
  const [modelId, setModelId] = useState<string>('');
  const [nickname, setNickname] = useState('');
  const [equipmentId, setEquipmentId] = useState('');
  const [error, setError] = useState('');

  // Only active models — retired ones should not be selectable for new aircraft.
  const { data: models } = useQuery({
    queryKey: ['aircraft-models', 'active'],
    queryFn: () => fetchAircraftModels(true),
  });

  const mutation = useMutation({
    mutationFn: (data: CreateAircraftRequest) => createAircraft(data),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['aircraft'] });
      onClose();
    },
    onError: (err: Error) => setError(err.message),
  });

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (!serialNumber.trim()) { setError('Serial number is required'); return; }
    setError('');
    mutation.mutate({
      serial_number: serialNumber.trim(),
      model_id: modelId ? Number(modelId) : undefined,
      nickname: nickname.trim() || undefined,
      equipment_id: equipmentId.trim() || undefined,
    });
  };

  return (
    <div style={overlayStyle} onClick={onClose}>
      <div style={modalStyle} onClick={e => e.stopPropagation()}>
        <h2 style={{ fontFamily: 'var(--font-display)', fontSize: '1.1rem', color: 'var(--color-text-primary)', marginBottom: 'var(--space-5)' }}>Add Aircraft</h2>
        <form onSubmit={handleSubmit}>
          <div style={fieldStyle}><label style={labelStyle}>Serial Number *</label><input style={inputStyle} value={serialNumber} onChange={e => setSerialNumber(e.target.value)} placeholder="e.g. 1ZNBJ1AB0K0000123" /></div>
          <div style={fieldStyle}>
            <label style={labelStyle}>Model</label>
            <select style={inputStyle} value={modelId} onChange={e => setModelId(e.target.value)}>
              <option value="">— Select model —</option>
              {(models ?? []).map(m => (
                <option key={m.id} value={m.id}>{m.name}</option>
              ))}
            </select>
            <div style={{ fontSize: '0.72rem', color: 'var(--color-text-muted)', marginTop: 2 }}>
              Missing a model? An admin can add one on the Aircraft Models page.
            </div>
          </div>
          <div style={fieldStyle}><label style={labelStyle}>Nickname</label><input style={inputStyle} value={nickname} onChange={e => setNickname(e.target.value)} placeholder="e.g. Alpha" /></div>
          <div style={fieldStyle}><label style={labelStyle}>Equipment ID</label><input style={inputStyle} value={equipmentId} onChange={e => setEquipmentId(e.target.value)} placeholder="e.g. UAV-001" /></div>
          {error && <p style={{ color: 'var(--color-critical)', fontSize: '0.8rem', marginBottom: 'var(--space-3)' }}>{error}</p>}
          <div style={{ display: 'flex', gap: 'var(--space-3)', justifyContent: 'flex-end' }}>
            <button type="button" onClick={onClose} style={cancelBtnStyle}>Cancel</button>
            <button type="submit" disabled={mutation.isPending} style={submitBtnStyle(mutation.isPending)}>{mutation.isPending ? 'Adding...' : 'Add Aircraft'}</button>
          </div>
        </form>
      </div>
    </div>
  );
}

// --- Add Battery Modal ---
function AddBatteryModal({ onClose, aircraft }: { onClose: () => void; aircraft: Aircraft[] }) {
  const queryClient = useQueryClient();
  const [serialNumber, setSerialNumber] = useState('');
  const [equipmentId, setEquipmentId] = useState('');
  const [aircraftId, setAircraftId] = useState('');
  const [compatibleModelId, setCompatibleModelId] = useState('');
  const [maxCycles, setMaxCycles] = useState('');
  const [capacityWh, setCapacityWh] = useState('');
  const [weightKg, setWeightKg] = useState('');
  const [error, setError] = useState('');

  // Only active models — retired ones should not be selectable for new batteries.
  const { data: models } = useQuery({
    queryKey: ['aircraft-models', 'active'],
    queryFn: () => fetchAircraftModels(true),
  });

  const mutation = useMutation({
    mutationFn: (data: CreateBatteryRequest) => createBattery(data),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['batteries'] });
      onClose();
    },
    onError: (err: Error) => setError(err.message),
  });

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (!serialNumber.trim()) { setError('Serial number is required'); return; }
    const trimmedWeight = weightKg.trim();
    if (trimmedWeight !== '') {
      const w = Number(trimmedWeight);
      if (isNaN(w) || w <= 0) { setError('Weight must be a number greater than 0.'); return; }
    }
    setError('');
    // When a compatible model is chosen, also send its name as the legacy
    // free-text compatible_model so display sites reading the text column
    // stay coherent with the structured choice.
    const selectedModel = compatibleModelId
      ? models?.find(m => m.id === Number(compatibleModelId))
      : undefined;
    mutation.mutate({
      serial_number: serialNumber.trim(),
      equipment_id: equipmentId.trim() || undefined,
      aircraft_id: aircraftId ? Number(aircraftId) : undefined,
      compatible_model_id: compatibleModelId ? Number(compatibleModelId) : undefined,
      compatible_model: selectedModel ? selectedModel.name : undefined,
      max_cycles: maxCycles ? Number(maxCycles) : undefined,
      capacity_wh: capacityWh ? Number(capacityWh) : undefined,
      weight_kg: trimmedWeight === '' ? undefined : Number(trimmedWeight),
    });
  };

  return (
    <div style={overlayStyle} onClick={onClose}>
      <div style={modalStyle} onClick={e => e.stopPropagation()}>
        <h2 style={{ fontFamily: 'var(--font-display)', fontSize: '1.1rem', color: 'var(--color-text-primary)', marginBottom: 'var(--space-5)' }}>Add Battery</h2>
        <form onSubmit={handleSubmit}>
          <div style={fieldStyle}><label style={labelStyle}>Serial Number *</label><input style={inputStyle} value={serialNumber} onChange={e => setSerialNumber(e.target.value)} placeholder="e.g. 3NBJH12345678" /></div>
          <div style={fieldStyle}><label style={labelStyle}>Equipment ID</label><input style={inputStyle} value={equipmentId} onChange={e => setEquipmentId(e.target.value)} placeholder="BAT-001" /></div>
          <div style={fieldStyle}>
            <label style={labelStyle}>Linked Aircraft</label>
            <select style={inputStyle} value={aircraftId} onChange={e => setAircraftId(e.target.value)}>
              <option value="">— None —</option>
              {aircraft.map(a => <option key={a.id} value={a.id}>{formatAircraftLabel(a)}</option>)}
            </select>
          </div>
          <div style={fieldStyle}>
            <label style={labelStyle}>Compatible Model</label>
            <select style={inputStyle} value={compatibleModelId} onChange={e => setCompatibleModelId(e.target.value)}>
              <option value="">— None —</option>
              {(models ?? []).map(m => <option key={m.id} value={m.id}>{m.name}</option>)}
            </select>
          </div>
          <div style={fieldStyle}><label style={labelStyle}>Max Cycles</label><input style={inputStyle} type="number" value={maxCycles} onChange={e => setMaxCycles(e.target.value)} placeholder="300" /></div>
          <div style={{ display: 'flex', gap: 'var(--space-4)', overflow: 'hidden' }}>
            <div style={{ ...fieldStyle, flex: '1 1 0', minWidth: 0 }}><label style={labelStyle}>Capacity Wh</label><input style={inputStyle} type="number" step="0.1" value={capacityWh} onChange={e => setCapacityWh(e.target.value)} placeholder="e.g. 43.6" /></div>
            <div style={{ ...fieldStyle, flex: '1 1 0', minWidth: 0 }}><label style={labelStyle}>Weight (kg)</label><input style={inputStyle} type="number" step="0.01" value={weightKg} onChange={e => setWeightKg(e.target.value)} placeholder="e.g. 0.57" /></div>
          </div>
          {error && <p style={{ color: 'var(--color-critical)', fontSize: '0.8rem', marginBottom: 'var(--space-3)' }}>{error}</p>}
          <div style={{ display: 'flex', gap: 'var(--space-3)', justifyContent: 'flex-end' }}>
            <button type="button" onClick={onClose} style={cancelBtnStyle}>Cancel</button>
            <button type="submit" disabled={mutation.isPending} style={submitBtnStyle(mutation.isPending)}>{mutation.isPending ? 'Adding...' : 'Add Battery'}</button>
          </div>
        </form>
      </div>
    </div>
  );
}

// --- Add Controller Modal ---
function AddControllerModal({ onClose, aircraft }: { onClose: () => void; aircraft: Aircraft[] }) {
  const queryClient = useQueryClient();
  const [serialNumber, setSerialNumber] = useState('');
  const [equipmentId, setEquipmentId] = useState('');
  const [model, setModel] = useState('');
  const [nickname, setNickname] = useState('');
  const [aircraftId, setAircraftId] = useState('');
  const [compatibleModels, setCompatibleModels] = useState('');
  const [error, setError] = useState('');

  const mutation = useMutation({
    mutationFn: (data: CreateControllerRequest) => createController(data),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['controllers'] });
      onClose();
    },
    onError: (err: Error) => setError(err.message),
  });

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (!serialNumber.trim()) { setError('Serial number is required'); return; }
    setError('');
    mutation.mutate({
      serial_number: serialNumber.trim(),
      equipment_id: equipmentId.trim() || undefined,
      model: model.trim() || undefined,
      nickname: nickname.trim() || undefined,
      aircraft_id: aircraftId ? Number(aircraftId) : undefined,
      compatible_models: compatibleModels.trim() || undefined,
    });
  };

  return (
    <div style={overlayStyle} onClick={onClose}>
      <div style={modalStyle} onClick={e => e.stopPropagation()}>
        <h2 style={{ fontFamily: 'var(--font-display)', fontSize: '1.1rem', color: 'var(--color-text-primary)', marginBottom: 'var(--space-5)' }}>Add Controller</h2>
        <form onSubmit={handleSubmit}>
          <div style={fieldStyle}><label style={labelStyle}>Serial Number *</label><input style={inputStyle} value={serialNumber} onChange={e => setSerialNumber(e.target.value)} placeholder="e.g. RC-1234567" /></div>
          <div style={fieldStyle}><label style={labelStyle}>Equipment ID</label><input style={inputStyle} value={equipmentId} onChange={e => setEquipmentId(e.target.value)} placeholder="CTL-001" /></div>
          <div style={fieldStyle}><label style={labelStyle}>Model</label><input style={inputStyle} value={model} onChange={e => setModel(e.target.value)} placeholder="e.g. DJI RC Pro" /></div>
          <div style={fieldStyle}>
            <label style={labelStyle}>Compatible Models</label>
            <input style={inputStyle} value={compatibleModels} onChange={e => setCompatibleModels(e.target.value)} placeholder="e.g. Mavic 3, Mini 4 Pro" />
            <span style={{ fontSize: '0.7rem', color: 'var(--color-text-muted)', marginTop: '2px' }}>List aircraft models this controller supports, separated by commas.</span>
          </div>
          <div style={fieldStyle}><label style={labelStyle}>Nickname</label><input style={inputStyle} value={nickname} onChange={e => setNickname(e.target.value)} placeholder="e.g. Bravo RC" /></div>
          <div style={fieldStyle}>
            <label style={labelStyle}>Linked Aircraft</label>
            <select style={inputStyle} value={aircraftId} onChange={e => setAircraftId(e.target.value)}>
              <option value="">— None —</option>
              {aircraft.map(a => <option key={a.id} value={a.id}>{formatAircraftLabel(a)}</option>)}
            </select>
          </div>
          {error && <p style={{ color: 'var(--color-critical)', fontSize: '0.8rem', marginBottom: 'var(--space-3)' }}>{error}</p>}
          <div style={{ display: 'flex', gap: 'var(--space-3)', justifyContent: 'flex-end' }}>
            <button type="button" onClick={onClose} style={cancelBtnStyle}>Cancel</button>
            <button type="submit" disabled={mutation.isPending} style={submitBtnStyle(mutation.isPending)}>{mutation.isPending ? 'Adding...' : 'Add Controller'}</button>
          </div>
        </form>
      </div>
    </div>
  );
}

// --- Edit Aircraft Modal ---
function EditAircraftModal({ aircraft, allAircraft, onClose }: { aircraft: Aircraft; allAircraft: Aircraft[]; onClose: () => void }) {
  const queryClient = useQueryClient();
  const [nickname, setNickname] = useState(aircraft.nickname ?? '');
  const [modelId, setModelId] = useState<string>(aircraft.model_id != null ? String(aircraft.model_id) : '');
  const [equipmentId, setEquipmentId] = useState(aircraft.equipment_id ?? '');
  const [status, setStatus] = useState(aircraft.status);
  const [notes, setNotes] = useState(aircraft.retirement_notes ?? '');
  const [emptyWeight, setEmptyWeight] = useState(
    aircraft.empty_weight_kg != null ? String(aircraft.empty_weight_kg) : ''
  );
  // SN is only editable for rows the QR scanner couldn't extract a real SN
  // from (Tello, etc.). For everyone else it stays read-only — SN is a stable
  // identity field and admins shouldn't normally rewrite it.
  const [serialNumber, setSerialNumber] = useState(aircraft.serial_number);
  const [error, setError] = useState('');
  const [mergeOpen, setMergeOpen] = useState(false);

  // Show active models plus the aircraft's current model even if retired,
  // so editing an aircraft that was assigned a now-retired model doesn't
  // lose the selection (but adding NEW aircraft can't pick retired ones).
  const { data: models } = useQuery({
    queryKey: ['aircraft-models', 'active-plus-current', aircraft.model_id],
    queryFn: async () => {
      const active = await fetchAircraftModels(true);
      if (aircraft.model_id != null && !active.some(m => m.id === aircraft.model_id)) {
        const all = await fetchAircraftModels();
        const current = all.find(m => m.id === aircraft.model_id);
        return current ? [...active, current] : active;
      }
      return active;
    },
  });

  const mutation = useMutation({
    mutationFn: (data: object) => updateAircraft(aircraft.id, data as import('../types/aircraft').UpdateAircraftRequest),
    onSuccess: () => { queryClient.invalidateQueries({ queryKey: ['aircraft'] }); onClose(); },
    onError: (err: Error) => setError(err.message),
  });

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    setError('');
    const trimmedSn = serialNumber.trim();
    if (aircraft.needs_manual_sn && !trimmedSn) {
      setError('Serial number is required.');
      return;
    }
    const trimmedWeight = emptyWeight.trim();
    if (trimmedWeight !== '') {
      const w = Number(trimmedWeight);
      if (isNaN(w) || w <= 0) {
        setError('Empty weight must be a number greater than 0.');
        return;
      }
    }
    mutation.mutate({
      nickname: nickname.trim() || null,
      // Clearing the dropdown (empty string) sends null → backend keeps the
      // existing model_id. Pass 0/undefined if you need to explicitly detach.
      model_id: modelId ? Number(modelId) : undefined,
      equipment_id: equipmentId.trim() || null,
      status,
      retirement_notes: notes.trim() || null,
      // Empty field → undefined so COALESCE keeps the existing value (same
      // limitation as the other fields here: this modal cannot null a value).
      empty_weight_kg: trimmedWeight === '' ? undefined : Number(trimmedWeight),
      // Only send serial_number when the user actually changed it. The
      // backend auto-clears needs_manual_sn iff SN changes — silently
      // round-tripping the same value would not trigger the clear.
      serial_number:
        aircraft.needs_manual_sn && trimmedSn !== aircraft.serial_number
          ? trimmedSn
          : undefined,
    });
  };

  return (
    <div style={overlayStyle} onClick={onClose}>
      <div style={modalStyle} onClick={e => e.stopPropagation()}>
        <h2 style={{ fontFamily: 'var(--font-display)', fontSize: '1.1rem', color: 'var(--color-text-primary)', marginBottom: 'var(--space-5)' }}>Edit Aircraft</h2>
        {aircraft.needs_manual_sn && (
          <div style={needsManualSnBannerStyle} role="alert">
            <strong>Manual SN correction needed.</strong> The serial number for
            this aircraft was not extracted from the QR scan. Replace the
            placeholder below with the actual SN from the device label — the
            warning will clear once you save.
          </div>
        )}
        <form onSubmit={handleSubmit}>
          <div style={fieldStyle}>
            <label style={labelStyle}>Serial Number</label>
            {aircraft.needs_manual_sn ? (
              <input
                style={inputStyle}
                value={serialNumber}
                onChange={e => setSerialNumber(e.target.value)}
                placeholder="Enter the SN printed on the device"
                required
              />
            ) : (
              <input style={{ ...inputStyle, opacity: 0.6 }} value={aircraft.serial_number} readOnly />
            )}
          </div>
          {aircraft.needs_manual_sn && (
            <div
              style={{
                borderTop: '1px dashed var(--color-border)',
                paddingTop: 'var(--space-3)',
                marginBottom: 'var(--space-4)',
                display: 'flex',
                flexDirection: 'column',
                gap: 'var(--space-2)',
              }}
            >
              <span style={{ ...labelStyle, color: 'var(--color-text-muted)' }}>
                Or, if this is a duplicate of an existing aircraft:
              </span>
              <button
                type="button"
                onClick={() => setMergeOpen(true)}
                style={{
                  alignSelf: 'flex-start',
                  background: 'transparent',
                  color: 'var(--color-warning)',
                  border: '1px solid var(--color-warning)',
                  borderRadius: 'var(--radius-sm)',
                  padding: 'var(--space-2) var(--space-4)',
                  cursor: 'pointer',
                  fontSize: '0.85rem',
                  fontWeight: 500,
                }}
              >
                Merge into existing aircraft
              </button>
            </div>
          )}
          <div style={fieldStyle}><label style={labelStyle}>Nickname</label><input style={inputStyle} value={nickname} onChange={e => setNickname(e.target.value)} placeholder="e.g. Alpha" /></div>
          <div style={fieldStyle}>
            <label style={labelStyle}>Model</label>
            <select style={inputStyle} value={modelId} onChange={e => setModelId(e.target.value)}>
              <option value="">— Select model —</option>
              {(models ?? []).map(m => (
                <option key={m.id} value={m.id}>{m.name}{m.is_active ? '' : ' (retired)'}</option>
              ))}
            </select>
          </div>
          <div style={fieldStyle}><label style={labelStyle}>Equipment ID</label><input style={inputStyle} value={equipmentId} onChange={e => setEquipmentId(e.target.value)} placeholder="e.g. UAV-001" /></div>
          <div style={fieldStyle}><label style={labelStyle}>Empty weight (kg)</label><input style={inputStyle} value={emptyWeight} onChange={e => setEmptyWeight(e.target.value)} inputMode="decimal" placeholder="Measured airframe weight, e.g. 0.63" /></div>
          <div style={fieldStyle}>
            <label style={labelStyle}>Status</label>
            <select style={inputStyle} value={status} onChange={e => setStatus(e.target.value as "active" | "maintenance" | "retired")}>
              <option value="active">Active</option>
              <option value="maintenance">Maintenance</option>
              <option value="retired">Retired</option>
            </select>
          </div>
          <div style={fieldStyle}><label style={labelStyle}>Notes</label><textarea style={{ ...inputStyle, minHeight: '50px', resize: 'vertical' }} value={notes} onChange={e => setNotes(e.target.value)} placeholder="Optional notes..." /></div>
          {error && <p style={{ color: 'var(--color-critical)', fontSize: '0.8rem', marginBottom: 'var(--space-3)' }}>{error}</p>}
          <div style={{ display: 'flex', gap: 'var(--space-3)', justifyContent: 'flex-end' }}>
            <button type="button" onClick={onClose} style={cancelBtnStyle}>Cancel</button>
            <button type="submit" disabled={mutation.isPending} style={submitBtnStyle(mutation.isPending)}>{mutation.isPending ? 'Saving...' : 'Save Changes'}</button>
          </div>
        </form>
      </div>
      {mergeOpen && (
        <MergeAircraftModal
          source={aircraft}
          allAircraft={allAircraft}
          onClose={() => setMergeOpen(false)}
          onSuccessClose={() => { setMergeOpen(false); onClose(); }}
        />
      )}
    </div>
  );
}

// --- Edit Battery Modal ---
function EditBatteryModal({ battery, aircraft, onClose }: { battery: Battery; aircraft: Aircraft[]; onClose: () => void }) {
  const queryClient = useQueryClient();
  const [equipmentId, setEquipmentId] = useState(battery.equipment_id ?? '');
  const [aircraftId, setAircraftId] = useState(battery.aircraft_id ? String(battery.aircraft_id) : '');
  const [compatibleModelId, setCompatibleModelId] = useState(battery.compatible_model_id != null ? String(battery.compatible_model_id) : '');
  const [maxCycles, setMaxCycles] = useState(String(battery.max_cycles));
  const [capacityWh, setCapacityWh] = useState(battery.capacity_wh ? String(battery.capacity_wh) : '');
  const [weightKg, setWeightKg] = useState(battery.weight_kg != null ? String(battery.weight_kg) : '');
  const [status, setStatus] = useState(battery.status);
  const [error, setError] = useState('');

  // Show active models plus the battery's current compatible model even if
  // retired, so editing a battery assigned a now-retired model doesn't lose
  // the selection (mirrors the Edit Aircraft modal's model dropdown).
  const { data: models } = useQuery({
    queryKey: ['aircraft-models', 'active-plus-current', battery.compatible_model_id],
    queryFn: async () => {
      const active = await fetchAircraftModels(true);
      if (battery.compatible_model_id != null && !active.some(m => m.id === battery.compatible_model_id)) {
        const all = await fetchAircraftModels();
        const current = all.find(m => m.id === battery.compatible_model_id);
        return current ? [...active, current] : active;
      }
      return active;
    },
  });

  const mutation = useMutation({
    mutationFn: (data: object) => updateBattery(battery.id, data as Partial<CreateBatteryRequest>),
    onSuccess: () => { queryClient.invalidateQueries({ queryKey: ['batteries'] }); onClose(); },
    onError: (err: Error) => setError(err.message),
  });

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    const trimmedWeight = weightKg.trim();
    if (trimmedWeight !== '') {
      const w = Number(trimmedWeight);
      if (isNaN(w) || w <= 0) { setError('Weight must be a number greater than 0.'); return; }
    }
    setError('');
    // When a model IS selected, also sync the legacy free-text
    // compatible_model to its name so text-reading displays stay coherent.
    // When "— None —" is selected, only the FK is cleared — the log-derived
    // text label remains untouched.
    const selectedModel = compatibleModelId
      ? models?.find(m => m.id === Number(compatibleModelId))
      : undefined;
    mutation.mutate({
      equipment_id: equipmentId.trim() || null,
      aircraft_id: aircraftId ? Number(aircraftId) : null,
      compatible_model_id: compatibleModelId ? Number(compatibleModelId) : null,
      ...(selectedModel ? { compatible_model: selectedModel.name } : {}),
      max_cycles: maxCycles ? Number(maxCycles) : undefined,
      capacity_wh: capacityWh ? Number(capacityWh) : undefined,
      // Empty field → undefined so COALESCE keeps the existing weight (this
      // modal cannot null a value, matching the other numeric fields here).
      weight_kg: trimmedWeight === '' ? undefined : Number(trimmedWeight),
      status,
    });
  };

  return (
    <div style={overlayStyle} onClick={onClose}>
      <div style={modalStyle} onClick={e => e.stopPropagation()}>
        <h2 style={{ fontFamily: 'var(--font-display)', fontSize: '1.1rem', color: 'var(--color-text-primary)', marginBottom: 'var(--space-5)' }}>Edit Battery</h2>
        <form onSubmit={handleSubmit}>
          <div style={fieldStyle}><label style={labelStyle}>Serial Number</label><input style={{ ...inputStyle, opacity: 0.6 }} value={battery.serial_number} readOnly /></div>
          <div style={fieldStyle}><label style={labelStyle}>Equipment ID</label><input style={inputStyle} value={equipmentId} onChange={e => setEquipmentId(e.target.value)} placeholder="BAT-001" /></div>
          <div style={fieldStyle}>
            <label style={labelStyle}>Linked Aircraft</label>
            <select style={inputStyle} value={aircraftId} onChange={e => setAircraftId(e.target.value)}>
              <option value="">— None —</option>
              {aircraft.map(a => <option key={a.id} value={a.id}>{formatAircraftLabel(a)}</option>)}
            </select>
          </div>
          <div style={fieldStyle}>
            <label style={labelStyle}>Compatible Model</label>
            <select style={inputStyle} value={compatibleModelId} onChange={e => setCompatibleModelId(e.target.value)}>
              <option value="">— None —</option>
              {(models ?? []).map(m => <option key={m.id} value={m.id}>{m.name}</option>)}
            </select>
          </div>
          <div style={{ display: 'flex', gap: 'var(--space-4)', overflow: 'hidden' }}>
            <div style={{ ...fieldStyle, flex: '1 1 0', minWidth: 0 }}><label style={labelStyle}>Max Cycles</label><input style={inputStyle} type="number" value={maxCycles} onChange={e => setMaxCycles(e.target.value)} /></div>
            <div style={{ ...fieldStyle, flex: '1 1 0', minWidth: 0 }}><label style={labelStyle}>Capacity Wh</label><input style={inputStyle} type="number" step="0.1" value={capacityWh} onChange={e => setCapacityWh(e.target.value)} /></div>
            <div style={{ ...fieldStyle, flex: '1 1 0', minWidth: 0 }}><label style={labelStyle}>Weight (kg)</label><input style={inputStyle} type="number" step="0.01" value={weightKg} onChange={e => setWeightKg(e.target.value)} placeholder="e.g. 0.57" /></div>
          </div>
          <div style={fieldStyle}>
            <label style={labelStyle}>Status</label>
            <select style={inputStyle} value={status} onChange={e => setStatus(e.target.value as "active" | "maintenance" | "retired")}>
              <option value="active">Active</option>
              <option value="retired">Retired</option>
            </select>
          </div>
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

// --- Edit Controller Modal ---
function EditControllerModal({ controller, aircraft, onClose }: { controller: Controller; aircraft: Aircraft[]; onClose: () => void }) {
  const queryClient = useQueryClient();
  const [nickname, setNickname] = useState(controller.nickname ?? '');
  const [model, setModel] = useState(controller.model ?? '');
  const [equipmentId, setEquipmentId] = useState(controller.equipment_id ?? '');
  const [aircraftId, setAircraftId] = useState(controller.aircraft_id ? String(controller.aircraft_id) : '');
  const [compatibleModels, setCompatibleModels] = useState(controller.compatible_models ?? '');
  const [status, setStatus] = useState(controller.status);
  const [notes, setNotes] = useState(controller.notes ?? '');
  const [error, setError] = useState('');

  const mutation = useMutation({
    mutationFn: (data: object) => updateController(controller.id, data as Partial<CreateControllerRequest>),
    onSuccess: () => { queryClient.invalidateQueries({ queryKey: ['controllers'] }); onClose(); },
    onError: (err: Error) => setError(err.message),
  });

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    setError('');
    mutation.mutate({
      nickname: nickname.trim() || null,
      model: model.trim() || null,
      equipment_id: equipmentId.trim() || null,
      aircraft_id: aircraftId ? Number(aircraftId) : null,
      compatible_models: compatibleModels.trim() || null,
      status,
      notes: notes.trim() || null,
    });
  };

  return (
    <div style={overlayStyle} onClick={onClose}>
      <div style={modalStyle} onClick={e => e.stopPropagation()}>
        <h2 style={{ fontFamily: 'var(--font-display)', fontSize: '1.1rem', color: 'var(--color-text-primary)', marginBottom: 'var(--space-5)' }}>Edit Controller</h2>
        <form onSubmit={handleSubmit}>
          <div style={fieldStyle}><label style={labelStyle}>Serial Number</label><input style={{ ...inputStyle, opacity: 0.6 }} value={controller.serial_number} readOnly /></div>
          <div style={fieldStyle}><label style={labelStyle}>Nickname</label><input style={inputStyle} value={nickname} onChange={e => setNickname(e.target.value)} /></div>
          <div style={fieldStyle}><label style={labelStyle}>Model</label><input style={inputStyle} value={model} onChange={e => setModel(e.target.value)} /></div>
          <div style={fieldStyle}>
            <label style={labelStyle}>Compatible Models</label>
            <input style={inputStyle} value={compatibleModels} onChange={e => setCompatibleModels(e.target.value)} placeholder="e.g. Mavic 3, Mini 4 Pro" />
            <span style={{ fontSize: '0.7rem', color: 'var(--color-text-muted)', marginTop: '2px' }}>List aircraft models this controller supports, separated by commas.</span>
          </div>
          <div style={fieldStyle}><label style={labelStyle}>Equipment ID</label><input style={inputStyle} value={equipmentId} onChange={e => setEquipmentId(e.target.value)} placeholder="CTL-001" /></div>
          <div style={fieldStyle}>
            <label style={labelStyle}>Linked Aircraft</label>
            <select style={inputStyle} value={aircraftId} onChange={e => setAircraftId(e.target.value)}>
              <option value="">— None —</option>
              {aircraft.map(a => <option key={a.id} value={a.id}>{formatAircraftLabel(a)}</option>)}
            </select>
          </div>
          <div style={fieldStyle}>
            <label style={labelStyle}>Status</label>
            <select style={inputStyle} value={status} onChange={e => setStatus(e.target.value as "active" | "maintenance" | "retired")}>
              <option value="active">Active</option>
              <option value="retired">Retired</option>
            </select>
          </div>
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

// --- Main Fleet Page ---
export function FleetPage() {
  const [activeTab, setActiveTab] = useState<Tab>('aircraft');
  const [showAddAircraft, setShowAddAircraft] = useState(false);
  const [showAddBattery, setShowAddBattery] = useState(false);
  const [showAddController, setShowAddController] = useState(false);
  const [showAddAccessory, setShowAddAccessory] = useState(false);
  const [editAircraft, setEditAircraft] = useState<Aircraft | null>(null);
  const [editBattery, setEditBattery] = useState<Battery | null>(null);
  const [editController, setEditController] = useState<Controller | null>(null);
  const [editAccessory, setEditAccessory] = useState<Accessory | null>(null);
  const [batchImportOpen, setBatchImportOpen] = useState(false);
  // Search query is hoisted here so it persists across tab switches — typing
  // a serial in Aircraft and switching to Batteries keeps the query.
  const [searchQuery, setSearchQuery] = useState('');

  const queryClient = useQueryClient();
  const { data: aircraft } = useQuery({ queryKey: ['aircraft', true], queryFn: () => fetchAircraft(true) });
  const aircraftList = aircraft ?? [];

  const { data: currentUser } = useCurrentUser();
  const canEdit = userHasAnyRole(currentUser, 'admin', 'technician');

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

  return (
    <div>
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', marginBottom: 'var(--space-4)' }}>
        <h1 style={{
          fontFamily: 'var(--font-display)',
          fontSize: '1.25rem',
          fontWeight: 500,
          color: 'var(--color-text-primary)',
          letterSpacing: '0.02em',
        }}>Fleet</h1>
        {canEdit && (
          <button
            onClick={() => setBatchImportOpen(true)}
            style={{
              background: 'transparent',
              color: 'var(--color-text-secondary)',
              border: '1px solid var(--color-border)',
              borderRadius: 'var(--radius-sm)',
              padding: 'var(--space-2) var(--space-3)',
              fontSize: '0.85rem',
              cursor: 'pointer',
              display: 'flex',
              alignItems: 'center',
              gap: 'var(--space-2)',
            }}
          >
            <Upload size={14} />
            Batch Import
          </button>
        )}
      </div>

      <div style={{
        display: 'flex',
        gap: 'var(--space-1)',
        borderBottom: '1px solid var(--color-border)',
        marginBottom: 'var(--space-5)',
      }}>
        <button style={tabStyle('aircraft')} onClick={() => setActiveTab('aircraft')}>Aircraft</button>
        <button style={tabStyle('batteries')} onClick={() => setActiveTab('batteries')}>Batteries</button>
        <button style={tabStyle('controllers')} onClick={() => setActiveTab('controllers')}>Controllers</button>
        <button style={tabStyle('accessories')} onClick={() => setActiveTab('accessories')}>Accessories</button>
      </div>

      {activeTab === 'aircraft' && <AircraftTab onAdd={() => setShowAddAircraft(true)} onEdit={a => setEditAircraft(a)} canEdit={canEdit} searchQuery={searchQuery} onSearchChange={setSearchQuery} />}
      {activeTab === 'batteries' && <BatteriesTab onAdd={() => setShowAddBattery(true)} onEdit={b => setEditBattery(b)} aircraft={aircraftList} canEdit={canEdit} searchQuery={searchQuery} onSearchChange={setSearchQuery} />}
      {activeTab === 'controllers' && <ControllersTab onAdd={() => setShowAddController(true)} onEdit={c => setEditController(c)} aircraft={aircraftList} canEdit={canEdit} searchQuery={searchQuery} onSearchChange={setSearchQuery} />}
      {activeTab === 'accessories' && <AccessoriesTab onAdd={() => setShowAddAccessory(true)} onEdit={a => setEditAccessory(a)} aircraft={aircraftList} canEdit={canEdit} searchQuery={searchQuery} onSearchChange={setSearchQuery} />}

      {showAddAircraft && <AddAircraftModal onClose={() => setShowAddAircraft(false)} />}
      {showAddBattery && <AddBatteryModal onClose={() => setShowAddBattery(false)} aircraft={aircraftList} />}
      {showAddController && <AddControllerModal onClose={() => setShowAddController(false)} aircraft={aircraftList} />}
      {showAddAccessory && <AccessoryFormModal aircraft={aircraftList} onClose={() => setShowAddAccessory(false)} />}
      {editAircraft && <EditAircraftModal aircraft={editAircraft} allAircraft={aircraftList} onClose={() => setEditAircraft(null)} />}
      {editBattery && <EditBatteryModal battery={editBattery} aircraft={aircraftList} onClose={() => setEditBattery(null)} />}
      {editController && <EditControllerModal controller={editController} aircraft={aircraftList} onClose={() => setEditController(null)} />}
      {editAccessory && <AccessoryFormModal accessory={editAccessory} aircraft={aircraftList} onClose={() => setEditAccessory(null)} />}

      <BatchImportModal
        isOpen={batchImportOpen}
        onClose={() => setBatchImportOpen(false)}
        onSuccess={() => {
          queryClient.invalidateQueries({ queryKey: ['aircraft'] });
          queryClient.invalidateQueries({ queryKey: ['batteries'] });
          queryClient.invalidateQueries({ queryKey: ['controllers'] });
        }}
        defaultEntityType={activeTab === 'batteries' ? 'battery' : activeTab === 'controllers' ? 'controller' : 'aircraft'}
      />
    </div>
  );
}
