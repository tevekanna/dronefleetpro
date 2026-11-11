// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useNavigate } from 'react-router-dom';
import { CheckCircle2 } from 'lucide-react';
import type {
  BatteryNearEolRow,
  MaintenanceAlertRow,
} from '../../types/fleetDashboard';
import type { OpenDefect } from '../../api/dashboard';
import { formatISODate } from '../../utils/date';
import styles from './dashboardLayout.module.css';

export interface NeedsAttentionPanelProps {
  maintenanceAlerts: MaintenanceAlertRow[];
  openDefects: OpenDefect[];
  batteriesNearEol: BatteryNearEolRow[];
}

const ROWS_PER_SECTION = 5;

function dueLabel(row: MaintenanceAlertRow): { text: string; statusClass: string } {
  if (row.days_overdue !== null) {
    const days = row.days_overdue;
    if (days > 0) {
      return {
        text: days === 1 ? '1 day overdue' : `${days} days overdue`,
        statusClass: styles.statusRed,
      };
    }
    if (days === 0) {
      return { text: 'due today', statusClass: styles.statusAmber };
    }
    const remaining = Math.abs(days);
    return {
      text: remaining === 1 ? 'due in 1 day' : `due in ${remaining} days`,
      statusClass: styles.statusAmber,
    };
  }
  const pct = Math.round(row.consumption_pct * 100);
  return {
    text: `${pct}% consumed`,
    statusClass: row.status === 'overdue' ? styles.statusRed : styles.statusAmber,
  };
}

function severityFromPct(pct: number): 'amber' | 'red' {
  return pct >= 1.0 ? 'red' : 'amber';
}

export function NeedsAttentionPanel({
  maintenanceAlerts,
  openDefects,
  batteriesNearEol,
}: NeedsAttentionPanelProps) {
  const navigate = useNavigate();

  const overdueMaint = maintenanceAlerts.filter((r) => r.status === 'overdue');
  const concerningBatteries = batteriesNearEol.filter((b) => b.cycle_pct >= 0.8);
  const defectsToShow = openDefects.slice(0, ROWS_PER_SECTION);

  return (
    <section className={styles.panel} aria-labelledby="needs-attention-title">
      <header className={styles.panelHeader}>
        <div>
          <h2 id="needs-attention-title" className={styles.panelTitle}>
            Needs attention
          </h2>
          <p className={styles.panelSubtitle}>Items needing your review or action</p>
        </div>
      </header>

      {/* Overdue maintenance */}
      <div className={styles.subsection}>
        <div className={styles.subsectionHeader}>
          <span className={styles.subsectionTitle}>
            Overdue maintenance{overdueMaint.length > 0 && ` (${overdueMaint.length})`}
          </span>
        </div>
        {overdueMaint.length === 0 ? (
          <span className={styles.allClear}>
            <CheckCircle2 size={14} aria-hidden="true" /> No overdue maintenance
          </span>
        ) : (
          <div className={styles.rowList}>
            {overdueMaint.slice(0, ROWS_PER_SECTION).map((row) => {
              const label = dueLabel(row);
              const href = `/maintenance-schedules/${row.schedule_id}/history`;
              return (
                <div
                  key={row.schedule_id}
                  className={styles.row}
                  onClick={() => navigate(href)}
                  role="button"
                  tabIndex={0}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter' || e.key === ' ') {
                      e.preventDefault();
                      navigate(href);
                    }
                  }}
                >
                  <span className={styles.equipmentId}>
                    {row.aircraft_equipment_id ?? row.aircraft_serial_number}
                  </span>
                  <span className={styles.rowPrimary} title={row.schedule_name}>
                    {row.schedule_name}
                    {row.is_critical && (
                      <span className={styles.statusRed}> · critical</span>
                    )}
                  </span>
                  <span className={`${styles.rowStatus} ${label.statusClass}`}>
                    <span
                      className={`${styles.severityDot} ${styles.severityDotRed}`}
                      aria-hidden="true"
                    />
                    {label.text}
                  </span>
                </div>
              );
            })}
          </div>
        )}
      </div>

      {/* Open defects */}
      <div className={styles.subsection}>
        <div className={styles.subsectionHeader}>
          <span className={styles.subsectionTitle}>
            Open defects{openDefects.length > 0 && ` (${openDefects.length})`}
          </span>
        </div>
        {defectsToShow.length === 0 ? (
          <span className={styles.allClear}>
            <CheckCircle2 size={14} aria-hidden="true" /> No open defects
          </span>
        ) : (
          <div className={styles.rowList}>
            {defectsToShow.map((defect) => (
              <div
                key={defect.id}
                className={styles.row}
                onClick={() => navigate('/maintenance?tab=defects')}
                role="button"
                tabIndex={0}
                title="View in Maintenance — Defect Log tab"
                onKeyDown={(e) => {
                  if (e.key === 'Enter' || e.key === ' ') {
                    e.preventDefault();
                    navigate('/maintenance?tab=defects');
                  }
                }}
              >
                {/* TODO(aircraft-display-pt2): pending backend DTO extension — OpenDefect lacks equipment_id */}
                <span className={styles.equipmentId}>
                  {defect.aircraft_name ?? '—'}
                </span>
                <span className={styles.rowPrimary} title={defect.description}>
                  {defect.description}
                </span>
                <span className={`${styles.rowStatus} ${styles.statusRed}`}>
                  <span
                    className={`${styles.severityDot} ${styles.severityDotRed}`}
                    aria-hidden="true"
                  />
                  {formatISODate(defect.defect_date)} · {defect.reported_by}
                </span>
              </div>
            ))}
          </div>
        )}
      </div>

      {/* Batteries near EOL */}
      <div className={styles.subsection}>
        <div className={styles.subsectionHeader}>
          <span className={styles.subsectionTitle}>
            Batteries near EOL{concerningBatteries.length > 0 && ` (${concerningBatteries.length})`}
          </span>
        </div>
        {concerningBatteries.length === 0 ? (
          <span className={styles.allClear}>
            <CheckCircle2 size={14} aria-hidden="true" /> All batteries healthy
          </span>
        ) : (
          <div className={styles.rowList}>
            {concerningBatteries.slice(0, ROWS_PER_SECTION).map((battery) => {
              const sev = severityFromPct(battery.cycle_pct);
              const pctNum = Math.round(battery.cycle_pct * 100);
              const statusClass = sev === 'red' ? styles.statusRed : styles.statusAmber;
              const dotClass = sev === 'red' ? styles.severityDotRed : styles.severityDotAmber;
              return (
                <div
                  key={battery.battery_id}
                  className={styles.row}
                  onClick={() =>
                    navigate(`/fleet-stats/batteries/${battery.battery_id}`)
                  }
                  role="button"
                  tabIndex={0}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter' || e.key === ' ') {
                      e.preventDefault();
                      navigate(`/fleet-stats/batteries/${battery.battery_id}`);
                    }
                  }}
                >
                  <span className={styles.equipmentId}>
                    {battery.equipment_id ?? battery.serial_number}
                  </span>
                  <span className={styles.rowPrimary}>
                    {battery.model ?? <span className={styles.muted}>—</span>}
                  </span>
                  <span className={`${styles.rowStatus} ${statusClass}`}>
                    <span
                      className={`${styles.severityDot} ${dotClass}`}
                      aria-hidden="true"
                    />
                    {battery.cycle_count}/{battery.max_cycles} ({pctNum}%)
                  </span>
                </div>
              );
            })}
          </div>
        )}
      </div>
    </section>
  );
}
