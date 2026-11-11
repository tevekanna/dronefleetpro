// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useState } from 'react';
import { useParams, useSearchParams } from 'react-router-dom';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { Breadcrumbs } from '../components/Breadcrumbs/Breadcrumbs';
import { useAircraftDetail } from '../hooks/useAircraftDetail';
import {
  fetchAircraftMaintenanceDetail,
  type AircraftMaintenanceDetail,
} from '../api/maintenanceAircraft';
import { AircraftWorkspaceHeader } from '../components/aircraft-workspace/AircraftWorkspaceHeader';
import {
  AircraftWorkspaceTabs,
  type TabDef,
  type WorkspaceTab,
} from '../components/aircraft-workspace/AircraftWorkspaceTabs';
import { OverviewTab } from '../components/aircraft-workspace/OverviewTab';
import { AircraftMaintenanceTab } from '../components/aircraft-workspace/AircraftMaintenanceTab';
import { ModelChecklistsTab } from '../components/aircraft-workspace/ModelChecklistsTab';
import { AircraftFlightLog } from '../components/aircraft-detail/AircraftFlightLog';
import { AirworthinessModal } from '../components/maintenance/AirworthinessModal';
import { AddDefectModal } from '../components/maintenance/AddDefectModal';
import { canEdit as getCanEdit, canReportDefect as getCanReportDefect, hasAnyRole } from '../utils/auth';
import workspaceStyles from '../components/aircraft-workspace/AircraftWorkspace.module.css';
import styles from './AircraftDetailPage.module.css';

// Roles allowed to read the per-aircraft maintenance endpoint. Viewers are
// excluded, so they never see the Maintenance tab (see below).
const MAINTENANCE_ROLES = ['admin', 'technician', 'safety_officer', 'pilot'];

const VALID_TABS: WorkspaceTab[] = ['overview', 'maintenance', 'flights', 'model'];

function crumbs(label: string) {
  return [{ label: 'Fleet', to: '/fleet' }, { label }];
}

/**
 * Outer guard: validate the :id param and mount the workspace body only when
 * it is a real number. Keeping the data hooks in the inner component means
 * they always run in the same order (Rules of Hooks) regardless of the guard.
 */
export function AircraftDetailPage() {
  const { id } = useParams<{ id: string }>();
  const aircraftId = id ? Number.parseInt(id, 10) : NaN;

  if (!Number.isFinite(aircraftId)) {
    return (
      <div className={workspaceStyles.page}>
        <Breadcrumbs items={crumbs('…')} />
        <div className={workspaceStyles.errorBox}>Invalid aircraft id in URL.</div>
      </div>
    );
  }

  return <AircraftWorkspaceBody aircraftId={aircraftId} />;
}

