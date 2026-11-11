// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useEffect, useMemo, useRef, useState } from 'react';
import { AlertTriangle, Plus } from 'lucide-react';
import { useSearchParams } from 'react-router-dom';
import { useQuery } from '@tanstack/react-query';
import { fetchAircraft } from '../api/aircraft';
import type { Aircraft } from '../types/aircraft';
import { CompleteScheduleModal } from '../components/maintenance/CompleteScheduleModal';
import { DeactivateScheduleConfirm } from '../components/maintenance/DeactivateScheduleConfirm';
import { ScheduleFormModal } from '../components/maintenance/ScheduleFormModal';
import { ScheduleRow } from '../components/maintenance/ScheduleRow';
import { SeedDefaultsConfirm } from '../components/maintenance/SeedDefaultsConfirm';
import { ModelAccordion } from '../components/shared/ModelAccordion';
import type { AccordionGroup } from '../components/shared/ModelAccordion';
import { useMaintenanceSchedules } from '../hooks/useMaintenanceSchedules';
import { useSeedDefaultSchedules } from '../hooks/useMaintenanceScheduleTemplates';
import { useCurrentUser } from '../hooks/useCurrentUser';
import { userHasAnyRole } from '../utils/auth';
import { showToast } from '../store/toastStore';
import { formatAircraftLabel } from '../utils/aircraftFormat';
import type {
  ScheduleStatus,
  ScheduleWithStatus,
} from '../types/maintenanceSchedule';
import groupStyles from '../components/maintenance/AircraftScheduleGroup.module.css';
import rowStyles from '../components/maintenance/ScheduleRow.module.css';
import { MaintenanceSubNav } from '../components/maintenance/MaintenanceSubNav';
import styles from './MaintenanceSchedulesPage.module.css';

type StatusFilter = 'all' | ScheduleStatus;

interface GroupedSchedules {
  aircraftId: number;
  equipmentId: string | null;
  model: string | null;
  serialNumber: string;
  schedules: ScheduleWithStatus[];
  worstStatus: ScheduleStatus;
}

const STATUS_RANK: Record<ScheduleStatus, number> = { overdue: 0, warning: 1, ok: 2 };

function worstStatusOf(schedules: ScheduleWithStatus[]): ScheduleStatus {
  return schedules.reduce<ScheduleStatus>(
    (worst, s) => (STATUS_RANK[s.status] < STATUS_RANK[worst] ? s.status : worst),
    'ok',
  );
}

function groupByAircraft(
  schedules: ScheduleWithStatus[],
  aircraftMap: Map<number, Aircraft>,
): GroupedSchedules[] {
  const buckets = new Map<number, ScheduleWithStatus[]>();
  for (const s of schedules) {
    const list = buckets.get(s.aircraft_id) ?? [];
    list.push(s);
    buckets.set(s.aircraft_id, list);
  }
  const groups: GroupedSchedules[] = [];
  for (const [aircraftId, list] of buckets.entries()) {
    const first = list[0];
    const fleetEntry = aircraftMap.get(aircraftId);
    groups.push({
      aircraftId,
      equipmentId: first.aircraft_equipment_id ?? fleetEntry?.equipment_id ?? null,
      model: first.aircraft_model ?? fleetEntry?.model ?? null,
      serialNumber: first.aircraft_serial_number || fleetEntry?.serial_number || `#${aircraftId}`,
      schedules: list,
      worstStatus: worstStatusOf(list),
    });
  }
  return groups.sort((a, b) => {
    const rankDiff = STATUS_RANK[a.worstStatus] - STATUS_RANK[b.worstStatus];
    if (rankDiff !== 0) return rankDiff;
    const aLabel = a.equipmentId ?? a.serialNumber;
    const bLabel = b.equipmentId ?? b.serialNumber;
    return aLabel.localeCompare(bLabel);
  });
}

