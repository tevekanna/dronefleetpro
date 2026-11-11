// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useParams } from 'react-router-dom';
import { Breadcrumbs } from '../components/Breadcrumbs/Breadcrumbs';
import type { BreadcrumbItem } from '../components/Breadcrumbs/Breadcrumbs';
import { useMaintenanceDetail } from '../hooks/useMaintenanceDetail';
import { MaintenanceHeaderCard } from '../components/maintenance-detail/MaintenanceHeaderCard';
import { MaintenanceBodyCard } from '../components/maintenance-detail/MaintenanceBodyCard';
import { MaintenanceAuditCard } from '../components/maintenance-detail/MaintenanceAuditCard';
import styles from './MaintenanceDetailPage.module.css';

function crumbs(label: string | undefined): BreadcrumbItem[] {
  return [
    { label: 'Maintenance', to: '/maintenance' },
    { label: label ?? '…' },
  ];
}

function buildLabel(
  date: string,
  targetName: string | null,
): string {
  const d = date.substring(0, 10);
  return targetName ? `${d} — ${targetName}` : d;
}

function LoadingSkeleton() {
  return (
    <>
      <div className={`${styles.skeleton} ${styles.skeletonHeader}`} />
      <div className={`${styles.skeleton} ${styles.skeletonBlock}`} />
      <div className={`${styles.skeleton} ${styles.skeletonBlock}`} />
      <div className={`${styles.skeleton} ${styles.skeletonAudit}`} />
    </>
  );
}

export function MaintenanceDetailPage() {
  const { id } = useParams<{ id: string }>();
  const recordId = id ? Number.parseInt(id, 10) : undefined;
  const { data, isLoading, error } = useMaintenanceDetail(recordId);

  if (recordId === undefined || Number.isNaN(recordId)) {
    return (
      <div className={styles.page}>
        <Breadcrumbs items={crumbs(undefined)} />
        <div className={styles.errorBox}>Invalid maintenance id in URL.</div>
      </div>
    );
  }

  if (isLoading) {
    return (
      <div className={styles.page}>
        <Breadcrumbs items={crumbs(undefined)} />
        <LoadingSkeleton />
      </div>
    );
  }

  if (error || !data) {
    return (
      <div className={styles.page}>
        <Breadcrumbs items={crumbs(undefined)} />
        <div className={styles.errorBox}>
          Failed to load maintenance record: {(error as Error)?.message ?? 'unknown error'}
        </div>
      </div>
    );
  }

  const targetName = data.record.controller_id
    ? data.record.controller_name
    : data.record.aircraft_name;
  const label = buildLabel(data.record.performed_date, targetName);

  return (
    <div className={styles.page}>
      <Breadcrumbs items={crumbs(label)} />
      <MaintenanceHeaderCard record={data.record} />
      <MaintenanceBodyCard record={data.record} />
      <MaintenanceAuditCard audit={data.audit} />
    </div>
  );
}