function AircraftWorkspaceBody({ aircraftId }: { aircraftId: number }) {
  const [searchParams, setSearchParams] = useSearchParams();
  const queryClient = useQueryClient();

  const canSeeMaintenance = hasAnyRole(...MAINTENANCE_ROLES);
  const userCanEdit = getCanEdit();
  const userCanReportDefect = getCanReportDefect();
  const isAdmin = hasAnyRole('admin');

  const { data, isLoading, error } = useAircraftDetail(aircraftId);

  const maintenanceQuery = useQuery<AircraftMaintenanceDetail>({
    queryKey: ['aircraft-maintenance-detail', aircraftId],
    queryFn: () => fetchAircraftMaintenanceDetail(aircraftId),
    enabled: canSeeMaintenance,
  });
  const maintenance = maintenanceQuery.data;

  const [showDefectModal, setShowDefectModal] = useState(false);
  const [showAirworthinessModal, setShowAirworthinessModal] = useState(false);

  const tabs: TabDef[] = [
    { key: 'overview', label: 'Overview' },
    ...(canSeeMaintenance ? [{ key: 'maintenance' as const, label: 'Maintenance' }] : []),
    { key: 'flights', label: 'Flights' },
    // Model-scoped management sits under the same gate as Maintenance
    // (viewer excluded).
    ...(canSeeMaintenance ? [{ key: 'model' as const, label: 'Model & checklists' }] : []),
  ];

  // Resolve the active tab from ?tab, falling back to overview for unknown
  // values or when a viewer deep-links to the maintenance tab they can't read.
  const requestedTab = searchParams.get('tab') as WorkspaceTab | null;
  const activeTab: WorkspaceTab =
    requestedTab && VALID_TABS.includes(requestedTab) && tabs.some((t) => t.key === requestedTab)
      ? requestedTab
      : 'overview';

  const selectTab = (tab: WorkspaceTab) => {
    const next = new URLSearchParams(searchParams);
    next.set('tab', tab);
    setSearchParams(next, { replace: false });
  };

  if (isLoading) {
    return (
      <div className={workspaceStyles.page}>
        <Breadcrumbs items={crumbs('…')} />
        <div className={`${workspaceStyles.skeleton} ${workspaceStyles.skeletonHeader}`} />
        <div className={styles.statsGrid}>
          <div className={`${styles.skeleton} ${styles.skeletonStat}`} />
          <div className={`${styles.skeleton} ${styles.skeletonStat}`} />
          <div className={`${styles.skeleton} ${styles.skeletonStat}`} />
          <div className={`${styles.skeleton} ${styles.skeletonStat}`} />
        </div>
      </div>
    );
  }

  if (error || !data) {
    return (
      <div className={workspaceStyles.page}>
        <Breadcrumbs items={crumbs('…')} />
        <div className={workspaceStyles.errorBox}>
          Failed to load aircraft: {(error as Error)?.message ?? 'unknown error'}
        </div>
      </div>
    );
  }

  const { aircraft, stats } = data;
  const label = aircraft.equipment_id ?? aircraft.serial_number;
  const model = aircraft.model ? { id: aircraft.model.id, name: aircraft.model.name } : null;

  return (
    <div className={workspaceStyles.page}>
      <Breadcrumbs items={crumbs(label)} />

      <AircraftWorkspaceHeader
        title={label}
        aircraftName={maintenance?.aircraft_name ?? aircraft.nickname}
        serialNumber={aircraft.serial_number}
        model={model}
        status={aircraft.status}
        totalFlights={stats.total_flights}
        totalSeconds={stats.total_seconds}
        lastFlight={stats.last_flight}
        airworthiness={
          maintenance
            ? {
                status: maintenance.airworthiness_status,
                reason: maintenance.airworthiness_reason,
              }
            : null
        }
        canReportDefect={userCanReportDefect}
        canEdit={userCanEdit}
        onReportDefect={() => setShowDefectModal(true)}
        onChangeStatus={() => setShowAirworthinessModal(true)}
      />

      <AircraftWorkspaceTabs tabs={tabs} active={activeTab} onSelect={selectTab} />

      {activeTab === 'overview' && (
        <OverviewTab
          aircraftId={aircraftId}
          stats={stats}
          model={model}
          takeoffMass={data.takeoff_mass}
          maintenance={maintenance}
          mounted={data.mounted}
          canEdit={userCanEdit}
        />
      )}

      {activeTab === 'maintenance' && canSeeMaintenance && (
        <>
          {maintenanceQuery.isLoading && (
            <div className={`${workspaceStyles.skeleton} ${workspaceStyles.skeletonPanel}`} />
          )}
          {maintenanceQuery.error && (
            <div className={workspaceStyles.errorBox}>
              Failed to load maintenance:{' '}
              {(maintenanceQuery.error as Error)?.message ?? 'unknown error'}
            </div>
          )}
          {maintenance && (
            <AircraftMaintenanceTab
              detail={maintenance}
              onReportDefect={() => setShowDefectModal(true)}
              canEdit={userCanEdit}
              canReportDefect={userCanReportDefect}
              isAdmin={isAdmin}
            />
          )}
        </>
      )}

      {activeTab === 'flights' && <AircraftFlightLog aircraftId={aircraftId} />}

      {activeTab === 'model' && canSeeMaintenance && (
        <ModelChecklistsTab
          modelId={model?.id ?? null}
          modelName={model?.name ?? null}
          isAdmin={isAdmin}
        />
      )}

      {userCanReportDefect && showDefectModal && (
        <AddDefectModal
          aircraftId={aircraftId}
          aircraftLabel={label}
          onClose={() => setShowDefectModal(false)}
          onSuccess={() => {
            setShowDefectModal(false);
            void queryClient.invalidateQueries({
              queryKey: ['aircraft-maintenance-detail', aircraftId],
            });
          }}
        />
      )}

      {userCanEdit && maintenance && showAirworthinessModal && (
        <AirworthinessModal
          aircraftId={aircraftId}
          aircraftLabel={label}
          currentStatus={maintenance.airworthiness_status}
          onClose={() => setShowAirworthinessModal(false)}
          onSuccess={() => {
            setShowAirworthinessModal(false);
            void queryClient.invalidateQueries({
              queryKey: ['aircraft-maintenance-detail', aircraftId],
            });
            void queryClient.invalidateQueries({ queryKey: ['maintenance-fleet-status'] });
            void queryClient.invalidateQueries({ queryKey: ['aircraft'] });
            void queryClient.invalidateQueries({ queryKey: ['aircraft-detail', aircraftId] });
          }}
        />
      )}
    </div>
  );
}
