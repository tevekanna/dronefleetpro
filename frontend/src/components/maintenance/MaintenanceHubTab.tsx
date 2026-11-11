// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useMemo, useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { useNavigate } from 'react-router-dom';
import {
  CalendarPlus,
  FilePlus2,
  ClipboardPlus,
  Archive,
  ChevronRight,
} from 'lucide-react';
import {
  fetchFleetMaintenanceStatus,
  type FleetMaintenanceStatus,
  type FleetMaintenanceRow,
} from '../../api/maintenanceAircraft';
import styles from './MaintenanceHubTab.module.css';

function isoDate(value: string | null): string {
  return value ? value.slice(0, 10) : '—';
}

type SortKey = 'equipment' | 'aircraft' | 'model' | 'airworthiness' | 'overdue' | 'due_soon' | 'defects' | 'next_due';
type SortDir = 'asc' | 'desc';
type StatFilter = 'airworthy' | 'not_airworthy' | 'overdue' | 'due_soon' | null;

function strCmp(a: string, b: string): number { return a.localeCompare(b); }
function dateCmp(a: string | null, b: string | null): number {
  // ISO strings compare chronologically; nulls sort last in ascending order.
  const av = a ?? '9999-12-31';
  const bv = b ?? '9999-12-31';
  return av.localeCompare(bv);
}
function compareRows(a: FleetMaintenanceRow, b: FleetMaintenanceRow, key: SortKey): number {
  switch (key) {
    case 'equipment':     return strCmp(a.equipment_id ?? a.serial_number, b.equipment_id ?? b.serial_number);
    case 'aircraft':      return strCmp(a.aircraft_name, b.aircraft_name);
    case 'model':         return strCmp(a.model ?? '', b.model ?? '');
    case 'airworthiness': return strCmp(a.airworthiness_status, b.airworthiness_status);
    case 'overdue':       return a.overdue_count - b.overdue_count;
    case 'due_soon':      return a.due_soon_count - b.due_soon_count;
    case 'defects':       return a.open_defect_count - b.open_defect_count;
    case 'next_due':      return dateCmp(a.next_due_at, b.next_due_at);
  }
}

function HubStatCard({ label, value, accent, highlight, active, onToggle }: {
  label: string;
  value: number;
  accent: 'ok' | 'critical' | 'warning' | 'info';
  highlight: boolean;
  active: boolean;
  onToggle: () => void;
}) {
  return (
    <button
      type="button"
      className={`${styles.statCard} ${styles[`statTop_${accent}`]} ${active ? styles.statCardActive : ''}`}
      aria-pressed={active}
      onClick={onToggle}
    >
      <span className={`${styles.statValue} ${highlight ? styles[`statValue_${accent}`] : ''}`}>
        {value}
      </span>
      <span className={styles.statLabel}>{label}</span>
    </button>
  );
}

function SortHeader({ label, colKey, align, sortKey, sortDir, onSort }: {
  label: string;
  colKey: SortKey;
  align: 'left' | 'right';
  sortKey: SortKey | null;
  sortDir: SortDir;
  onSort: (k: SortKey) => void;
}) {
  const active = sortKey === colKey;
  return (
    <th
      className={`${align === 'right' ? styles.thNumber : styles.thLeft} ${styles.thSortable} ${active ? styles.thSortActive : ''}`}
      aria-sort={active ? (sortDir === 'asc' ? 'ascending' : 'descending') : 'none'}
      onClick={() => onSort(colKey)}
    >
      <span className={styles.thInner}>
        {label}
        <span className={styles.sortCaret} aria-hidden>
          {active ? (sortDir === 'asc' ? '▲' : '▼') : ''}
        </span>
      </span>
    </th>
  );
}

function HubQuickActions() {
  const navigate = useNavigate();
  const actions = [
    { icon: CalendarPlus, label: 'New Schedule', sub: 'Recurring task', to: '/maintenance-schedules' },
    { icon: FilePlus2, label: 'New Template', sub: 'Model-level', to: '/maintenance-templates' },
    { icon: ClipboardPlus, label: 'Add Record', sub: 'Log work done', to: '/maintenance-records' },
    { icon: Archive, label: 'Retired', sub: 'Decommissioned', to: '/retired-equipment' },
  ];
  return (
    <div className={styles.quickGrid}>
      {actions.map(({ icon: Icon, label, sub, to }) => (
        <button key={to} className={styles.quickCard} onClick={() => navigate(to)}>
          <Icon size={18} className={styles.quickIcon} />
          <span className={styles.quickLabel}>{label}</span>
          <span className={styles.quickSub}>{sub}</span>
        </button>
      ))}
    </div>
  );
}

