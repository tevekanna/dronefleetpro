// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useAircraftBreakdowns } from '../../hooks/useFleetStats';
import { BreakdownGroup } from '../breakdowns/BreakdownGroup';
import styles from './AircraftBreakdownsPanel.module.css';

interface Props {
  aircraftId: number;
}

/// Activity breakdowns for a single airframe. Mirrors PilotBreakdownsPanel
/// visually (same bar groups), minus the by_aircraft_model section
/// (meaningless on a per-aircraft page) and the by_pilot section (out of
/// PR #7 scope — could be added later if a "who flies this" view is wanted).
export function AircraftBreakdownsPanel({ aircraftId }: Props) {
  const { data, isLoading, error } = useAircraftBreakdowns(aircraftId);

  return (
    <div className={styles.section}>
      <div className={styles.sectionHeading}>Activity breakdowns</div>
      {isLoading && <div className={styles.muted}>Loading breakdowns…</div>}
      {error && (
        <div className={styles.error}>
          Failed to load breakdowns: {(error as Error).message}
        </div>
      )}
      {!isLoading && !error && data && (
        <div style={{ display: 'flex', flexDirection: 'column', gap: 'var(--space-5)' }}>
          <BreakdownGroup heading="EASA category" rows={data.by_easa_category} />
          <BreakdownGroup heading="Visual regime" rows={data.by_visual_regime} />
          <BreakdownGroup heading="Day / night" rows={data.by_daylight} />
          <BreakdownGroup heading="Mission type" rows={data.by_mission_type} />
        </div>
      )}
    </div>
  );
}
