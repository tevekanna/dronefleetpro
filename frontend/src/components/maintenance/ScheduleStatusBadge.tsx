// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import type { ScheduleStatus } from '../../types/maintenanceSchedule';
import styles from './ScheduleStatusBadge.module.css';

const LABEL: Record<ScheduleStatus, string> = {
  ok: 'OK',
  warning: 'Due soon',
  overdue: 'Overdue',
};

export function ScheduleStatusBadge({ status }: { status: ScheduleStatus }) {
  return (
    <span className={`${styles.badge} ${styles[status]}`} role="status">
      <span className={styles.dot} aria-hidden="true" />
      {LABEL[status]}
    </span>
  );
}
