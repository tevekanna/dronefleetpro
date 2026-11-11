// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useState, useEffect } from 'react';
import { useQuery } from '@tanstack/react-query';
import { X } from 'lucide-react';
import { fetchAircraft } from '../../api/aircraft';
import { fetchPilots } from '../../api/pilots';
import { useFlightFilterOptions } from '../../hooks/useFlightListFilters';
import type { FlightFilters } from '../../types/flight';
import { formatAircraftLabel } from '../../utils/aircraftFormat';
import { IsoDateInput } from '../inputs/IsoDateInput';
import styles from './FlightFilterBar.module.css';

// Fallbacks rendered until the /flights/filters response arrives, or when the
// org has no committed flights with those fields populated. Live API list
// supersedes these once available.
const FALLBACK_MISSION_TYPES = [
  'Photo/Film',
  'Survey',
  'Inspection',
  'Training',
  'Free Flight',
  'Test Flight',
];
const FALLBACK_EASA_CATEGORIES = [
  'Open-A1',
  'Open-A2',
  'Open-A3',
  'Specific-SAIL-I',
  'Specific-SAIL-II',
];

interface FlightFilterBarProps {
  filters: FlightFilters;
  onChange: (filters: FlightFilters) => void;
  onReset: () => void;
}

function countActiveFilters(f: FlightFilters): number {
  let count = 0;
  if (f.dateFrom) count++;
  if (f.dateTo) count++;
  if (f.aircraftId) count++;
  if (f.pilotId) count++;
  if (f.missionType) count++;
  if (f.easaCategory) count++;
  if (f.search) count++;
  return count;
}

export function FlightFilterBar({ filters, onChange, onReset }: FlightFilterBarProps) {
  const { data: aircraft } = useQuery({ queryKey: ['aircraft', true], queryFn: () => fetchAircraft(true) });
  const { data: pilots } = useQuery({ queryKey: ['pilots'], queryFn: fetchPilots });
  const { data: filterOptions } = useFlightFilterOptions();

  const missionTypes =
    filterOptions?.mission_types && filterOptions.mission_types.length > 0
      ? filterOptions.mission_types
      : FALLBACK_MISSION_TYPES;
  const easaCategories =
    filterOptions?.easa_categories && filterOptions.easa_categories.length > 0
      ? filterOptions.easa_categories
      : FALLBACK_EASA_CATEGORIES;

  // Debounced search
  const [searchLocal, setSearchLocal] = useState(filters.search ?? '');
  useEffect(() => {
    setSearchLocal(filters.search ?? '');
  }, [filters.search]);

  useEffect(() => {
    const timer = setTimeout(() => {
      const trimmed = searchLocal.trim() || undefined;
      if (trimmed !== filters.search) {
        onChange({ ...filters, search: trimmed });
      }
    }, 400);
    return () => clearTimeout(timer);
  }, [searchLocal]); // eslint-disable-line react-hooks/exhaustive-deps

  const activeCount = countActiveFilters(filters);

  return (
    <div className={styles.filterBar}>
      <div className={styles.filterHeader}>
        <span className={styles.filterLabel}>Filters</span>
        {activeCount > 0 && (
          <span className={styles.activeBadge}>{activeCount} active</span>
        )}
      </div>
      <div className={styles.controls}>
        <div className={styles.field}>
          <span className={styles.fieldLabel}>From</span>
          <IsoDateInput
            className={styles.input}
            value={filters.dateFrom ?? ''}
            onChange={e => onChange({ ...filters, dateFrom: e.target.value || undefined })}
          />
        </div>
        <div className={styles.field}>
          <span className={styles.fieldLabel}>To</span>
          <IsoDateInput
            className={styles.input}
            value={filters.dateTo ?? ''}
            onChange={e => onChange({ ...filters, dateTo: e.target.value || undefined })}
          />
        </div>
        <div className={styles.field}>
          <span className={styles.fieldLabel}>Aircraft</span>
          <select
            className={styles.select}
            value={filters.aircraftId ?? ''}
            onChange={e => onChange({ ...filters, aircraftId: e.target.value ? Number(e.target.value) : undefined })}
          >
            <option value="">All aircraft</option>
            {(aircraft ?? []).map(a => (
              <option key={a.id} value={a.id}>{formatAircraftLabel(a)}</option>
            ))}
          </select>
        </div>
        <div className={styles.field}>
          <span className={styles.fieldLabel}>Pilot</span>
          <select
            className={styles.select}
            value={filters.pilotId ?? ''}
            onChange={e => onChange({ ...filters, pilotId: e.target.value ? Number(e.target.value) : undefined })}
          >
            <option value="">All pilots</option>
            {(pilots ?? []).filter(p => p.is_active).map(p => (
              <option key={p.id} value={p.id}>{p.name}</option>
            ))}
          </select>
        </div>
        <div className={styles.field}>
          <span className={styles.fieldLabel}>Mission</span>
          <select
            className={styles.select}
            value={filters.missionType ?? ''}
            onChange={e => onChange({ ...filters, missionType: e.target.value || undefined })}
          >
            <option value="">All types</option>
            {missionTypes.map(t => <option key={t} value={t}>{t}</option>)}
          </select>
        </div>
        <div className={styles.field}>
          <span className={styles.fieldLabel}>EASA Category</span>
          <select
            className={styles.select}
            value={filters.easaCategory ?? ''}
            onChange={e => onChange({ ...filters, easaCategory: e.target.value || undefined })}
          >
            <option value="">All categories</option>
            {easaCategories.map(c => <option key={c} value={c}>{c}</option>)}
          </select>
        </div>
        <div className={`${styles.field} ${styles.searchField}`}>
          <span className={styles.fieldLabel}>Search</span>
          <input
            type="text"
            className={`${styles.input} ${styles.searchInput}`}
            placeholder="Search operation, aircraft, remarks..."
            value={searchLocal}
            onChange={e => setSearchLocal(e.target.value)}
          />
        </div>
        {activeCount > 0 && (
          <button className={styles.resetBtn} onClick={onReset}>
            <X size={14} />
            Clear filters
          </button>
        )}
      </div>
    </div>
  );
}
