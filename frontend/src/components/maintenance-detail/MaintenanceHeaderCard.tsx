// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { Link } from 'react-router-dom';
import { Plane, Gamepad2 } from 'lucide-react';
import type { MaintenanceRecord } from '../../api/maintenance';
import styles from '../../pages/MaintenanceDetailPage.module.css';

interface Props {
  record: MaintenanceRecord;
}

function CategoryBadge({ value }: { value: string | null }) {
  if (!value) return <span className={styles.detailMuted}>—</span>;
  const cls =
    value === 'Scheduled'
      ? `${styles.categoryBadge} ${styles.categoryScheduled}`
      : `${styles.categoryBadge} ${styles.categoryUnscheduled}`;
  return <span className={cls}>{value}</span>;
}

function TargetMetaItem({ record }: { record: MaintenanceRecord }) {
  if (record.controller_id) {
    return (
      <div className={styles.metaItem}>
        <span className={styles.metaLabel}>
          <Gamepad2 size={12} style={{ verticalAlign: 'middle', marginRight: 4 }} />
          Controller
        </span>
        <Link
          to={`/fleet-stats/controllers/${record.controller_id}`}
          className={`${styles.metaValue} ${styles.metaLink}`}
        >
          {record.controller_name ?? `Controller #${record.controller_id}`}
        </Link>
      </div>
    );
  }
  return (
    <div className={styles.metaItem}>
      <span className={styles.metaLabel}>
        <Plane size={12} style={{ verticalAlign: 'middle', marginRight: 4 }} />
        Aircraft
      </span>
      {record.aircraft_id ? (
        <Link
          to={`/aircraft/${record.aircraft_id}`}
          className={`${styles.metaValue} ${styles.metaLink}`}
        >
          {record.aircraft_name ?? `Aircraft #${record.aircraft_id}`}
        </Link>
      ) : (
        <span className={`${styles.metaValue} ${styles.detailMuted}`}>—</span>
      )}
    </div>
  );
}

export function MaintenanceHeaderCard({ record }: Props) {
  const date = record.performed_date.substring(0, 10);
  const subtitle = record.controller_id
    ? record.controller_name
    : record.aircraft_name;

  return (
    <div className={`${styles.section} ${styles.headerCard}`}>
      <div className={styles.headerTopRow}>
        <div>
          <div className={styles.flightTitle}>Maintenance — {date}</div>
          {subtitle && <div className={styles.flightSubtitle}>{subtitle}</div>}
        </div>
      </div>

      <div className={styles.metaGrid}>
        <div className={styles.metaItem}>
          <span className={styles.metaLabel}>Date</span>
          <span className={styles.metaValue}>{date}</span>
        </div>

        <TargetMetaItem record={record} />

        <div className={styles.metaItem}>
          <span className={styles.metaLabel}>Category</span>
          <span className={styles.metaValue}>
            <CategoryBadge value={record.maint_category} />
          </span>
        </div>

        <div className={styles.metaItem}>
          <span className={styles.metaLabel}>Type</span>
          <span className={styles.metaValue}>{record.maint_type ?? '—'}</span>
        </div>

        <div className={styles.metaItem}>
          <span className={styles.metaLabel}>Created by</span>
          <span className={styles.metaValue}>{record.created_by}</span>
        </div>

        <div className={styles.metaItem}>
          <span className={styles.metaLabel}>Sign-off</span>
          <span className={styles.metaValue}>{record.sign_off}</span>
        </div>

        {record.flight_id !== null && (
          <div className={styles.metaItem}>
            <span className={styles.metaLabel}>Flight ref</span>
            <Link
              to={`/flights/${record.flight_id}`}
              className={`${styles.metaValue} ${styles.metaLink}`}
            >
              Flight #{record.flight_id}
            </Link>
          </div>
        )}

        {record.schedule_id !== null && (
          <div className={styles.metaItem}>
            <span className={styles.metaLabel}>Schedule</span>
            <Link
              to={`/maintenance-schedules/${record.schedule_id}/history`}
              className={`${styles.metaValue} ${styles.metaLink}`}
            >
              Schedule #{record.schedule_id}
            </Link>
          </div>
        )}
      </div>
    </div>
  );
}
