// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import type {
  AircraftDetailStats,
  MountedEquipment,
  TakeoffMass,
} from '../../api/aircraftDetail';
import type { AircraftMaintenanceDetail } from '../../api/maintenanceAircraft';
import { AircraftStatsCards } from '../aircraft-detail/AircraftStatsCards';
import { AircraftModelDocsCard } from '../aircraft-detail/AircraftModelDocsCard';
import { AircraftBreakdownsPanel } from '../aircraft-detail/AircraftBreakdownsPanel';
import { MountedEquipmentCard } from './MountedEquipmentCard';
import styles from './AircraftWorkspace.module.css';

interface Props {
  aircraftId: number;
  stats: AircraftDetailStats;
  model: { id: number; name: string } | null;
  /** Server-computed current-configuration takeoff mass + advisory MTOM. */
  takeoffMass: TakeoffMass;
  /** Present for non-viewer roles; drives the compact maintenance summary. */
  maintenance: AircraftMaintenanceDetail | undefined;
  /** Equipment currently mounted on this airframe (active rows only). */
  mounted: MountedEquipment;
  /** Admin/technician gate for the per-row Unmount action. */
  canEdit: boolean;
}

/** Kilogram figure rounded to 2 decimals, trailing zeros dropped. */
function fmtKg(v: number): string {
  return String(Math.round(v * 100) / 100);
}

/** Plain-English reasons why the takeoff mass could not be computed. */
function incompleteReasons(tm: TakeoffMass): string {
  const reasons: string[] = [];
  if (tm.empty_weight_missing) reasons.push('Empty weight not recorded');
  if (tm.batteries_missing_weight > 0) {
    reasons.push(
      `${tm.batteries_missing_weight} mounted ${
        tm.batteries_missing_weight === 1 ? 'battery' : 'batteries'
      } missing weight`,
    );
  }
  if (tm.accessories_missing_weight > 0) {
    reasons.push(
      `${tm.accessories_missing_weight} mounted ${
        tm.accessories_missing_weight === 1 ? 'accessory' : 'accessories'
      } missing weight`,
    );
  }
  return reasons.join('; ');
}

/**
 * Airframe facts strip — individual-airframe attributes (as opposed to the
 * model-level catalogue figures shown on the aircraft-model detail page).
 * An unrecorded empty weight reads "Not recorded", never 0 kg; an incomplete
 * takeoff mass reads "Incomplete" with the reasons, never a silently-low
 * figure. The MTOM comparison is advisory display only.
 */
function AirframeFactsStrip({ takeoffMass: tm }: { takeoffMass: TakeoffMass }) {
  const overMtom = tm.total_kg != null && tm.mtom_kg != null && tm.total_kg > tm.mtom_kg;
  const utilisationPct =
    tm.total_kg != null && tm.mtom_kg != null && tm.mtom_kg > 0
      ? Math.round((tm.total_kg / tm.mtom_kg) * 100)
      : null;

  return (
    <div className={styles.summaryStrip}>
      <div className={styles.summaryCell}>
        {tm.empty_weight_kg != null ? (
          <span className={styles.summaryValue}>
            {fmtKg(tm.empty_weight_kg)} kg
            {tm.empty_weight_source === 'model' && (
              <span className={styles.summarySub}> (model)</span>
            )}
          </span>
        ) : (
          <span className={`${styles.summaryValue} ${styles.summaryValueMuted}`}>
            Not recorded
          </span>
        )}
        <span className={styles.summaryLabel}>Empty weight</span>
      </div>

      <div className={styles.summaryCell}>
        {tm.total_kg != null &&
        tm.empty_weight_kg != null &&
        tm.batteries_kg != null &&
        tm.accessories_kg != null ? (
          <>
            <span
              className={`${styles.summaryValue} ${overMtom ? styles.summaryValueCrit : ''}`}
            >
              {fmtKg(tm.total_kg)} kg
            </span>
            <span className={styles.summarySub}>
              {fmtKg(tm.empty_weight_kg)} + {fmtKg(tm.batteries_kg)} +{' '}
              {fmtKg(tm.accessories_kg)} kg
            </span>
            {utilisationPct != null && (
              <span
                className={`${styles.summarySub} ${overMtom ? styles.summarySubCrit : ''}`}
              >
                {overMtom ? `Over MTOM — ${utilisationPct}% of MTOM` : `${utilisationPct}% of MTOM`}
              </span>
            )}
          </>
        ) : (
          <span
            className={`${styles.summaryValue} ${styles.summaryValueMuted}`}
            title={incompleteReasons(tm)}
          >
            Incomplete
          </span>
        )}
        <span className={styles.summaryLabel}>Takeoff mass</span>
      </div>

      <div className={styles.summaryCell}>
        <span
          className={`${styles.summaryValue} ${tm.mtom_kg != null ? '' : styles.summaryValueMuted}`}
        >
          {tm.mtom_kg != null ? `${fmtKg(tm.mtom_kg)} kg` : 'Not set'}
        </span>
        <span className={styles.summaryLabel}>MTOM</span>
      </div>
    </div>
  );
}

/**
 * Compact "next due / open defects" strip derived cheaply from the
 * maintenance-detail query. Rendered only when that data is available (i.e.
 * not for viewer role), so no extra request is made.
 */
function MaintenanceSummaryStrip({ maintenance }: { maintenance: AircraftMaintenanceDetail }) {
  const overdue = maintenance.schedules.filter((s) => s.status === 'overdue').length;
  const dueSoon = maintenance.schedules.filter((s) => s.status === 'warning').length;
  const openDefects = maintenance.open_defects.length;

  return (
    <div className={styles.summaryStrip}>
      <div className={styles.summaryCell}>
        <span className={`${styles.summaryValue} ${overdue > 0 ? styles.summaryValueCrit : ''}`}>
          {overdue}
        </span>
        <span className={styles.summaryLabel}>Overdue schedules</span>
      </div>
      <div className={styles.summaryCell}>
        <span className={`${styles.summaryValue} ${dueSoon > 0 ? styles.summaryValueWarn : ''}`}>
          {dueSoon}
        </span>
        <span className={styles.summaryLabel}>Due soon</span>
      </div>
      <div className={styles.summaryCell}>
        <span
          className={`${styles.summaryValue} ${openDefects > 0 ? styles.summaryValueCrit : ''}`}
        >
          {openDefects}
        </span>
        <span className={styles.summaryLabel}>Open defects</span>
      </div>
    </div>
  );
}

export function OverviewTab({
  aircraftId,
  stats,
  model,
  takeoffMass,
  maintenance,
  mounted,
  canEdit,
}: Props) {
  return (
    <>
      <AircraftStatsCards stats={stats} />
      <AirframeFactsStrip takeoffMass={takeoffMass} />
      <MountedEquipmentCard aircraftId={aircraftId} mounted={mounted} canEdit={canEdit} />
      {maintenance && <MaintenanceSummaryStrip maintenance={maintenance} />}
      <AircraftModelDocsCard model={model} />
      <AircraftBreakdownsPanel aircraftId={aircraftId} />
    </>
  );
}
