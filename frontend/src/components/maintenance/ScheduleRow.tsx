// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { AlertTriangle, History } from 'lucide-react';
import { Link } from 'react-router-dom';
import type {
  IntervalType,
  ScheduleWithStatus,
  SecondaryIntervalType,
} from '../../types/maintenanceSchedule';
import { ScheduleDueLabel } from './ScheduleDueLabel';
import { ScheduleProgressBar } from './ScheduleProgressBar';
import { ScheduleStatusBadge } from './ScheduleStatusBadge';
import styles from './ScheduleRow.module.css';

interface Props {
  schedule: ScheduleWithStatus;
  canEdit: boolean;
  canComplete: boolean;
  onEdit?: (schedule: ScheduleWithStatus) => void;
  onComplete?: (schedule: ScheduleWithStatus) => void;
  onDeactivate?: (schedule: ScheduleWithStatus) => void;
}

function intervalUnit(type: IntervalType | SecondaryIntervalType): string {
  if (type === 'flight_hours') return 'fh';
  if (type === 'calendar_days') return 'days';
  return 'cycles';
}

function formatInterval(value: number, type: IntervalType | SecondaryIntervalType): string {
  const rounded = Number.isInteger(value) ? value.toString() : value.toFixed(1);
  return `Every ${rounded} ${intervalUnit(type)}`;
}

export function ScheduleRow({
  schedule,
  canEdit,
  canComplete,
  onEdit,
  onComplete,
  onDeactivate,
}: Props) {
  const primaryInterval = formatInterval(schedule.interval_value, schedule.interval_type);
  const secondaryInterval =
    schedule.secondary_interval_type && schedule.secondary_interval_value !== null
      ? formatInterval(schedule.secondary_interval_value, schedule.secondary_interval_type)
      : null;

  return (
    <div
      className={styles.row}
      data-status={schedule.status}
      data-schedule-id={schedule.id}
      id={`schedule-${schedule.id}`}
    >
      <div className={styles.name}>
        {schedule.is_critical && (
          <AlertTriangle size={14} className={styles.criticalIcon} aria-label="Critical" />
        )}
        <span className={styles.nameText} title={schedule.name}>
          {schedule.name}
        </span>
      </div>
      <div className={styles.interval}>
        {primaryInterval}
        {secondaryInterval && (
          <span className={styles.intervalSecondary}>+ {secondaryInterval}</span>
        )}
      </div>
      <ScheduleProgressBar pct={schedule.consumption_pct} status={schedule.status} />
      <ScheduleDueLabel schedule={schedule} />
      <ScheduleStatusBadge status={schedule.status} />
      <div className={styles.actions}>
        {canComplete && onComplete && (
          <button
            type="button"
            className={styles.btnComplete}
            onClick={() => onComplete(schedule)}
          >
            Complete
          </button>
        )}
        {canEdit && onEdit && (
          <button
            type="button"
            className={styles.btnSecondary}
            onClick={() => onEdit(schedule)}
          >
            Edit
          </button>
        )}
        <Link
          to={`/maintenance-schedules/${schedule.id}/history`}
          className={styles.btnIcon}
          aria-label={`History of ${schedule.name}`}
          title="View history"
        >
          <History size={14} />
        </Link>
        {canEdit && onDeactivate && (
          <button
            type="button"
            className={styles.btnSecondary}
            onClick={() => onDeactivate(schedule)}
          >
            Deactivate
          </button>
        )}
      </div>
    </div>
  );
}
