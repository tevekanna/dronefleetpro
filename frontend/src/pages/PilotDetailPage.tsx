// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useParams } from 'react-router-dom';
import { Breadcrumbs } from '../components/Breadcrumbs/Breadcrumbs';
import { usePilotDetail } from '../hooks/usePilotDetail';
import { PilotHeaderCard } from '../components/pilot-detail/PilotHeaderCard';
import { PilotStatsCards } from '../components/pilot-detail/PilotStatsCards';
import { PilotQualifications } from '../components/pilot-detail/PilotQualifications';
import { PilotBreakdownsPanel } from '../components/pilot-detail/PilotBreakdownsPanel';
import { PilotFlightLog } from '../components/pilot-detail/PilotFlightLog';
import { PilotIncidents } from '../components/pilot-detail/PilotIncidents';
import styles from './PilotDetailPage.module.css';

function pilotCrumbs(pilotName: string | undefined) {
  return [
    { label: 'Pilots', to: '/pilots' },
    { label: pilotName ?? '…' },
  ];
}

function LoadingSkeleton() {
  return (
    <div className={styles.page}>
      <div className={`${styles.skeleton} ${styles.skeletonHeader}`} />
      <div className={styles.skeletonStats}>
        <div className={`${styles.skeleton} ${styles.skeletonStat}`} />
        <div className={`${styles.skeleton} ${styles.skeletonStat}`} />
        <div className={`${styles.skeleton} ${styles.skeletonStat}`} />
        <div className={`${styles.skeleton} ${styles.skeletonStat}`} />
      </div>
      <div className={styles.twoColumn}>
        <div className={`${styles.skeleton} ${styles.skeletonBlock}`} />
        <div className={`${styles.skeleton} ${styles.skeletonBlock}`} />
      </div>
    </div>
  );
}

export function PilotDetailPage() {
  const { id } = useParams<{ id: string }>();
  const pilotId = id ? Number.parseInt(id, 10) : undefined;
  const { data, isLoading, error } = usePilotDetail(pilotId);

  if (pilotId === undefined || Number.isNaN(pilotId)) {
    return (
      <div className={styles.page}>
        <Breadcrumbs items={pilotCrumbs(undefined)} />
        <div className={styles.errorBox}>Invalid pilot id in URL.</div>
      </div>
    );
  }

  if (isLoading) {
    return (
      <div className={styles.page}>
        <Breadcrumbs items={pilotCrumbs(undefined)} />
        <LoadingSkeleton />
      </div>
    );
  }

  if (error || !data) {
    return (
      <div className={styles.page}>
        <Breadcrumbs items={pilotCrumbs(undefined)} />
        <div className={styles.errorBox}>
          Failed to load pilot detail: {(error as Error)?.message ?? 'unknown error'}
        </div>
      </div>
    );
  }

  return (
    <div className={styles.page}>
      <Breadcrumbs items={pilotCrumbs(data.pilot.name)} />
      <PilotHeaderCard pilot={data.pilot} />
      <PilotStatsCards stats={data.stats} />
      <div className={styles.twoColumn}>
        <PilotQualifications qualifications={data.qualifications} />
        <PilotBreakdownsPanel breakdowns={data.breakdowns} />
      </div>
      <PilotFlightLog pilotId={pilotId} />
      <PilotIncidents incidents={data.recent_incidents} />
    </div>
  );
}
