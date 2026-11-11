// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useState } from 'react';
import { Link } from 'react-router-dom';
import { useQueryClient } from '@tanstack/react-query';
import { CalendarPlus, History } from 'lucide-react';
import { CompleteScheduleModal } from '../maintenance/CompleteScheduleModal';
import { ScheduleFormModal } from '../maintenance/ScheduleFormModal';
import { DeactivateScheduleConfirm } from '../maintenance/DeactivateScheduleConfirm';
import type { AircraftMaintenanceDetail, DraftSummary } from '../../api/maintenanceAircraft';
import type { HubOpenDefect, HubRecentActivity } from '../../api/maintenanceHub';
import type { ScheduleWithStatus } from '../../types/maintenanceSchedule';
import styles from './AircraftWorkspace.module.css';

const INTERVAL_LABEL: Record<string, string> = {
  calendar_days: 'Calendar days',
  flight_hours: 'Flight hours',
  cycles: 'Cycles',
};

const SCHEDULE_STATUS_LABEL: Record<ScheduleWithStatus['status'], string> = {
  ok: 'OK',
  warning: 'DUE SOON',
  overdue: 'OVERDUE',
};

function isoDate(value: string | null): string {
  return value ? value.slice(0, 10) : '—';
}

function truncate(text: string, max: number): string {
  return text.length > max ? `${text.slice(0, max - 1)}…` : text;
}

function nextDueFor(s: ScheduleWithStatus): string {
  switch (s.interval_type) {
    case 'calendar_days':
      return isoDate(s.next_due_at);
    case 'flight_hours':
      return s.next_due_flight_hours != null ? `${s.next_due_flight_hours.toFixed(1)} h` : '—';
    case 'cycles':
      return s.next_due_cycles != null ? `${s.next_due_cycles} cycles` : '—';
    default:
      return '—';
  }
}

function ScheduleStatusPill({ status }: { status: ScheduleWithStatus['status'] }) {
  return (
    <span className={`${styles.statusPill} ${styles[`status_${status}`]}`}>
      {SCHEDULE_STATUS_LABEL[status]}
    </span>
  );
}

function ConsumptionBar({ pct }: { pct: number }) {
  const clamped = Math.max(0, Math.min(100, pct));
  const tone =
    clamped >= 100 ? styles.barOverdue : clamped >= 80 ? styles.barWarning : styles.barOk;
  return (
    <div className={styles.barTrack} aria-label={`${Math.round(clamped)}% consumed`}>
      <div className={`${styles.barFill} ${tone}`} style={{ width: `${clamped}%` }} />
    </div>
  );
}