function sortSchedules(list: ScheduleWithStatus[]): ScheduleWithStatus[] {
  return [...list].sort((a, b) => {
    const rankDiff = STATUS_RANK[a.status] - STATUS_RANK[b.status];
    if (rankDiff !== 0) return rankDiff;
    if (a.next_due_at && b.next_due_at) return a.next_due_at.localeCompare(b.next_due_at);
    if (a.next_due_at) return -1;
    if (b.next_due_at) return 1;
    return b.consumption_pct - a.consumption_pct;
  });
}

function countsByStatus(schedules: ScheduleWithStatus[]): Record<ScheduleStatus, number> {
  return {
    overdue: schedules.filter((s) => s.status === 'overdue').length,
    warning: schedules.filter((s) => s.status === 'warning').length,
    ok: schedules.filter((s) => s.status === 'ok').length,
  };
}

interface UnseededRowProps {
  aircraft: Aircraft;
  canSeed: boolean;
  isSeeding: boolean;
  onSeed: (aircraftId: number) => void;
}

function UnseededAircraftRow({ aircraft, canSeed, isSeeding, onSeed }: UnseededRowProps) {
  const label = formatAircraftLabel(aircraft);
  const hasModel = aircraft.model_id !== null;
  const needsManualSn = aircraft.needs_manual_sn;
  const disabled = !canSeed || !hasModel || needsManualSn || isSeeding;
  const tooltip = needsManualSn
    ? 'Serial number not confirmed yet — review this aircraft in Fleet before seeding.'
    : !hasModel
      ? 'Set a model on this aircraft first (Fleet → edit aircraft).'
      : !canSeed
        ? 'Only admins can seed defaults.'
        : undefined;

  return (
    <div className={styles.unseededRow}>
      <span className={styles.unseededLabel}>{label}</span>
      {!hasModel && (
        <span className={styles.unseededHint}>no model assigned</span>
      )}
      {needsManualSn && (
        <span className={styles.unseededHint}>serial not confirmed</span>
      )}
      <span className={styles.spacer} />
      <button
        type="button"
        className={styles.unseededAction}
        onClick={() => onSeed(aircraft.id)}
        disabled={disabled}
        title={tooltip}
      >
        {isSeeding ? 'Seeding…' : 'Seed defaults'}
      </button>
    </div>
  );
}

