// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import type { FleetDashboard } from '../../types/fleetDashboard';
import { BatteriesNearEolWidget } from './BatteriesNearEolWidget';
import { MaintenanceAlertsWidget } from './MaintenanceAlertsWidget';
import { MostUsedAircraftWidget } from './MostUsedAircraftWidget';
import styles from './fleetStatsDashboard.module.css';

export interface OverviewTabProps {
  data: FleetDashboard;
}

export function OverviewTab({ data }: OverviewTabProps) {
  return (
    <>
      <section className={styles.section}>
        <header className={styles.sectionHeader}>
          <h2 className={styles.sectionTitle}>Most used aircraft</h2>
          <span className={styles.sectionMeta}>by total flight hours</span>
        </header>
        <MostUsedAircraftWidget rows={data.most_used_aircraft} />
      </section>

      <section className={styles.section}>
        <header className={styles.sectionHeader}>
          <h2 className={styles.sectionTitle}>Batteries near end-of-life</h2>
          <span className={styles.sectionMeta}>cycle ratio ≥ 80%</span>
        </header>
        <BatteriesNearEolWidget rows={data.batteries_near_eol_top} />
      </section>

      <section className={styles.section}>
        <header className={styles.sectionHeader}>
          <h2 className={styles.sectionTitle}>Maintenance alerts</h2>
          <span className={styles.sectionMeta}>overdue + due soon</span>
        </header>
        <MaintenanceAlertsWidget rows={data.maintenance_alerts} />
      </section>
    </>
  );
}
