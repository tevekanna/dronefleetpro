// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useParams } from 'react-router-dom';
import { Breadcrumbs } from '../components/Breadcrumbs/Breadcrumbs';
import { useMaintenanceSchedule } from '../hooks/useMaintenanceSchedules';
import { ScheduleStatusBadge } from '../components/maintenance/ScheduleStatusBadge';
import type { ScheduleHistoryItem } from '../types/maintenanceSchedule';
import styles from './MaintenanceHistoryPage.module.css';

function truncate(s: string, max = 80): string {
  if (!s) return '—';
  return s.length > max ? `${s.slice(0, max)}…` : s;
}

function intervalSummary(
  type: string,
  value: number,
  secondaryType: string | null,
  secondaryValue: number | null,
): string {
  const unit = type === 'flight_hours' ? 'fh' : type === 'calendar_days' ? 'days' : 'cycles';
  const primary = `Every ${value} ${unit}`;
  if (!secondaryType || secondaryValue === null) return primary;
  const secUnit = secondaryType === 'flight_hours' ? 'fh' : 'days';
  return `${primary} · and every ${secondaryValue} ${secUnit}`;
}

function HistoryTable({ history }: { history: ScheduleHistoryItem[] }) {
  if (history.length === 0) {
    return <div className={styles.empty}>No completions logged yet.</div>;
  }
  return (
    <table className={styles.table}>
      <thead>
        <tr>
          <th className={styles.th}>Date</th>
          <th className={styles.th}>Type</th>
          <th className={styles.th}>Created by</th>
          <th className={styles.th}>Sign-off</th>
          <th className={styles.th}>Remark</th>
          <th className={styles.th}>Action taken</th>
        </tr>
      </thead>
      <tbody>
        {history.map((row) => (
          <tr key={row.maintenance_id} className={styles.tr}>
            <td className={`${styles.td} ${styles.mono}`}>{row.performed_date}</td>
            <td className={styles.td}>{row.maint_type || '—'}</td>
            <td className={`${styles.td} ${styles.mono}`}>{row.created_by}</td>
            <td className={`${styles.td} ${styles.mono}`}>{row.sign_off}</td>
            <td className={styles.td}>
              <span className={styles.truncated} title={row.remark}>
                {truncate(row.remark)}
              </span>
            </td>
            <td className={styles.td}>
              <span className={styles.truncated} title={row.action_taken}>
                {truncate(row.action_taken)}
              </span>
            </td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

export function MaintenanceHistoryPage() {
  const { scheduleId } = useParams<{ scheduleId: string }>();
  const id = scheduleId ? Number(scheduleId) : null;
  const { data, isLoading, error } = useMaintenanceSchedule(id);

  if (isLoading) {
    return <p className={styles.errorMsg}>Loading…</p>;
  }
  if (error) {
    return (
      <p className={styles.errorMsg}>
        Failed to load schedule: {(error as Error).message}
      </p>
    );
  }
  if (!data) return null;

  const aircraftLabel = data.aircraft_equipment_id ?? data.aircraft_serial_number;
  const lastCompleted = data.last_performed_at
    ? new Date(data.last_performed_at).toISOString().slice(0, 10)
    : 'Never completed';

  return (
    <div className={styles.page}>
      <Breadcrumbs
        items={[
          { label: 'Schedules', to: '/maintenance-schedules' },
          { label: aircraftLabel },
          { label: `${data.name} · History` },
        ]}
      />

      <div className={styles.headerCard}>
        <div className={styles.headerTop}>
          <h1 className={styles.scheduleName}>{data.name}</h1>
          <ScheduleStatusBadge status={data.status} />
        </div>
        <div className={styles.metaRow}>
          <span>
            Aircraft: <strong>{aircraftLabel}</strong>
          </span>
          <span>
            Interval:{' '}
            <strong>
              {intervalSummary(
                data.interval_type,
                data.interval_value,
                data.secondary_interval_type,
                data.secondary_interval_value,
              )}
            </strong>
          </span>
          <span>
            Last completed: <strong>{lastCompleted}</strong>
          </span>
          <span>
            Completions: <strong>{data.history.length}</strong>
          </span>
        </div>
      </div>

      <HistoryTable history={data.history} />
    </div>
  );
}
