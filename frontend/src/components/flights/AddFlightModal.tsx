// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useState } from 'react';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import { fetchAircraft } from '../../api/aircraft';
import { createFlight } from '../../api/flights';
import { useFlightFilterOptions } from '../../hooks/useFlightListFilters';
import type { Aircraft } from '../../types/aircraft';
import type { CreateFlightRequest } from '../../types/flight';
import { formatAircraftLabel } from '../../utils/aircraftFormat';
import { IsoDateInput } from '../inputs/IsoDateInput';
import styles from './AddFlightModal.module.css';

const FALLBACK_EASA_CATEGORIES = [
  'Open-A1',
  'Open-A2',
  'Open-A3',
  'Specific-SAIL-I',
  'Specific-SAIL-II',
];
const FALLBACK_MISSION_TYPES = [
  'Photo/Film',
  'Survey',
  'Inspection',
  'Training',
  'Free Flight',
  'Test Flight',
];

interface AddFlightModalProps {
  onClose: () => void;
}

export function AddFlightModal({ onClose }: AddFlightModalProps) {
  const queryClient = useQueryClient();
  // include_retired=true preserves prior runtime behaviour. A per-call-site
  // review of retired-aircraft visibility is still to be done.
  const { data: aircraft } = useQuery({
    queryKey: ['aircraft', true],
    queryFn: () => fetchAircraft(true),
  });
  const { data: filterOptions } = useFlightFilterOptions();

  const easaCategories =
    filterOptions?.easa_categories && filterOptions.easa_categories.length > 0
      ? filterOptions.easa_categories
      : FALLBACK_EASA_CATEGORIES;
  const missionTypes =
    filterOptions?.mission_types && filterOptions.mission_types.length > 0
      ? filterOptions.mission_types
      : FALLBACK_MISSION_TYPES;

  const [flightDate, setFlightDate] = useState('');
  const [durationMin, setDurationMin] = useState('');
  const [aircraftId, setAircraftId] = useState('');
  const [easaCategory, setEasaCategory] = useState('');
  const [missionType, setMissionType] = useState('');
  const [operationName, setOperationName] = useState('');
  const [remarks, setRemarks] = useState('');
  const [preflightDone, setPreflightDone] = useState(false);
  const [error, setError] = useState('');

  const mutation = useMutation({
    mutationFn: (data: CreateFlightRequest) => createFlight(data),
    onSuccess: () => {
      queryClient.invalidateQueries({ queryKey: ['flights'] });
      queryClient.invalidateQueries({ queryKey: ['flight-stats'] });
      queryClient.invalidateQueries({ queryKey: ['flight-filter-options'] });
      onClose();
    },
    onError: (err: Error) => setError(err.message),
  });

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (!flightDate.trim()) {
      setError('Flight date is required');
      return;
    }
    if (!durationMin.trim()) {
      setError('Duration is required');
      return;
    }
    setError('');
    mutation.mutate({
      flight_datetime: flightDate.trim() + 'T12:00:00Z',
      duration_seconds: Math.round(Number(durationMin) * 60),
      aircraft_id: aircraftId ? Number(aircraftId) : undefined,
      easa_category: easaCategory || undefined,
      mission_type: missionType || undefined,
      operation_name: operationName.trim() || undefined,
      remarks: remarks.trim() || undefined,
      preflight_done: preflightDone,
    });
  };

  return (
    <div className={styles.overlay} onClick={onClose}>
      <div className={styles.modal} onClick={(e) => e.stopPropagation()}>
        <h2 className={styles.modalTitle}>Add Manual Flight</h2>
        <form onSubmit={handleSubmit}>
          <div className={styles.fieldRow}>
            <div className={styles.field}>
              <label className={styles.label}>Flight Date *</label>
              <IsoDateInput
                className={styles.input}
                value={flightDate}
                onChange={(e) => setFlightDate(e.target.value)}
                required
              />
            </div>
            <div className={styles.field}>
              <label className={styles.label}>Duration (minutes) *</label>
              <input
                className={styles.input}
                type="number"
                min="1"
                value={durationMin}
                onChange={(e) => setDurationMin(e.target.value)}
                placeholder="e.g. 15"
                required
              />
            </div>
          </div>

          <div className={styles.field}>
            <label className={styles.label}>Aircraft</label>
            <select
              className={styles.input}
              value={aircraftId}
              onChange={(e) => setAircraftId(e.target.value)}
            >
              <option value="">— Select aircraft —</option>
              {(aircraft ?? []).map((a: Aircraft) => (
                <option key={a.id} value={a.id}>
                  {formatAircraftLabel(a)}
                </option>
              ))}
            </select>
          </div>

          <div className={styles.fieldRow}>
            <div className={styles.field}>
              <label className={styles.label}>EASA Category</label>
              <select
                className={styles.input}
                value={easaCategory}
                onChange={(e) => setEasaCategory(e.target.value)}
              >
                <option value="">— Select —</option>
                {easaCategories.map((c) => (
                  <option key={c} value={c}>
                    {c}
                  </option>
                ))}
              </select>
            </div>
            <div className={styles.field}>
              <label className={styles.label}>Mission Type</label>
              <select
                className={styles.input}
                value={missionType}
                onChange={(e) => setMissionType(e.target.value)}
              >
                <option value="">— Select —</option>
                {missionTypes.map((m) => (
                  <option key={m} value={m}>
                    {m}
                  </option>
                ))}
              </select>
            </div>
          </div>

          <div className={styles.field}>
            <label className={styles.label}>Operation Name</label>
            <input
              className={styles.input}
              value={operationName}
              onChange={(e) => setOperationName(e.target.value)}
              placeholder="e.g. Site survey Gothenburg"
            />
          </div>

          <div className={styles.field}>
            <label className={styles.label}>Remarks</label>
            <textarea
              className={styles.textarea}
              value={remarks}
              onChange={(e) => setRemarks(e.target.value)}
              placeholder="Optional notes…"
            />
          </div>

          <div className={styles.checkboxRow}>
            <input
              type="checkbox"
              id="preflight"
              checked={preflightDone}
              onChange={(e) => setPreflightDone(e.target.checked)}
              className={styles.checkbox}
            />
            <label htmlFor="preflight" className={styles.checkboxLabel}>
              Preflight checklist completed
            </label>
          </div>

          {error && <p className={styles.formError}>{error}</p>}

          <div className={styles.actionsRight}>
            <button type="button" onClick={onClose} className={styles.btnCancel}>
              Cancel
            </button>
            <button
              type="submit"
              disabled={mutation.isPending}
              className={styles.btnSubmit}
            >
              {mutation.isPending ? 'Saving…' : 'Add Flight'}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}
