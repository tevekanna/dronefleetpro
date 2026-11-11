// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import type { AircraftRef } from '../../api/flightDetail';
import { formatAircraftCell } from '../../utils/aircraftFormat';
import styles from './AircraftCell.module.css';

interface AircraftCellProps {
  aircraft: AircraftRef | null | undefined;
}

/**
 * Two-line aircraft cell for tables. Primary: equipment id (or SN
 * fallback). Secondary: "Model · SN" — SN only included when it isn't
 * already the primary. Renders an em dash when no aircraft is set.
 *
 * Presentational only — not a Link. A wrapping component decides
 * whether the row is clickable.
 */
export function AircraftCell({ aircraft }: AircraftCellProps) {
  if (!aircraft) return <span className={styles.muted}>—</span>;

  const { primary, secondary } = formatAircraftCell(aircraft);

  return (
    <div className={styles.cell}>
      <div className={styles.primary}>{primary}</div>
      {secondary && <div className={styles.secondary}>{secondary}</div>}
    </div>
  );
}
