// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { Link } from 'react-router-dom';
import type {
  AircraftRef,
  BatteryRef,
  ControllerRef,
  PilotRef,
} from '../../api/flightDetail';
import type { Flight } from '../../types/flight';
import { formatAircraftCell } from '../../utils/aircraftFormat';
import styles from '../../pages/FlightDetailPage.module.css';
import { formatDate, formatDuration, formatTime, sourceLabel } from './flightFormatters';

function SourceBadge({ source }: { source: string | null }) {
  const label = sourceLabel(source);
  let cls = styles.sourceBadgeManual;
  if (source === 'dji_import') cls = styles.sourceBadgeDji;
  else if (source === 'csv_import') cls = styles.sourceBadgeLitchi;
  return <span className={`${styles.sourceBadge} ${cls}`}>{label}</span>;
}

function EquipmentBlock({
  label,
  primary,
  secondary,
  linkTo,
}: {
  label: string;
  primary: string;
  secondary?: string | null;
  /** When set, the primary value renders as a Link to this path. */
  linkTo?: string;
}) {
  const valueClass = linkTo ? `${styles.metaValue} ${styles.metaLink}` : styles.metaValue;
  const value = linkTo ? (
    <Link to={linkTo} className={valueClass}>
      {primary}
    </Link>
  ) : (
    <span className={valueClass}>{primary}</span>
  );
  return (
    <div className={styles.metaItem}>
      <span className={styles.metaLabel}>{label}</span>
      <div className={styles.equipmentLine}>
        {value}
        {secondary && <span className={styles.equipmentSecondary}>{secondary}</span>}
      </div>
    </div>
  );
}

function aircraftPrimary(a: AircraftRef): string {
  return formatAircraftCell(a).primary;
}
function aircraftSecondary(a: AircraftRef): string | null {
  return formatAircraftCell(a).secondary;
}

function batteryPrimary(b: BatteryRef): string {
  return b.equipment_id ?? b.serial_number;
}
function batterySecondary(b: BatteryRef): string | null {
  const parts: string[] = [];
  if (b.compatible_model) parts.push(b.compatible_model);
  if (b.equipment_id && b.equipment_id !== b.serial_number) parts.push(b.serial_number);
  return parts.length ? parts.join(' · ') : null;
}

function controllerPrimary(c: ControllerRef): string {
  return c.equipment_id ?? c.nickname ?? c.serial_number;
}
function controllerSecondary(c: ControllerRef): string | null {
  const parts: string[] = [];
  if (c.model) parts.push(c.model);
  if (c.equipment_id && c.equipment_id !== c.serial_number) parts.push(c.serial_number);
  return parts.length ? parts.join(' · ') : null;
}

interface FlightHeaderCardProps {
  flight: Flight;
  aircraft: AircraftRef | null;
  battery: BatteryRef | null;
  controller: ControllerRef | null;
  pilot: PilotRef | null;
}

export function FlightHeaderCard({
  flight,
  aircraft,
  battery,
  controller,
  pilot,
}: FlightHeaderCardProps) {
  const startTs = flight.flight_start ?? flight.flight_datetime;
  const endTs = flight.flight_end;
  const subtitle = aircraft
    ? aircraftPrimary(aircraft)
    : flight.aircraft_name ?? flight.aircraft_serial ?? null;

  return (
    <div className={`${styles.section} ${styles.headerCard}`}>
      <div className={styles.headerTopRow}>
        <div>
          <div className={styles.flightTitle}>Flight — {formatDate(startTs)}</div>
          {subtitle && <div className={styles.flightSubtitle}>{subtitle}</div>}
        </div>
        <SourceBadge source={flight.log_source} />
      </div>

      <div className={styles.metaGrid}>
        <div className={styles.metaItem}>
          <span className={styles.metaLabel}>Start</span>
          <span className={styles.metaValue}>{formatTime(startTs)}</span>
        </div>
        <div className={styles.metaItem}>
          <span className={styles.metaLabel}>End</span>
          <span className={styles.metaValue}>{formatTime(endTs)}</span>
        </div>
        <div className={styles.metaItem}>
          <span className={styles.metaLabel}>Duration</span>
          <span className={styles.metaValue}>{formatDuration(flight.duration_seconds)}</span>
        </div>

        {pilot ? (
          <div className={styles.metaItem}>
            <span className={styles.metaLabel}>Pilot</span>
            <Link to={`/pilots/${pilot.id}`} className={`${styles.metaValue} ${styles.metaLink}`}>
              {pilot.name}
              {pilot.user_code && (
                <span className={styles.equipmentSecondary} style={{ marginLeft: 'var(--space-2)' }}>
                  {pilot.user_code}
                </span>
              )}
            </Link>
          </div>
        ) : (
          flight.pic_name && (
            <div className={styles.metaItem}>
              <span className={styles.metaLabel}>Pilot</span>
              <span className={styles.metaValue}>{flight.pic_name}</span>
            </div>
          )
        )}

        {aircraft && (
          <EquipmentBlock
            label="Aircraft"
            primary={aircraftPrimary(aircraft)}
            secondary={aircraftSecondary(aircraft)}
            linkTo={`/aircraft/${aircraft.id}`}
          />
        )}
        {battery && (
          <EquipmentBlock
            label="Battery"
            primary={batteryPrimary(battery)}
            secondary={batterySecondary(battery)}
            linkTo={`/fleet-stats/batteries/${battery.id}`}
          />
        )}
        {controller && (
          <EquipmentBlock
            label="Controller"
            primary={controllerPrimary(controller)}
            secondary={controllerSecondary(controller)}
            linkTo={`/fleet-stats/controllers/${controller.id}`}
          />
        )}
      </div>
    </div>
  );
}
