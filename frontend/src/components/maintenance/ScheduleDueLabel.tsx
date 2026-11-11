// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { differenceInCalendarDays, parseISO } from 'date-fns';
import type {
  IntervalType,
  ScheduleStatus,
  ScheduleWithStatus,
  SecondaryIntervalType,
} from '../../types/maintenanceSchedule';
import styles from './ScheduleDueLabel.module.css';

interface Axis {
  type: IntervalType | SecondaryIntervalType;
  intervalValue: number;
  consumptionPct: number;
  nextDueAt: string | null;
  nextDueValue: number | null;
}

function primaryAxis(s: ScheduleWithStatus): Axis {
  return {
    type: s.interval_type,
    intervalValue: s.interval_value,
    consumptionPct: s.consumption_pct,
    nextDueAt: s.next_due_at,
    nextDueValue:
      s.interval_type === 'flight_hours'
        ? s.next_due_flight_hours
        : s.interval_type === 'cycles'
        ? s.next_due_cycles
        : null,
  };
}

function secondaryAxis(s: ScheduleWithStatus): Axis | null {
  if (!s.secondary_interval_type || s.secondary_consumption_pct === null) return null;
  if (s.secondary_interval_value === null) return null;
  return {
    type: s.secondary_interval_type,
    intervalValue: s.secondary_interval_value,
    consumptionPct: s.secondary_consumption_pct,
    nextDueAt: s.secondary_next_due_at,
    nextDueValue:
      s.secondary_interval_type === 'flight_hours' ? s.secondary_next_due_hours : null,
  };
}

function formatDays(days: number): string {
  const rounded = Math.round(days);
  if (rounded === 0) return 'Due today';
  const n = Math.abs(rounded);
  const unit = n === 1 ? 'day' : 'days';
  return rounded > 0 ? `Due in ${n} ${unit}` : `Overdue by ${n} ${unit}`;
}

function formatHoursRemaining(remaining: number): string {
  if (Math.abs(remaining) < 0.05) return 'Due now';
  const n = Math.abs(remaining).toFixed(1);
  return remaining > 0 ? `Due in ${n} fh` : `Overdue by ${n} fh`;
}

function formatCyclesRemaining(remaining: number): string {
  const rounded = Math.round(remaining);
  if (rounded === 0) return 'Due now';
  const n = Math.abs(rounded);
  const unit = n === 1 ? 'cycle' : 'cycles';
  return rounded > 0 ? `Due in ${n} ${unit}` : `Overdue by ${n} ${unit}`;
}

function describeAxis(axis: Axis, now: Date): string {
  if (axis.type === 'calendar_days') {
    if (!axis.nextDueAt) return `Every ${axis.intervalValue} days`;
    return formatDays(differenceInCalendarDays(parseISO(axis.nextDueAt), now));
  }
  if (axis.type === 'flight_hours') {
    if (axis.nextDueValue === null) return `Every ${axis.intervalValue} fh`;
    const remaining = axis.intervalValue * (1 - axis.consumptionPct / 100);
    return formatHoursRemaining(remaining);
  }
  if (axis.nextDueValue === null) return `Every ${axis.intervalValue} cycles`;
  const remaining = axis.intervalValue * (1 - axis.consumptionPct / 100);
  return formatCyclesRemaining(remaining);
}

function axisStatus(pct: number, warningThreshold: number): ScheduleStatus {
  if (pct >= 100) return 'overdue';
  if (pct >= warningThreshold * 100) return 'warning';
  return 'ok';
}

const MAIN_CLASS: Record<ScheduleStatus, string> = {
  ok: styles.main,
  warning: `${styles.main} ${styles.mainWarning}`,
  overdue: `${styles.main} ${styles.mainOverdue}`,
};

export function ScheduleDueLabel({ schedule }: { schedule: ScheduleWithStatus }) {
  const now = new Date();
  const primary = primaryAxis(schedule);
  const secondary = secondaryAxis(schedule);

  let main = primary;
  let other: Axis | null = secondary;
  if (secondary && secondary.consumptionPct > primary.consumptionPct) {
    main = secondary;
    other = primary;
  }

  const mainStatus = axisStatus(main.consumptionPct, schedule.warning_threshold);

  return (
    <div className={styles.wrapper}>
      <span className={MAIN_CLASS[mainStatus]}>{describeAxis(main, now)}</span>
      {other && <span className={styles.secondary}>{describeAxis(other, now)}</span>}
    </div>
  );
}