export function MaintenanceSchedulesPage() {
  const [searchParams, setSearchParams] = useSearchParams();
  const initialStatus = (searchParams.get('status') as StatusFilter) || 'all';
  const scheduleDeepLink = searchParams.get('schedule');

  const initialAircraft = searchParams.get('aircraft');
  const [statusFilter, setStatusFilter] = useState<StatusFilter>(initialStatus);
  const [aircraftFilter, setAircraftFilter] = useState<number | null>(
    initialAircraft ? Number(initialAircraft) : null,
  );
  const [showInactive, setShowInactive] = useState(false);

  const [completing, setCompleting] = useState<ScheduleWithStatus | null>(null);
  const [editing, setEditing] = useState<ScheduleWithStatus | null>(null);
  const [deactivating, setDeactivating] = useState<ScheduleWithStatus | null>(null);
  const [creatingForAircraft, setCreatingForAircraft] = useState<number | null>(null);
  const [seedingAllOpen, setSeedingAllOpen] = useState(false);
  const [seedingAircraftId, setSeedingAircraftId] = useState<number | null>(null);

  const { data: currentUser } = useCurrentUser();
  const canEdit = userHasAnyRole(currentUser, 'admin', 'technician');
  const canComplete = canEdit;
  const canDeactivate = userHasAnyRole(currentUser, 'admin');
  const isAdmin = canDeactivate;

  const { data: aircraftList } = useQuery({
    queryKey: ['aircraft', true],
    queryFn: () => fetchAircraft(true),
  });

  const seedDefaults = useSeedDefaultSchedules();

  const queryParams = useMemo(
    () => ({
      ...(statusFilter !== 'all' ? { status: statusFilter } : {}),
      ...(aircraftFilter !== null ? { aircraft_id: aircraftFilter } : {}),
      ...(showInactive ? { include_inactive: true } : {}),
    }),
    [statusFilter, aircraftFilter, showInactive],
  );

  const { data: schedules, isLoading, error } = useMaintenanceSchedules(queryParams);

  const aircraftMap = useMemo(() => {
    const m = new Map<number, Aircraft>();
    for (const a of aircraftList ?? []) m.set(a.id, a);
    return m;
  }, [aircraftList]);

  const groups = useMemo(
    () => groupByAircraft(schedules ?? [], aircraftMap),
    [schedules, aircraftMap],
  );

  // Aircraft that have zero schedules. Driven by the full aircraft list
  // (filtered to active) minus the ids that appear in `groups`. Hidden when
  // the user has narrowed the schedule list with a status filter, since
  // aircraft with zero schedules can't satisfy "overdue" / "warning" / "ok".
  const aircraftWithoutSchedules = useMemo(() => {
    if (statusFilter !== 'all' || showInactive) return [];
    const withSchedules = new Set(groups.map((g) => g.aircraftId));
    let list = (aircraftList ?? []).filter(
      (a) => a.status !== 'retired' && !withSchedules.has(a.id),
    );
    if (aircraftFilter !== null) list = list.filter((a) => a.id === aircraftFilter);
    return list.sort((a, b) => {
      const aLabel = a.equipment_id ?? a.serial_number;
      const bLabel = b.equipment_id ?? b.serial_number;
      return aLabel.localeCompare(bLabel);
    });
  }, [aircraftList, groups, aircraftFilter, statusFilter, showInactive]);

  const overdueCount = (schedules ?? []).filter((s) => s.status === 'overdue').length;
  const warningCount = (schedules ?? []).filter((s) => s.status === 'warning').length;

  // Keep URL in sync when the user changes the status filter (deep-link stability).
  useEffect(() => {
    const next = new URLSearchParams(searchParams);
    if (statusFilter === 'all') next.delete('status');
    else next.set('status', statusFilter);
    if (next.toString() !== searchParams.toString()) setSearchParams(next, { replace: true });
  }, [statusFilter, searchParams, setSearchParams]);

  // Deep-link: scroll to ?schedule=<id> once the list has rendered. The
  // accordion item containing it must be expanded so the row is visible —
  // we set the per-aircraft localStorage entry before scrolling. Flash
  // highlight afterwards.
  const flashedRef = useRef<string | null>(null);
  useEffect(() => {
    if (!scheduleDeepLink || isLoading || !schedules) return;
    if (flashedRef.current === scheduleDeepLink) return;
    const targetSchedule = schedules.find((s) => String(s.id) === scheduleDeepLink);
    if (targetSchedule) {
      try {
        localStorage.setItem(
          `maintenance-schedules:${targetSchedule.aircraft_id}`,
          'true',
        );
      } catch {
        // Storage unavailable — accordion will stay collapsed but the
        // page still loads.
      }
    }
    // Defer scroll to next frame so the freshly-expanded accordion has
    // had a chance to render.
    const t = window.setTimeout(() => {
      const el = document.getElementById(`schedule-${scheduleDeepLink}`);
      if (!el) return;
      flashedRef.current = scheduleDeepLink;
      el.scrollIntoView({ behavior: 'smooth', block: 'center' });
      el.classList.add(rowStyles.rowHighlight);
      window.setTimeout(() => el.classList.remove(rowStyles.rowHighlight), 2100);
    }, 50);
    return () => window.clearTimeout(t);
  }, [scheduleDeepLink, schedules, isLoading]);

  const handleSeedAircraft = (aircraftId: number) => {
    setSeedingAircraftId(aircraftId);
    seedDefaults.mutate(
      { aircraft_id: aircraftId },
      {
        onSuccess: (data) => {
          if (data.aircraft_seeded > 0) {
            showToast(
              `Seeded ${data.total_schedules_created} schedule${
                data.total_schedules_created === 1 ? '' : 's'
              } for this aircraft`,
              'success',
            );
          } else if (data.aircraft_skipped_no_templates > 0) {
            showToast(
              "This aircraft's model has no templates yet. Add templates in Schedule Templates first.",
              'info',
            );
          } else if (data.aircraft_skipped_no_model > 0) {
            showToast('Aircraft has no model assigned — set a model first.', 'info');
          } else if (data.aircraft_skipped_had_schedules > 0) {
            showToast('Aircraft already has schedules.', 'info');
          } else if (data.aircraft_skipped_needs_manual_sn > 0) {
            showToast(
              'Serial number not confirmed yet — review this aircraft in Fleet before seeding.',
              'info',
            );
          } else {
            showToast('Nothing to seed.', 'info');
          }
        },
        onError: (e) => {
          showToast(`Seed failed: ${(e as Error).message}`, 'error');
        },
        onSettled: () => {
          setSeedingAircraftId(null);
        },
      },
    );
  };

  const accordionGroups: AccordionGroup[] = useMemo(() => {
    return groups.map((group) => {
      const sorted = sortSchedules(group.schedules);
      const counts = countsByStatus(group.schedules);
      const label = formatAircraftLabel({
        equipment_id: group.equipmentId,
        model: group.model,
        serial_number: group.serialNumber,
      });
      const headerExtras = (
        <div className={groupStyles.headerSummary}>
          {counts.overdue > 0 && (
            <span className={`${groupStyles.pill} ${groupStyles.pillOverdue}`}>
              {counts.overdue} overdue
            </span>
          )}
          {counts.warning > 0 && (
            <span className={`${groupStyles.pill} ${groupStyles.pillWarning}`}>
              {counts.warning} warning
            </span>
          )}
          {counts.overdue === 0 && counts.warning === 0 && counts.ok > 0 && (
            <span className={`${groupStyles.pill} ${groupStyles.pillOk}`}>all ok</span>
          )}
        </div>
      );
      return {
        groupKey: String(group.aircraftId),
        label,
        count: group.schedules.length,
        headerExtras,
        children: (
          <div className={groupStyles.rowsContainer}>
            {sorted.map((schedule) => (
              <ScheduleRow
                key={schedule.id}
                schedule={schedule}
                canEdit={canEdit}
                canComplete={canComplete}
                onEdit={canEdit ? (s) => setEditing(s) : undefined}
                onComplete={canComplete ? (s) => setCompleting(s) : undefined}
                onDeactivate={canDeactivate ? (s) => setDeactivating(s) : undefined}
              />
            ))}
            {canEdit && (
              <button
                type="button"
                className={groupStyles.addButton}
                onClick={() => setCreatingForAircraft(group.aircraftId)}
              >
                <Plus size={14} /> Add schedule
              </button>
            )}
          </div>
        ),
      };
    });
  }, [groups, canEdit, canComplete, canDeactivate]);

  const showEmptyOverall =
    !isLoading &&
    !error &&
    accordionGroups.length === 0 &&
    aircraftWithoutSchedules.length === 0;

  return (
    <div className={styles.page}>
      <MaintenanceSubNav />
      <div className={styles.header}>
        <h1 className={styles.title}>Maintenance Schedules</h1>
        <div className={styles.filters}>
          <select
            className={styles.select}
            value={statusFilter}
            onChange={(e) => setStatusFilter(e.target.value as StatusFilter)}
            aria-label="Filter by status"
          >
            <option value="all">All statuses</option>
            <option value="overdue">Overdue</option>
            <option value="warning">Due soon</option>
            <option value="ok">OK</option>
          </select>
          <select
            className={styles.select}
            value={aircraftFilter ?? ''}
            onChange={(e) => setAircraftFilter(e.target.value ? Number(e.target.value) : null)}
            aria-label="Filter by aircraft"
          >
            <option value="">All aircraft</option>
            {(aircraftList ?? []).map((a) => (
              <option key={a.id} value={a.id}>
                {formatAircraftLabel(a)}
              </option>
            ))}
          </select>
          <label className={styles.toggle}>
            <input
              type="checkbox"
              checked={showInactive}
              onChange={(e) => setShowInactive(e.target.checked)}
            />
            Show inactive
          </label>
        </div>
      </div>

      {overdueCount > 0 && (
        <div className={`${styles.banner} ${styles.bannerOverdue}`} role="alert">
          <AlertTriangle size={18} className={styles.bannerIcon} />
          <span>
            {overdueCount} maintenance {overdueCount === 1 ? 'schedule is' : 'schedules are'}{' '}
            overdue.
          </span>
        </div>
      )}
      {overdueCount === 0 && warningCount > 0 && (
        <div className={`${styles.banner} ${styles.bannerWarning}`}>
          <AlertTriangle size={18} className={styles.bannerIcon} />
          <span>
            {warningCount} maintenance {warningCount === 1 ? 'schedule is' : 'schedules are'} due
            soon.
          </span>
        </div>
      )}

      {isLoading && (
        <div>
          <div className={styles.skeletonRow} />
          <div className={styles.skeletonRow} />
          <div className={styles.skeletonRow} />
        </div>
      )}

      {error && (
        <p className={styles.errorMsg}>Failed to load schedules: {(error as Error).message}</p>
      )}

      {!isLoading && !error && aircraftWithoutSchedules.length > 0 && (
        <section className={styles.unseededSection}>
          <header className={styles.unseededHeader}>
            <h2 className={styles.unseededTitle}>
              Aircraft without schedules
              <span className={styles.unseededCount}>{aircraftWithoutSchedules.length}</span>
            </h2>
            {isAdmin && (
              <button
                type="button"
                className={styles.unseededBulk}
                onClick={() => setSeedingAllOpen(true)}
              >
                Seed all
              </button>
            )}
          </header>
          <p className={styles.unseededIntro}>
            These aircraft have no maintenance schedules. Use the per-row
            action to seed the defaults from the model's templates, or use
            "Seed all" to do every eligible aircraft at once.
          </p>
          <div className={styles.unseededList}>
            {aircraftWithoutSchedules.map((a) => (
              <UnseededAircraftRow
                key={a.id}
                aircraft={a}
                canSeed={isAdmin}
                isSeeding={seedingAircraftId === a.id}
                onSeed={handleSeedAircraft}
              />
            ))}
          </div>
        </section>
      )}

      {!isLoading && !error && accordionGroups.length > 0 && (
        <ModelAccordion
          groups={accordionGroups}
          storageKey="maintenance-schedules"
          defaultOpen={false}
        />
      )}

      {showEmptyOverall && (
        <div className={styles.empty}>
          <span className={styles.emptyTitle}>No maintenance schedules yet.</span>
          <span className={styles.emptyHint}>
            Schedules are auto-created when a new aircraft is registered. Once
            you add aircraft in Fleet, they will appear here.
          </span>
        </div>
      )}

      {completing && (
        <CompleteScheduleModal
          isOpen
          onClose={() => setCompleting(null)}
          schedule={completing}
        />
      )}
      {(editing || creatingForAircraft !== null) && (
        <ScheduleFormModal
          isOpen
          onClose={() => {
            setEditing(null);
            setCreatingForAircraft(null);
          }}
          aircraftId={editing ? editing.aircraft_id : creatingForAircraft ?? 0}
          aircraftModelId={
            editing
              ? editing.aircraft_model_id
              : aircraftMap.get(creatingForAircraft ?? -1)?.model_id ?? null
          }
          canCreateTemplate={isAdmin}
          existingSchedule={editing ?? undefined}
        />
      )}
      {deactivating && (
        <DeactivateScheduleConfirm
          isOpen
          onClose={() => setDeactivating(null)}
          schedule={deactivating}
        />
      )}
      {seedingAllOpen && (
        <SeedDefaultsConfirm
          isOpen
          onClose={() => setSeedingAllOpen(false)}
        />
      )}
    </div>
  );
}
