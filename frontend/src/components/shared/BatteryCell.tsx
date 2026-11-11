// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import type { BatteryRef } from '../../api/flightDetail';
import styles from './BatteryCell.module.css';

interface BatteryCellProps {
  battery: BatteryRef | null | undefined;
}

/**
 * Two-line battery cell for tables, mirroring {@link AircraftCell}.
 * Primary: equipment id (or SN fallback). Secondary: "model · SN" — SN
 * only included when it isn't already the primary. A retired battery
 * gets the critical colour on its primary line. Renders an em dash when
 * no battery is set.
 *
 * Presentational only — not a Link. A wrapping component decides whether
 * the cell links out (the flight list wraps it in a Link to the battery
 * stats page).
 */
export function BatteryCell({ battery }: BatteryCellProps) {
  if (!battery) return <span className={styles.muted}>—</span>;

  const primary = battery.equipment_id ?? battery.serial_number;
  const parts: string[] = [];
  if (battery.compatible_model) parts.push(battery.compatible_model);
  if (battery.equipment_id && battery.equipment_id !== battery.serial_number) {
    parts.push(battery.serial_number);
  }
  const secondary = parts.length ? parts.join(' · ') : null;
  const isRetired = battery.status === 'retired';

  return (
    <div className={styles.cell}>
      <div className={isRetired ? `${styles.primary} ${styles.retired}` : styles.primary}>
        {primary}
      </div>
      {secondary && <div className={styles.secondary}>{secondary}</div>}
    </div>
  );
}