function HubSkeleton() {
  return (
    <div className={styles.wrapper}>
      <div className={styles.statRow}>
        {[0, 1, 2, 3].map((i) => (
          <div key={i} className={`${styles.skeleton} ${styles.skeletonStat}`} />
        ))}
      </div>
      <div className={`${styles.skeleton} ${styles.skeletonPanel}`} />
    </div>
  );
}

function FleetRow({ row }: { row: FleetMaintenanceRow }) {
  const navigate = useNavigate();
  // Link straight to the workspace maintenance tab (the /maintenance/aircraft/:id
  // redirect still exists for bookmarks, but the Hub no longer needs the hop).
  const target = `/aircraft/${row.aircraft_id}?tab=maintenance`;
  const onActivate = () => navigate(target);
  const onKey = (e: React.KeyboardEvent<HTMLTableRowElement>) => {
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      onActivate();
    }
  };

  const grounded = row.airworthiness_status === 'not_airworthy';
  const equipmentLabel = row.equipment_id ?? row.serial_number;

  const nextDue =
    row.next_due_label || row.next_due_at
      ? `${row.next_due_label ?? ''}${
          row.next_due_label && row.next_due_at ? ' · ' : ''
        }${isoDate(row.next_due_at)}`
      : '—';

  return (
    <tr
      className={`${styles.fleetRow} ${grounded ? styles.fleetRowGrounded : ''}`}
      onClick={onActivate}
      onKeyDown={onKey}
      tabIndex={0}
      role="link"
      aria-label={`Maintenance for ${equipmentLabel}`}
    >
      <td className={styles.cellMono}>{equipmentLabel}</td>
      <td className={styles.cellName}>{row.aircraft_name}</td>
      <td className={styles.cellMuted}>{row.model ?? '—'}</td>
      <td>
        <span
          className={`${styles.airBadge} ${
            grounded ? styles.airBadgeCritical : styles.airBadgeOk
          }`}
        >
          {grounded ? 'NOT AIRWORTHY' : 'AIRWORTHY'}
        </span>
      </td>
      <td className={styles.cellNumber}>
        {row.overdue_count > 0 ? (
          <span className={styles.numCritical}>{row.overdue_count}</span>
        ) : (
          <span className={styles.numMuted}>—</span>
        )}
      </td>
      <td className={styles.cellNumber}>
        {row.due_soon_count > 0 ? (
          <span className={styles.numWarning}>{row.due_soon_count}</span>
        ) : (
          <span className={styles.numMuted}>—</span>
        )}
      </td>
      <td className={styles.cellNumber}>
        {row.open_defect_count > 0 ? (
          <span className={styles.numCritical}>{row.open_defect_count}</span>
        ) : (
          <span className={styles.numMuted}>—</span>
        )}
      </td>
      <td className={styles.cellNextDue}>{nextDue}</td>
      <td className={styles.cellChevron}>
        <ChevronRight size={16} aria-hidden />
      </td>
    </tr>
  );
}

