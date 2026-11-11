// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import type { ScheduleStatus } from '../../types/maintenanceSchedule';
import styles from './ScheduleProgressBar.module.css';

interface Props {
  pct: number;
  status: ScheduleStatus;
}

const FILL_BY_STATUS: Record<ScheduleStatus, string> = {
  ok: styles.fillOk,
  warning: styles.fillWarning,
  overdue: styles.fillOverdue,
};

export function ScheduleProgressBar({ pct, status }: Props) {
  const clampedWidth = Math.min(Math.max(pct, 0), 100);
  const labelText = `${Math.round(pct)}%`;
  const labelClass =
    status === 'overdue' ? `${styles.label} ${styles.labelOverdue}` : styles.label;

  return (
    <div
      className={styles.wrapper}
      role="progressbar"
      aria-valuenow={Math.round(pct)}
      aria-valuemin={0}
      aria-valuemax={100}
    >
      <div className={styles.track}>
        <div
          className={`${styles.fill} ${FILL_BY_STATUS[status]}`}
          style={{ width: `${clampedWidth}%` }}
        />
      </div>
      <span className={labelClass}>{labelText}</span>
    </div>
  );
}
