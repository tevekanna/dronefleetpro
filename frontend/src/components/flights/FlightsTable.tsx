// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { Fragment, useMemo, useState } from 'react';
import { Link, useNavigate } from 'react-router-dom';
import { ChevronRight } from 'lucide-react';
import { useMutation, useQueryClient } from '@tanstack/react-query';
import { deleteFlight } from '../../api/flights';
import type { Flight } from '../../types/flight';
import { AircraftCell } from '../shared/AircraftCell';
import { BatteryCell } from '../shared/BatteryCell';
import { formatDuration } from '../flight-detail/flightFormatters';
import styles from './FlightsTable.module.css';

const COLUMN_COUNT = 9;

function formatDate(dateStr: string | null): string {
  if (!dateStr) return '—';
  return dateStr.substring(0, 10);
}

function SourceBadge({ source }: { source: string | null }) {
  if (!source) return <span className={styles.sourceBadgeNone}>—</span>;
  const isDji = source === 'dji_import';
  const className = isDji
    ? `${styles.sourceBadge} ${styles.sourceBadgeDji}`
    : styles.sourceBadge;
  return (
    <span className={className}>
      {isDji ? 'DJI Import' : source === 'manual' ? 'Manual' : source}
    </span>
  );
}

/**
 * A sortie bucket: all flights on THIS page that share a `sortie_id`,
 * in page order (flight_datetime DESC). `size` / `totalSeconds` come from
 * the server and describe the FULL sortie across the whole filtered set —
 * so `flights.length` can be smaller than `size` when the sortie continues
 * onto an adjacent page (or when another aircraft's flight interleaves in
 * the reverse-chronological list). The header always shows the server totals.
 */
interface SortieGroup {
  sortieId: number;
  size: number;
  totalSeconds: number;
  flights: Flight[];
}

/**
 * Cluster a page of flights into sorties, keyed by `sortie_id`, ordered by
 * first appearance (i.e. by newest member, since the page is DESC). Members
 * keep their page order. Clustering by id — rather than by consecutive
 * position — keeps a sortie's flights together under one header even if a
 * different aircraft's flight interleaves between them in the time-ordered
 * list. Flights without sortie metadata fall back to a solo group.
 */
function buildSortieGroups(flights: Flight[]): SortieGroup[] {
  const order: number[] = [];
  const byId = new Map<number, SortieGroup>();

  for (const f of flights) {
    // Fallback: treat a flight with no sortie metadata as its own solo
    // sortie (negative synthetic key can't collide with a real sortie_id).
    const id = f.sortie_id ?? -f.id;
    const size = f.sortie_size ?? 1;
    const total = f.sortie_total_seconds ?? f.duration_seconds;
    let group = byId.get(id);
    if (!group) {
      group = { sortieId: id, size, totalSeconds: total, flights: [] };
      byId.set(id, group);
      order.push(id);
    }
    group.flights.push(f);
  }

  return order.map((id) => byId.get(id)!);
}

interface FlightsTableProps {
  flights: Flight[];
}