export function MaintenanceHubTab() {
  const { data, isLoading, isError, error } = useQuery<FleetMaintenanceStatus>({
    queryKey: ['maintenance-fleet-status'],
    queryFn: fetchFleetMaintenanceStatus,
  });

  const [sortKey, setSortKey] = useState<SortKey | null>(null);
  const [sortDir, setSortDir] = useState<SortDir>('asc');
  const [statFilter, setStatFilter] = useState<StatFilter>(null);

  const handleSort = (k: SortKey) => {
    if (sortKey === k) setSortDir((d) => (d === 'asc' ? 'desc' : 'asc'));
    else { setSortKey(k); setSortDir('asc'); }
  };

  const rows = useMemo(() => {
    const list = data?.aircraft ?? [];
    const filtered = statFilter
      ? list.filter((r) => {
          switch (statFilter) {
            case 'airworthy':     return r.airworthiness_status === 'airworthy';
            case 'not_airworthy': return r.airworthiness_status === 'not_airworthy';
            case 'overdue':       return r.overdue_count > 0;
            case 'due_soon':      return r.due_soon_count > 0;
            default:              return true;
          }
        })
      : list;
    if (!sortKey) return filtered;
    const dir = sortDir === 'asc' ? 1 : -1;
    return [...filtered].sort((a, b) => compareRows(a, b, sortKey) * dir);
  }, [data, statFilter, sortKey, sortDir]);

  if (isLoading) return <HubSkeleton />;
  if (isError) {
    return (
      <div className={styles.errorState}>
        Failed to load maintenance hub: {(error as Error).message}
      </div>
    );
  }
  if (!data) return null;

  const {
    airworthy_count,
    not_airworthy_count,
    overdue_count,
    due_soon_count,
  } = data;

  return (
    <div className={styles.wrapper}>
      <div className={styles.statRow}>
        <HubStatCard
          label="Airworthy"
          value={airworthy_count}
          accent="ok"
          highlight={airworthy_count > 0}
          active={statFilter === 'airworthy'}
          onToggle={() => setStatFilter((f) => (f === 'airworthy' ? null : 'airworthy'))}
        />
        <HubStatCard
          label="Not Airworthy"
          value={not_airworthy_count}
          accent="critical"
          highlight={not_airworthy_count > 0}
          active={statFilter === 'not_airworthy'}
          onToggle={() => setStatFilter((f) => (f === 'not_airworthy' ? null : 'not_airworthy'))}
        />
        <HubStatCard
          label="Overdue"
          value={overdue_count}
          accent="critical"
          highlight={overdue_count > 0}
          active={statFilter === 'overdue'}
          onToggle={() => setStatFilter((f) => (f === 'overdue' ? null : 'overdue'))}
        />
        <HubStatCard
          label="Due Soon"
          value={due_soon_count}
          accent="warning"
          highlight={due_soon_count > 0}
          active={statFilter === 'due_soon'}
          onToggle={() => setStatFilter((f) => (f === 'due_soon' ? null : 'due_soon'))}
        />
      </div>

      <section className={styles.panel}>
        <div className={styles.panelHeader}>
          <h2 className={styles.panelTitle}>Fleet Maintenance Status</h2>
          {statFilter && (
            <div className={styles.filterHint}>
              <span>Showing {rows.length} of {data.aircraft.length}</span>
              <button type="button" className={styles.clearBtn} onClick={() => setStatFilter(null)}>
                Clear
              </button>
            </div>
          )}
        </div>
        {rows.length === 0 ? (
          <div className={styles.emptyMuted}>
            {statFilter ? 'No aircraft match this filter.' : 'No active aircraft found.'}
          </div>
        ) : (
          <table className={styles.fleetTable}>
            <thead>
              <tr>
                <SortHeader label="Equipment"     colKey="equipment"     align="left"  sortKey={sortKey} sortDir={sortDir} onSort={handleSort} />
                <SortHeader label="Aircraft"      colKey="aircraft"      align="left"  sortKey={sortKey} sortDir={sortDir} onSort={handleSort} />
                <SortHeader label="Model"         colKey="model"         align="left"  sortKey={sortKey} sortDir={sortDir} onSort={handleSort} />
                <SortHeader label="Airworthiness" colKey="airworthiness" align="left"  sortKey={sortKey} sortDir={sortDir} onSort={handleSort} />
                <SortHeader label="Overdue"       colKey="overdue"       align="right" sortKey={sortKey} sortDir={sortDir} onSort={handleSort} />
                <SortHeader label="Due Soon"      colKey="due_soon"      align="right" sortKey={sortKey} sortDir={sortDir} onSort={handleSort} />
                <SortHeader label="Defects"       colKey="defects"       align="right" sortKey={sortKey} sortDir={sortDir} onSort={handleSort} />
                <SortHeader label="Next Due"      colKey="next_due"      align="left"  sortKey={sortKey} sortDir={sortDir} onSort={handleSort} />
                <th aria-hidden />
              </tr>
            </thead>
            <tbody>
              {rows.map((row) => (
                <FleetRow key={row.aircraft_id} row={row} />
              ))}
            </tbody>
          </table>
        )}
      </section>

      <section className={styles.panel}>
        <div className={styles.panelHeader}>
          <h2 className={styles.panelTitle}>Quick Actions</h2>
        </div>
        <HubQuickActions />
      </section>
    </div>
  );
}
