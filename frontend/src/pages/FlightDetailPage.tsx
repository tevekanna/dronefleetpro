// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useParams } from 'react-router-dom';
import { Breadcrumbs } from '../components/Breadcrumbs/Breadcrumbs';
import type { BreadcrumbItem } from '../components/Breadcrumbs/Breadcrumbs';
import { useFlightDetail } from '../hooks/useFlightDetail';
import { FlightHeaderCard } from '../components/flight-detail/FlightHeaderCard';
import { FlightMapCard } from '../components/flight-detail/FlightMapCard';
import { FlightFindingsCard } from '../components/flight-detail/FlightFindingsCard';
import { FlightStatsGrid } from '../components/flight-detail/FlightStatsGrid';
import { FlightEasaCard } from '../components/flight-detail/FlightEasaCard';
import { FlightRemarksCard } from '../components/flight-detail/FlightRemarksCard';
import { FlightAircraftMaintenance } from '../components/flight-detail/FlightAircraftMaintenance';
import { FlightAuditCard } from '../components/flight-detail/FlightAuditCard';
import styles from './FlightDetailPage.module.css';

function flightCrumbs(label: string | undefined): BreadcrumbItem[] {
  return [
    { label: 'Flight Log', to: '/flights' },
    { label: label ?? '…' },
  ];
}

function formatFlightLabel(start: string | null): string {
  if (!start) return 'Flight';
  return `${start.substring(0, 10)} ${start.substring(11, 16)}`;
}

function LoadingSkeleton() {
  return (
    <>
      <div className={`${styles.skeleton} ${styles.skeletonHeader}`} />
      <div className={styles.skeletonStatsRow}>
        {Array.from({ length: 4 }).map((_, i) => (
          <div key={i} className={`${styles.skeleton} ${styles.skeletonStat}`} />
        ))}
      </div>
      <div className={`${styles.skeleton} ${styles.skeletonMap}`} />
    </>
  );
}

export function FlightDetailPage() {
  const { id } = useParams<{ id: string }>();
  const flightId = id ? Number.parseInt(id, 10) : undefined;
  const { data, isLoading, error } = useFlightDetail(flightId);

  if (flightId === undefined || Number.isNaN(flightId)) {
    return (
      <div className={styles.page}>
        <Breadcrumbs items={flightCrumbs(undefined)} />
        <div className={styles.errorBox}>Invalid flight id in URL.</div>
      </div>
    );
  }

  if (isLoading) {
    return (
      <div className={styles.page}>
        <Breadcrumbs items={flightCrumbs(undefined)} />
        <LoadingSkeleton />
      </div>
    );
  }

  if (error || !data) {
    return (
      <div className={styles.page}>
        <Breadcrumbs items={flightCrumbs(undefined)} />
        <div className={styles.errorBox}>
          Failed to load flight: {(error as Error)?.message ?? 'unknown error'}
        </div>
      </div>
    );
  }

  const startTs = data.flight.flight_start ?? data.flight.flight_datetime;

  return (
    <div className={styles.page}>
      <Breadcrumbs items={flightCrumbs(formatFlightLabel(startTs))} />
      <FlightHeaderCard
        flight={data.flight}
        aircraft={data.aircraft}
        battery={data.battery}
        controller={data.controller}
        pilot={data.pilot}
      />
      <FlightStatsGrid flight={data.flight} />
      <FlightMapCard flight={data.flight} />
      <FlightFindingsCard findings={data.findings} />
      <FlightEasaCard flight={data.flight} />
      <FlightRemarksCard flight={data.flight} />
      <FlightAircraftMaintenance schedules={data.aircraft_schedules} />
      <FlightAuditCard flight={data.flight} audit={data.audit} />
    </div>
  );
}