export function FlightsTable({ flights }: FlightsTableProps) {
  const navigate = useNavigate();
  const queryClient = useQueryClient();
  // Expanded sorties by id. Component state (not ModelAccordion's
  // localStorage) because sortie_id is per-request and not stable across
  // pages or filter changes — persisting it would be meaningless.
  const [expanded, setExpanded] = useState<Set<number>>(new Set());

  const deleteFlightMutation = useMutation({
    mutationFn: deleteFlight,
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['flights'] });
      queryClient.invalidateQueries({ queryKey: ['flight-stats'] });
    },
  });

  const groups = useMemo(() => buildSortieGroups(flights), [flights]);

  // The Delete button lives inside a clickable row, so it must stop the
  // click from propagating up to the row's onClick handler — otherwise
  // confirming the delete would also navigate away from the listing.
  const handleDelete = (e: React.MouseEvent, id: number) => {
    e.stopPropagation();
    if (window.confirm('Delete this flight? This cannot be undone.')) {
      deleteFlightMutation.mutate(id);
    }
  };

  const toggleSortie = (e: React.MouseEvent, sortieId: number) => {
    // Stop propagation so expanding a group doesn't also navigate — same
    // guard the battery link and Delete button use inside the row.
    e.stopPropagation();
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(sortieId)) next.delete(sortieId);
      else next.add(sortieId);
      return next;
    });
  };

  return (
    <table className={styles.table}>
      <thead>
        <tr>
          <th className={styles.th}>Date</th>
          <th className={styles.th}>Aircraft</th>
          <th className={styles.th}>Battery</th>
          <th className={styles.th}>Duration</th>
          <th className={styles.th}>Max Alt</th>
          <th className={styles.th}>EASA Category</th>
          <th className={styles.th}>Mission</th>
          <th className={styles.th}>Source</th>
          <th className={styles.th}>Actions</th>
        </tr>
      </thead>
      <tbody>
        {groups.map((group) => {
          // A solo sortie renders as an ordinary row — no group chrome.
          if (group.size <= 1) {
            return (
              <FlightRow
                key={group.flights[0].id}
                flight={group.flights[0]}
                onNavigate={navigate}
                onDelete={handleDelete}
              />
            );
          }

          const isOpen = expanded.has(group.sortieId);
          // Fewer visible members than the server's full-sortie size means
          // the rest sit on an adjacent page — flag it so the header totals
          // (which are correct for the full sortie) don't look wrong against
          // the visible child count.
          const isPartial = group.flights.length < group.size;

          return (
            <Fragment key={`sortie-${group.sortieId}`}>
              <SortieHeaderRow
                group={group}
                isOpen={isOpen}
                isPartial={isPartial}
                onToggle={toggleSortie}
              />
              {isOpen &&
                group.flights.map((f) => (
                  <FlightRow
                    key={f.id}
                    flight={f}
                    nested
                    onNavigate={navigate}
                    onDelete={handleDelete}
                  />
                ))}
            </Fragment>
          );
        })}
      </tbody>
    </table>
  );
}

function SortieHeaderRow({
  group,
  isOpen,
  isPartial,
  onToggle,
}: {
  group: SortieGroup;
  isOpen: boolean;
  isPartial: boolean;
  onToggle: (e: React.MouseEvent, sortieId: number) => void;
}) {
  const aircraft = group.flights[0].aircraft ?? null;
  return (
    <tr
      className={styles.sortieHeaderRow}
      onClick={(e) => onToggle(e, group.sortieId)}
    >
      <td className={styles.sortieHeaderCell} colSpan={COLUMN_COUNT}>
        <div className={styles.sortieHeader}>
          <ChevronRight
            size={16}
            className={`${styles.chevron} ${isOpen ? styles.chevronOpen : ''}`}
          />
          <span className={styles.sortieLabel}>Sortie</span>
          <AircraftCell aircraft={aircraft} />
          <span className={styles.countPill}>{group.size} flights</span>
          <span className={styles.sortieTotal}>
            {formatDuration(group.totalSeconds)} total
          </span>
          {isPartial && (
            <span className={styles.continuation}>
              · continues on adjacent page
            </span>
          )}
        </div>
      </td>
    </tr>
  );
}

function FlightRow({
  flight,
  nested = false,
  onNavigate,
  onDelete,
}: {
  flight: Flight;
  nested?: boolean;
  onNavigate: (path: string) => void;
  onDelete: (e: React.MouseEvent, id: number) => void;
}) {
  const f = flight;
  return (
    <tr
      className={`${styles.row} ${nested ? styles.nestedRow : ''}`}
      onClick={() => onNavigate(`/flights/${f.id}`)}
    >
      <td className={`${styles.td} ${styles.mono}`}>
        {formatDate(f.flight_datetime)}
      </td>
      <td className={styles.td}>
        <AircraftCell aircraft={f.aircraft ?? null} />
      </td>
      <td className={styles.td}>
        {f.battery ? (
          <Link
            to={`/fleet-stats/batteries/${f.battery.id}`}
            className={styles.batteryLink}
            onClick={(e) => e.stopPropagation()}
          >
            <BatteryCell battery={f.battery} />
          </Link>
        ) : (
          <BatteryCell battery={null} />
        )}
      </td>
      <td className={`${styles.td} ${styles.mono}`}>
        {formatDuration(f.duration_seconds)}
      </td>
      <td className={`${styles.td} ${styles.mono}`}>
        {f.max_altitude_m.toFixed(0)} m
      </td>
      <td className={styles.td}>{f.easa_category ?? '—'}</td>
      <td className={styles.td}>{f.mission_type ?? '—'}</td>
      <td className={styles.td}>
        <SourceBadge source={f.log_source} />
      </td>
      <td className={styles.td}>
        <button onClick={(e) => onDelete(e, f.id)} className={styles.btnDelete}>
          Delete
        </button>
      </td>
    </tr>
  );
}
