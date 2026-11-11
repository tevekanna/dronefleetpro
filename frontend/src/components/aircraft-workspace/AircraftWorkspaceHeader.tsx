// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { Link } from 'react-router-dom';
import styles from './AircraftWorkspace.module.css';

export interface AircraftWorkspaceHeaderProps {
  /** Primary label — equipment id with serial-number fallback. */
  title: string;
  aircraftName: string | null;
  serialNumber: string;
  model: { id: number; name: string } | null;
  status: string;
  totalFlights: number;
  totalSeconds: number;
  lastFlight: string | null;
  /**
   * Airworthiness state, sourced from the maintenance-detail query. Null for
   * roles that cannot read that endpoint (viewer), in which case the pill and
   * status action are omitted.
   */
  airworthiness: { status: 'airworthy' | 'not_airworthy'; reason: string | null } | null;
  canReportDefect: boolean;
  canEdit: boolean;
  onReportDefect: () => void;
  onChangeStatus: () => void;
}

function statusBadgeClass(status: string): string {
  if (status === 'retired') return styles.statusRetired;
  if (status === 'maintenance') return styles.statusMaintenance;
  return styles.statusActive;
}

function statusLabel(status: string): string {
  if (status === 'retired') return 'Retired';
  if (status === 'maintenance') return 'In maintenance';
  return 'Active';
}

function formatHours(seconds: number): string {
  if (seconds <= 0) return '0.0';
  return (seconds / 3600).toFixed(1);
}

function formatLastFlight(iso: string | null): string {
  return iso ? iso.slice(0, 10) : 'Never';
}

/**
 * Persistent identity + key-numbers card shown above the workspace tabs on
 * every tab. Merges the identity/model presentation of the old
 * AircraftHeaderCard with the airworthiness and quick actions of the old
 * per-aircraft maintenance header.
 */
export function AircraftWorkspaceHeader({
  title,
  aircraftName,
  serialNumber,
  model,
  status,
  totalFlights,
  totalSeconds,
  lastFlight,
  airworthiness,
  canReportDefect,
  canEdit,
  onReportDefect,
  onChangeStatus,
}: AircraftWorkspaceHeaderProps) {
  const notAirworthy = airworthiness?.status === 'not_airworthy';

  return (
    <section className={styles.headerCard}>
      <div className={styles.headerLeft}>
        <div className={styles.headerTitleRow}>
          <h1 className={styles.headerTitle}>{title}</h1>
          <span className={`${styles.statusBadge} ${statusBadgeClass(status)}`}>
            {statusLabel(status)}
          </span>
        </div>
        <div className={styles.headerSub}>
          {aircraftName && <span>{aircraftName}</span>}
          {model && (
            <>
              <span className={styles.dotSep}>·</span>
              <Link to={`/aircraft-models/${model.id}`} className={styles.modelLink}>
                {model.name}
              </Link>
            </>
          )}
        </div>
        <div className={styles.headerMeta}>
          <div className={styles.metaCell}>
            <span className={styles.metaLabel}>Serial</span>
            <span className={styles.metaValue}>{serialNumber}</span>
          </div>
          <div className={styles.metaCell}>
            <span className={styles.metaLabel}>Total flights</span>
            <span className={styles.metaValue}>{totalFlights}</span>
          </div>
          <div className={styles.metaCell}>
            <span className={styles.metaLabel}>Total hours</span>
            <span className={styles.metaValue}>{formatHours(totalSeconds)}</span>
          </div>
          <div className={styles.metaCell}>
            <span className={styles.metaLabel}>Last flight</span>
            <span className={styles.metaValue}>{formatLastFlight(lastFlight)}</span>
          </div>
        </div>
      </div>

      <div className={styles.headerRight}>
        {airworthiness && (
          <span
            className={`${styles.airworthyPill} ${
              notAirworthy ? styles.airworthyCritical : styles.airworthyOk
            }`}
          >
            {notAirworthy ? 'NOT AIRWORTHY' : 'AIRWORTHY'}
          </span>
        )}
        {(canReportDefect || (canEdit && airworthiness)) && (
          <div className={styles.headerActions}>
            {canReportDefect && (
              <button type="button" className={styles.btnSecondary} onClick={onReportDefect}>
                Report defect
              </button>
            )}
            {canEdit && airworthiness && (
              <button type="button" className={styles.btnSecondary} onClick={onChangeStatus}>
                Change status
              </button>
            )}
          </div>
        )}
        {notAirworthy && airworthiness?.reason && (
          <span className={styles.airworthinessReason}>Reason: {airworthiness.reason}</span>
        )}
      </div>
    </section>
  );
}
