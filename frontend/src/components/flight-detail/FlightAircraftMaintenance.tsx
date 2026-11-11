// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { Link } from 'react-router-dom';
import { ChevronRight } from 'lucide-react';
import type {
  ScheduleStatus,
  ScheduleWithStatus,
} from '../../types/maintenanceSchedule';
import styles from '../../pages/FlightDetailPage.module.css';

const STATUS_CLASS: Record<ScheduleStatus, string> = {
  ok: styles.statusOk,
  warning: styles.statusWarning,
  overdue: styles.statusOverdue,
};

function formatConsumption(s: ScheduleWithStatus): string {
  // Two-axis schedules (e.g. annual OR every 50 flight hours) show the
  // worst of the two. Fall back to a single percent when there's no
  // secondary axis.
  const primary = s.consumption_pct;
  const secondary = s.secondary_consumption_pct;
  if (secondary != null) {
    const worst = Math.max(primary, secondary);
    return `${worst.toFixed(0)}% of interval`;
  }
  return `${primary.toFixed(0)}% of interval`;
}

export function FlightAircraftMaintenance({
  schedules,
}: {
  schedules: ScheduleWithStatus[];
}) {
  if (schedules.length === 0) return null;

  return (
    <div className={styles.section}>
      <div className={styles.sectionHeading}>Aircraft maintenance status</div>
      <div className={styles.scheduleList}>
        {schedules.map(s => (
          <Link
            key={s.id}
            to="/maintenance-schedules"
            className={styles.scheduleRow}
          >
            <span className={styles.scheduleName} title={s.name}>
              {s.name}
            </span>
            <span className={`${styles.scheduleStatus} ${STATUS_CLASS[s.status]}`}>
              {s.status}
            </span>
            <span className={styles.scheduleConsumption}>
              {formatConsumption(s)}
            </span>
            <ChevronRight size={14} className={styles.scheduleArrow} aria-hidden />
          </Link>
        ))}
      </div>
    </div>
  );
}
