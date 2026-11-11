// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { CheckCircle2 } from 'lucide-react';
import { useNavigate } from 'react-router-dom';
import type { BatteryNearEolRow } from '../../types/fleetDashboard';
import { EmptyState } from './EmptyState';
import styles from './fleetStatsDashboard.module.css';

export interface BatteriesNearEolWidgetProps {
  rows: BatteryNearEolRow[];
}

function severityFromPct(pct: number): 'ok' | 'amber' | 'red' {
  if (pct >= 1.0) return 'red';
  if (pct >= 0.8) return 'amber';
  return 'ok';
}

function dotClass(sev: 'ok' | 'amber' | 'red'): string {
  if (sev === 'red') return styles.statusDotRed;
  if (sev === 'amber') return styles.statusDotAmber;
  return styles.statusDotOk;
}

function fillClass(sev: 'ok' | 'amber' | 'red'): string {
  if (sev === 'red') return styles.cycleBarFillRed;
  if (sev === 'amber') return ''; // default amber fill
  return styles.cycleBarFillOk;
}

export function BatteriesNearEolWidget({ rows }: BatteriesNearEolWidgetProps) {
  const navigate = useNavigate();

  // Only flag rows that are actually concerning (>=80% cycle ratio).
  const concerning = rows.filter((r) => r.cycle_pct >= 0.8);

  if (concerning.length === 0) {
    return (
      <EmptyState
        icon={<CheckCircle2 size={24} color="var(--color-ok)" aria-hidden="true" />}
        message="All batteries healthy."
        ok
      />
    );
  }

  return (
    <table className={styles.widgetTable}>
      <thead className={styles.widgetTableHead}>
        <tr>
          <th>ID</th>
          <th>Model</th>
          <th className={styles.cellRight}>Cycles</th>
          <th>Health</th>
        </tr>
      </thead>
      <tbody>
        {concerning.map((row) => {
          const sev = severityFromPct(row.cycle_pct);
          const pctNum = Math.round(row.cycle_pct * 100);
          return (
            <tr
              key={row.battery_id}
              className={styles.widgetTableRow}
              onClick={() => navigate(`/fleet-stats/batteries/${row.battery_id}`)}
            >
              <td className={styles.cellId}>{row.equipment_id ?? row.serial_number}</td>
              <td>{row.model ?? <span className={styles.cellMuted}>—</span>}</td>
              <td className={`${styles.cellRight} ${styles.cellMono}`}>
                {row.cycle_count}/{row.max_cycles}
              </td>
              <td>
                <span className={`${styles.statusDot} ${dotClass(sev)}`} />
                <span className={styles.cellMono}>{pctNum}%</span>
                <span className={styles.cycleBarOuter}>
                  <span
                    className={`${styles.cycleBarFill} ${fillClass(sev)}`}
                    style={{ width: `${Math.min(100, pctNum)}%` }}
                  />
                </span>
              </td>
            </tr>
          );
        })}
      </tbody>
    </table>
  );
}
