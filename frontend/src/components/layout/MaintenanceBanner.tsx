// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { AlertTriangle } from 'lucide-react';
import { Link } from 'react-router-dom';
import { useMaintenanceBannerStore } from '../../store/maintenanceBannerStore';
import { useMaintenanceDashboard } from '../../hooks/useMaintenanceSchedules';
import styles from './MaintenanceBanner.module.css';

export function MaintenanceBanner() {
  const { data } = useMaintenanceDashboard();
  const dismissed = useMaintenanceBannerStore((s) => s.dismissed);
  const dismiss = useMaintenanceBannerStore((s) => s.dismiss);

  if (!data || data.overdue === 0 || dismissed) return null;

  const n = data.overdue;
  return (
    <div className={styles.banner} role="alert">
      <AlertTriangle size={16} className={styles.icon} />
      <span>
        {n} maintenance {n === 1 ? 'schedule is' : 'schedules are'} overdue
      </span>
      <Link to="/maintenance-schedules?status=overdue" className={styles.viewLink}>
        View
      </Link>
      <button
        type="button"
        className={styles.closeBtn}
        onClick={dismiss}
        aria-label="Dismiss"
      >
        ×
      </button>
    </div>
  );
}
