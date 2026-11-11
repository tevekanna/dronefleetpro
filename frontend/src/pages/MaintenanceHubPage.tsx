// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { MaintenanceSubNav } from '../components/maintenance/MaintenanceSubNav';
import { MaintenanceHubTab } from '../components/maintenance/MaintenanceHubTab';
import styles from './MaintenanceHubPage.module.css';

export function MaintenanceHubPage() {
  return (
    <>
      <MaintenanceSubNav />
      <div className={styles.content}>
        <MaintenanceHubTab />
      </div>
    </>
  );
}