function SchedulesSection({
  schedules,
  drafts,
  canEdit,
  isAdmin,
  onAdd,
  onPerform,
  onEdit,
  onDeactivate,
}: {
  schedules: ScheduleWithStatus[];
  drafts: DraftSummary[];
  canEdit: boolean;
  isAdmin: boolean;
  onAdd: () => void;
  onPerform: (schedule: ScheduleWithStatus) => void;
  onEdit: (schedule: ScheduleWithStatus) => void;
  onDeactivate: (schedule: ScheduleWithStatus) => void;
}) {
  const draftMap = new Map(drafts.map((d) => [d.schedule_id, d]));
  return (
    <section className={styles.panel}>
      <header className={styles.panelHeader}>
        <h2 className={styles.panelTitle}>Maintenance Schedules</h2>
        {canEdit && (
          <button type="button" className={styles.btnTertiary} onClick={onAdd}>
            <CalendarPlus size={14} aria-hidden />
            Add schedule
          </button>
        )}
      </header>
      {schedules.length === 0 ? (
        <div className={styles.emptyMuted}>No active schedules for this aircraft.</div>
      ) : (
        <ul className={styles.scheduleList}>
          {schedules.map((s) => (
            <li key={s.id} className={styles.scheduleRow}>
              <div className={styles.scheduleMain}>
                <div className={styles.scheduleName}>
                  {s.name}
                  {draftMap.has(s.id) &&
                    (() => {
                      const d = draftMap.get(s.id)!;
                      const progressLabel =
                        d.items_total > 0
                          ? `${d.items_marked}/${d.items_total}`
                          : d.items_marked > 0
                            ? `${d.items_marked} items`
                            : null;
                      return (
                        <>
                          <span className={styles.draftBadge}>ON-GOING</span>
                          {progressLabel && (
                            <span className={styles.draftProgress}>{progressLabel}</span>
                          )}
                        </>
                      );
                    })()}
                </div>
                <div className={styles.scheduleSub}>
                  {INTERVAL_LABEL[s.interval_type] ?? s.interval_type}
                  <span className={styles.dotSep}>·</span>
                  <span>Last: {isoDate(s.last_performed_at)}</span>
                  <span className={styles.dotSep}>·</span>
                  <span>Next: {nextDueFor(s)}</span>
                </div>
              </div>
              <div className={styles.scheduleBar}>
                <ConsumptionBar pct={s.consumption_pct} />
                <span className={styles.scheduleBarPct}>
                  {Math.round(Math.max(0, Math.min(100, s.consumption_pct)))}%
                </span>
              </div>
              <ScheduleStatusPill status={s.status} />
              <div className={styles.scheduleActions}>
                {canEdit && (
                  <button
                    type="button"
                    className={`${styles.btnPerform} ${draftMap.has(s.id) ? styles.btnContinue : ''}`}
                    onClick={() => onPerform(s)}
                  >
                    {draftMap.has(s.id) ? 'Continue' : 'Perform'}
                  </button>
                )}
                {canEdit && (
                  <button
                    type="button"
                    className={styles.btnRowAction}
                    onClick={() => onEdit(s)}
                  >
                    Edit
                  </button>
                )}
                <Link
                  to={`/maintenance-schedules/${s.id}/history`}
                  className={styles.btnRowIcon}
                  aria-label={`History of ${s.name}`}
                  title="View history"
                >
                  <History size={14} />
                </Link>
                {isAdmin && (
                  <button
                    type="button"
                    className={styles.btnRowAction}
                    onClick={() => onDeactivate(s)}
                  >
                    Deactivate
                  </button>
                )}
              </div>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}

function DefectsSection({
  defects,
  onReport,
  canReportDefect,
}: {
  defects: HubOpenDefect[];
  onReport: () => void;
  canReportDefect: boolean;
}) {
  return (
    <section className={styles.panel}>
      <header className={styles.panelHeader}>
        <h2 className={styles.panelTitle}>Open Defects</h2>
        <div className={styles.panelHeaderRight}>
          {defects.length > 0 && <span className={styles.countBadge}>{defects.length}</span>}
          {canReportDefect && (
            <button type="button" className={styles.btnTertiary} onClick={onReport}>
              + Report defect
            </button>
          )}
        </div>
      </header>
      {defects.length === 0 ? (
        <div className={styles.emptyMuted}>No open defects.</div>
      ) : (
        <ul className={styles.simpleList}>
          {defects.map((d) => (
            <li key={d.id} className={styles.simpleRow}>
              <span
                className={`${styles.defectBadge} ${
                  d.status === 'open' ? styles.defectOpen : styles.defectInProgress
                }`}
              >
                {d.status === 'open' ? 'OPEN' : 'IN PROGRESS'}
              </span>
              <span className={styles.mono}>{isoDate(d.defect_date)}</span>
              <span className={styles.rowText}>{truncate(d.description, 60)}</span>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}

function HistorySection({ history }: { history: HubRecentActivity[] }) {
  return (
    <section className={styles.panel}>
      <header className={styles.panelHeader}>
        <h2 className={styles.panelTitle}>Recent Maintenance (last 20)</h2>
      </header>
      {history.length === 0 ? (
        <div className={styles.emptyMuted}>No maintenance records.</div>
      ) : (
        <ul className={styles.simpleList}>
          {history.map((r) => (
            <li key={r.id} className={styles.simpleRow}>
              <span className={styles.mono}>{isoDate(r.performed_date)}</span>
              <span className={styles.rowText}>{r.maint_type ?? '—'}</span>
              <span className={styles.mono}>{r.created_by}</span>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}

interface Props {
  detail: AircraftMaintenanceDetail;
  onReportDefect: () => void;
  canEdit: boolean;
  canReportDefect: boolean;
  isAdmin: boolean;
}

/**
 * Maintenance tab: schedules (with in-place Add/Edit/Deactivate/Perform),
 * open defects, and recent history. Schedule management happens here without
 * navigating away — the previous page sent the operator to
 * /maintenance-schedules, which was the workflow bug this merge fixes.
 */
export function AircraftMaintenanceTab({
  detail,
  onReportDefect,
  canEdit,
  canReportDefect,
  isAdmin,
}: Props) {
  const queryClient = useQueryClient();
  const [performing, setPerforming] = useState<ScheduleWithStatus | null>(null);
  const [editing, setEditing] = useState<ScheduleWithStatus | null>(null);
  const [creating, setCreating] = useState(false);
  const [deactivating, setDeactivating] = useState<ScheduleWithStatus | null>(null);

  // The schedule mutation hooks refresh the schedules list + dashboard, but not
  // the per-aircraft detail or fleet-status views this workspace reads from.
  // Invalidate both whenever a schedule modal closes (success or cancel — the
  // extra refetch on cancel is harmless).
  const refreshAircraftViews = () => {
    void queryClient.invalidateQueries({
      queryKey: ['aircraft-maintenance-detail', detail.aircraft_id],
    });
    void queryClient.invalidateQueries({ queryKey: ['maintenance-fleet-status'] });
  };

  return (
    <>
      <SchedulesSection
        schedules={detail.schedules}
        drafts={detail.drafts}
        canEdit={canEdit}
        isAdmin={isAdmin}
        onAdd={() => setCreating(true)}
        onPerform={(s) => setPerforming(s)}
        onEdit={(s) => setEditing(s)}
        onDeactivate={(s) => setDeactivating(s)}
      />

      <DefectsSection
        defects={detail.open_defects}
        onReport={onReportDefect}
        canReportDefect={canReportDefect}
      />

      <HistorySection history={detail.recent_maintenance} />

      {canEdit && performing && (
        <CompleteScheduleModal
          isOpen
          schedule={performing}
          onClose={() => {
            setPerforming(null);
            refreshAircraftViews();
          }}
        />
      )}

      {canEdit && (creating || editing) && (
        <ScheduleFormModal
          isOpen
          aircraftId={detail.aircraft_id}
          aircraftModelId={detail.model_id}
          canCreateTemplate={isAdmin}
          existingSchedule={editing ?? undefined}
          onClose={() => {
            setCreating(false);
            setEditing(null);
            refreshAircraftViews();
          }}
        />
      )}

      {isAdmin && deactivating && (
        <DeactivateScheduleConfirm
          isOpen
          schedule={deactivating}
          onClose={() => {
            setDeactivating(null);
            refreshAircraftViews();
          }}
        />
      )}
    </>
  );
}
