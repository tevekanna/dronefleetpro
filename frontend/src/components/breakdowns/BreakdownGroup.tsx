// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import styles from './BreakdownGroup.module.css';

/// One bucket in a categorical breakdown. Field shape matches what both
/// the pilot and aircraft detail endpoints return (`{ label, flights,
/// total_seconds }`), so both panels can feed this component directly.
export interface CategoryStats {
  label: string;
  flights: number;
  total_seconds: number;
}

function formatDuration(seconds: number): string {
  if (seconds <= 0) return '0h 0m';
  const h = Math.floor(seconds / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  return `${h}h ${m}m`;
}

/// Renders one labelled bar-chart group for a categorical breakdown.
/// Bars are sized against the largest bucket so visual emphasis is
/// consistent within the group. Used by PilotBreakdownsPanel and
/// AircraftBreakdownsPanel.
export function BreakdownGroup({
  heading,
  rows,
}: {
  heading: string;
  rows: CategoryStats[];
}) {
  const max = rows.reduce(
    (m, r) => Math.max(m, r.total_seconds, r.flights),
    0,
  );

  return (
    <div className={styles.group}>
      <div className={styles.heading}>{heading}</div>
      {rows.length === 0 ? (
        <div className={styles.muted}>No data yet.</div>
      ) : (
        <div className={styles.bars}>
          {rows.map((r) => {
            // Use seconds as the visual weight when present; fall back
            // to flight count (Mixed/Unknown day-night labels carry
            // seconds=0 when classification couldn't run).
            const weight = r.total_seconds > 0 ? r.total_seconds : r.flights;
            const pct = max > 0 ? Math.max(2, (weight / max) * 100) : 0;
            return (
              <div key={r.label} className={styles.row}>
                <span className={styles.label} title={r.label}>
                  {r.label}
                </span>
                <div className={styles.track}>
                  <div className={styles.fill} style={{ width: `${pct}%` }} />
                </div>
                <span className={styles.numeric}>
                  {r.flights}× · {formatDuration(r.total_seconds)}
                </span>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
