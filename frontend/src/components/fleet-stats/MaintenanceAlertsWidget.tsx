// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { CheckCircle2 } from 'lucide-react';
import { useNavigate } from 'react-router-dom';
import type { MaintenanceAlertRow } from '../../types/fleetDashboard';
import { EmptyState } from './EmptyState';
import styles from './fleetStatsDashboard.module.css';

export interface MaintenanceAlertsWidgetProps {
  rows: MaintenanceAlertRow[];
}

function statusDot(status: 'overdue' | 'warning' | 'ok'): string {
  if (status === 'overdue') return styles.statusDotRed;
  if (status === 'warning') return styles.statusDotAmber;
  return styles.statusDotOk;
}

function dueLabel(row: MaintenanceAlertRow): { text: string; color: string } {
  if (row.days_overdue !== null) {
    const days = row.days_overdue;
    if (days > 0) {
      return {
        text: days === 1 ? '1 day overdue' : `${days} days overdue`,
        color: 'var(--color-critical)',
      };
    }
    if (days === 0) {
      return { text: 'due today', color: 'var(--color-warning)' };
    }
    const remaining = Math.abs(days);
    return {
      text: remaining === 1 ? 'due in 1 day' : `due in ${remaining} days`,
      color: 'var(--color-warning)',
    };
  }
  // Flight-hour-based schedule with no calendar due date.
  const pct = Math.round(row.consumption_pct * 100);
  return {
    text: `${pct}% consumed`,
    color: row.status === 'overdue' ? 'var(--color-critical)' : 'var(--color-warning)',
  };
}

export function MaintenanceAlertsWidget({ rows }: MaintenanceAlertsWidgetProps) {
  const navigate = useNavigate();

  if (rows.length === 0) {
    return (
      <EmptyState
        icon={<CheckCircle2 size={24} color="var(--color-ok)" aria-hidden="true" />}
        message="No overdue maintenance."
        ok
      />
    );
  }

  // No per-schedule detail page yet — navigate to the maintenance-schedules list.
  return (
    <table className={styles.widgetTable}>
      <thead className={styles.widgetTableHead}>
        <tr>
          <th>Aircraft</th>
          <th>Schedule</th>
          <th>Status</th>
        </tr>
      </thead>
      <tbody>
        {rows.map((row) => {
          const label = dueLabel(row);
          return (
            <tr
              key={row.schedule_id}
              className={styles.widgetTableRow}
              onClick={() => navigate('/maintenance-schedules')}
            >
              {/* TODO(aircraft-display-pt2): pending backend DTO extension — MaintenanceAlertRow lacks aircraft_model */}
              <td className={styles.cellId}>
                {row.aircraft_equipment_id ?? row.aircraft_serial_number}
              </td>
              <td>
                {row.schedule_name}
                {row.is_critical && (
                  <span style={{ color: 'var(--color-critical)', marginLeft: 6 }}>
                    · critical
                  </span>
                )}
              </td>
              <td>
                <span className={`${styles.statusDot} ${statusDot(row.status)}`} />
                <span style={{ color: label.color }}>{label.text}</span>
              </td>
            </tr>
          );
        })}
      </tbody>
    </table>
  );
}
