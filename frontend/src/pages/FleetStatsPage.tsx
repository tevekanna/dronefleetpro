// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useCallback, useState } from 'react';
import { useSearchParams } from 'react-router-dom';
import { Download, FileText } from 'lucide-react';
import {
  useFleetDashboard,
  useFleetStatsOverview,
} from '../hooks/useFleetStats';
import {
  downloadBatteriesCsv,
  downloadControllersCsv,
} from '../api/fleetStats';
import { exportFleetStatsSummary } from '../services/pdfExport';
import { showToast } from '../store/toastStore';
import { AircraftTab } from '../components/fleet-stats/AircraftTab';
import { BatteriesTab } from '../components/fleet-stats/BatteriesTab';
import { ControllersTab } from '../components/fleet-stats/ControllersTab';
import { KpiStrip } from '../components/fleet-stats/KpiStrip';
import { OverviewTab } from '../components/fleet-stats/OverviewTab';
import { Tabs, type TabDef } from '../components/fleet-stats/Tabs';
import styles from './FleetStatsShared.module.css';

type TabId = 'overview' | 'aircraft' | 'batteries' | 'controllers';

const TABS: TabDef<TabId>[] = [
  { id: 'overview', label: 'Overview' },
  { id: 'aircraft', label: 'Aircraft' },
  { id: 'batteries', label: 'Batteries' },
  { id: 'controllers', label: 'Controllers' },
];

const VALID_TAB_IDS: TabId[] = ['overview', 'aircraft', 'batteries', 'controllers'];

function parseTab(raw: string | null): TabId {
  if (raw && (VALID_TAB_IDS as string[]).includes(raw)) {
    return raw as TabId;
  }
  return 'overview';
}

export function FleetStatsPage() {
  const { data, isLoading, error } = useFleetDashboard();
  // The legacy overview is still loaded so the existing PDF export
  // (which expects the FleetStatsOverview shape) keeps working.
  const { data: legacyOverview } = useFleetStatsOverview();

  const [searchParams, setSearchParams] = useSearchParams();
  const activeTab = parseTab(searchParams.get('tab'));

  const setActiveTab = useCallback(
    (id: TabId) => {
      const next = new URLSearchParams(searchParams);
      if (id === 'overview') {
        next.delete('tab');
      } else {
        next.set('tab', id);
      }
      setSearchParams(next, { replace: false });
    },
    [searchParams, setSearchParams],
  );

  const [exporting, setExporting] = useState<'batteries' | 'controllers' | 'pdf' | null>(null);
  const handleExport = async (kind: 'batteries' | 'controllers' | 'pdf') => {
    setExporting(kind);
    try {
      if (kind === 'batteries') await downloadBatteriesCsv();
      else if (kind === 'controllers') await downloadControllersCsv();
      else if (kind === 'pdf') {
        if (!legacyOverview) {
          throw new Error('Overview data not loaded yet.');
        }
        await exportFleetStatsSummary(legacyOverview);
      }
      showToast(`${kind.toUpperCase()} export started`, 'success');
    } catch (e) {
      showToast(`Export failed: ${(e as Error).message}`, 'error');
    } finally {
      setExporting(null);
    }
  };

  return (
    <div className={styles.page}>
      <div className={styles.header}>
        <div>
          <h1 className={styles.title}>Fleet Statistics</h1>
          <div className={styles.description}>
            Live snapshot of fleet health, recent activity, and maintenance status.
          </div>
        </div>
        <div className={styles.actions}>
          <button
            type="button"
            className={styles.btn}
            onClick={() => handleExport('batteries')}
            disabled={exporting !== null}
          >
            <Download size={14} /> Batteries CSV
          </button>
          <button
            type="button"
            className={styles.btn}
            onClick={() => handleExport('controllers')}
            disabled={exporting !== null}
          >
            <Download size={14} /> Controllers CSV
          </button>
          <button
            type="button"
            className={`${styles.btn} ${styles.btnPrimary}`}
            onClick={() => handleExport('pdf')}
            disabled={exporting !== null || !legacyOverview}
          >
            <FileText size={14} /> Summary PDF
          </button>
        </div>
      </div>

      {isLoading && <p className={styles.loadingMsg}>Loading dashboard…</p>}
      {error && (
        <p className={styles.errorMsg}>Failed to load: {(error as Error).message}</p>
      )}

      {data && <KpiStrip kpis={data.kpis} />}

      <Tabs tabs={TABS} active={activeTab} onChange={setActiveTab}>
        {activeTab === 'overview' && data && <OverviewTab data={data} />}
        {activeTab === 'aircraft' && <AircraftTab />}
        {activeTab === 'batteries' && <BatteriesTab />}
        {activeTab === 'controllers' && <ControllersTab />}
      </Tabs>
    </div>
  );
}
