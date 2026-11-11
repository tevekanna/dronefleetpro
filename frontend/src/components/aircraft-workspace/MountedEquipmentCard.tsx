// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useState } from 'react';
import { Link } from 'react-router-dom';
import { useMutation, useQuery, useQueryClient } from '@tanstack/react-query';
import type { MountedEquipment } from '../../api/aircraftDetail';
import { fetchAccessories, updateAccessory } from '../../api/accessories';
import { fetchBatteries, updateBattery } from '../../api/batteries';
import { fetchControllers, updateController } from '../../api/controllers';
import type { CreateAccessoryRequest } from '../../types/accessory';
import type { CreateBatteryRequest } from '../../types/battery';
import type { CreateControllerRequest } from '../../types/controller';
import styles from './AircraftWorkspace.module.css';

type EquipmentKind = 'accessory' | 'battery' | 'controller';

interface UnmountTarget {
  kind: EquipmentKind;
  id: number;
  label: string;
}

interface Props {
  aircraftId: number;
  mounted: MountedEquipment;
  canEdit: boolean;
}

const STATUS_LABEL: Record<string, string> = {
  active: 'Active',
  maintenance: 'Maintenance',
};

/**
 * "Mounted equipment" card on the Overview tab — accessories, batteries and
 * controllers whose aircraft_id points at this airframe (active rows only;
 * the backend filters retired). Unmount reuses the equipment update
 * endpoints with aircraft_id: null (tri-state semantics from PR #159),
 * behind a confirmation modal (project convention — never browser confirm()).
 * The card stays visible when empty: mounted state is an affirmative fact.
 */
export function MountedEquipmentCard({ aircraftId, mounted, canEdit }: Props) {
  const queryClient = useQueryClient();
  const [confirmTarget, setConfirmTarget] = useState<UnmountTarget | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [showMountModal, setShowMountModal] = useState(false);

  const unmount = useMutation({
    mutationFn: (target: UnmountTarget): Promise<unknown> => {
      // Same loose-typed pattern as the fleet edit modals: the create-request
      // types say `aircraft_id?: number`, but the update endpoints accept an
      // explicit null to clear the mount (tri-state, PR #159).
      const data: object = { aircraft_id: null };
      if (target.kind === 'accessory') {
        return updateAccessory(
          target.id,
          data as Partial<CreateAccessoryRequest> & Record<string, unknown>,
        );
      }
      if (target.kind === 'battery') {
        return updateBattery(target.id, data as Partial<CreateBatteryRequest>);
      }
      return updateController(target.id, data as Partial<CreateControllerRequest>);
    },
    onSuccess: () => {
      setActionError(null);
      setConfirmTarget(null);
      void queryClient.invalidateQueries({ queryKey: ['aircraft-detail', aircraftId] });
      void queryClient.invalidateQueries({ queryKey: ['accessories'] });
      void queryClient.invalidateQueries({ queryKey: ['batteries'] });
      void queryClient.invalidateQueries({ queryKey: ['controllers'] });
    },
    onError: (err: Error) => setActionError(err.message),
  });

  const openConfirm = (target: UnmountTarget) => {
    setActionError(null);
    setConfirmTarget(target);
  };

  const closeConfirm = () => {
    setActionError(null);
    setConfirmTarget(null);
  };

  const isEmpty =
    mounted.accessories.length === 0 &&
    mounted.batteries.length === 0 &&
    mounted.controllers.length === 0;

  return (
    <div className={styles.mountedCard}>
      <div className={styles.mountedHeadingRow}>
        <div className={styles.mountedHeading}>Mounted equipment</div>
        {canEdit && (
          <button
            type="button"
            className={styles.mountedMountBtn}
            onClick={() => setShowMountModal(true)}
          >
            Mount equipment
          </button>
        )}
      </div>

      {isEmpty && <div className={styles.mountedEmpty}>No equipment mounted.</div>}

      {mounted.accessories.length > 0 && (
        <div className={styles.mountedGroup}>
          <div className={styles.mountedGroupHeading}>Accessories</div>
          <div className={styles.mountedRowList}>
            {mounted.accessories.map((a) => {
              const label = a.equipment_id ?? a.name;
              const meta: string[] = [];
              if (a.serial_number) meta.push(a.serial_number);
              if (a.weight_kg != null) meta.push(`${a.weight_kg} kg`);
              return (
                <EquipmentRow
                  key={a.id}
                  primary={label}
                  meta={meta}
                  status={a.status}
                  canEdit={canEdit}
                  onUnmount={() => openConfirm({ kind: 'accessory', id: a.id, label })}
                />
              );
            })}
          </div>
        </div>
      )}

      {mounted.batteries.length > 0 && (
        <div className={styles.mountedGroup}>
          <div className={styles.mountedGroupHeading}>Batteries</div>
          <div className={styles.mountedRowList}>
            {mounted.batteries.map((b) => {
              const label = b.equipment_id ?? b.serial_number;
              const meta: string[] = [];
              if (b.equipment_id) meta.push(b.serial_number);
              if (b.weight_kg != null) meta.push(`${b.weight_kg} kg`);
              return (
                <EquipmentRow
                  key={b.id}
                  primary={label}
                  primaryLinkTo={`/fleet-stats/batteries/${b.id}`}
                  meta={meta}
                  status={b.status}
                  canEdit={canEdit}
                  onUnmount={() => openConfirm({ kind: 'battery', id: b.id, label })}
                />
              );
            })}
          </div>
        </div>
      )}

      {mounted.controllers.length > 0 && (
        <div className={styles.mountedGroup}>
          <div className={styles.mountedGroupHeading}>Controllers</div>
          <div className={styles.mountedRowList}>
            {mounted.controllers.map((c) => {
              const label = c.equipment_id ?? c.serial_number;
              const meta: string[] = [];
              if (c.equipment_id) meta.push(c.serial_number);
              return (
                <EquipmentRow
                  key={c.id}
                  primary={label}
                  meta={meta}
                  status={c.status}
                  canEdit={canEdit}
                  onUnmount={() => openConfirm({ kind: 'controller', id: c.id, label })}
                />
              );
            })}
          </div>
        </div>
      )}

      {confirmTarget && (
        <UnmountConfirmModal
          target={confirmTarget}
          isPending={unmount.isPending}
          error={actionError}
          onConfirm={() => unmount.mutate(confirmTarget)}
          onCancel={closeConfirm}
        />
      )}

      {showMountModal && (
        <MountEquipmentModal aircraftId={aircraftId} onClose={() => setShowMountModal(false)} />
      )}
    </div>
  );
}

/**
 * Picker for mounting unmounted equipment onto this airframe. Lists only
 * active (non-retired) items with no aircraft link — equipment mounted on
 * ANOTHER aircraft is deliberately absent, so moving it requires an explicit
 * unmount there first (no silent transfer). Mounting is additive and
 * trivially reversible via Unmount, so no per-item confirmation. The modal
 * stays open after a mount so several items can be mounted in a row; the
 * mounted item drops out of the list via query invalidation.
 */
function MountEquipmentModal({
  aircraftId,
  onClose,
}: {
  aircraftId: number;
  onClose: () => void;
}) {
  const queryClient = useQueryClient();
  const [mountError, setMountError] = useState<string | null>(null);

  // Exactly FleetPage's query shapes (['<list>', true] + fetch<List>(true))
  // so the cache entries are genuinely shared with the Fleet tab, not
  // duplicated under mismatched keys. Retired rows are filtered below.
  const accessories = useQuery({
    queryKey: ['accessories', true],
    queryFn: () => fetchAccessories(true),
  });
  const batteries = useQuery({
    queryKey: ['batteries', true],
    queryFn: () => fetchBatteries(true),
  });
  const controllers = useQuery({
    queryKey: ['controllers', true],
    queryFn: () => fetchControllers(true),
  });

  const mount = useMutation({
    mutationFn: (target: { kind: EquipmentKind; id: number }): Promise<unknown> => {
      // Same loose-typed pattern as the unmount mutation above.
      const data: object = { aircraft_id: aircraftId };
      if (target.kind === 'accessory') {
        return updateAccessory(
          target.id,
          data as Partial<CreateAccessoryRequest> & Record<string, unknown>,
        );
      }
      if (target.kind === 'battery') {
        return updateBattery(target.id, data as Partial<CreateBatteryRequest>);
      }
      return updateController(target.id, data as Partial<CreateControllerRequest>);
    },
    onSuccess: () => {
      setMountError(null);
      void queryClient.invalidateQueries({ queryKey: ['aircraft-detail', aircraftId] });
      void queryClient.invalidateQueries({ queryKey: ['accessories'] });
      void queryClient.invalidateQueries({ queryKey: ['batteries'] });
      void queryClient.invalidateQueries({ queryKey: ['controllers'] });
    },
    onError: (err: Error) => setMountError(err.message),
  });

  const isPendingFor = (kind: EquipmentKind, id: number) =>
    mount.isPending && mount.variables?.kind === kind && mount.variables?.id === id;

  const mountableAccessories = (accessories.data ?? []).filter(
    (a) => a.status !== 'retired' && a.aircraft_id === null,
  );
  const mountableBatteries = (batteries.data ?? []).filter(
    (b) => b.status !== 'retired' && b.aircraft_id === null,
  );
  const mountableControllers = (controllers.data ?? []).filter(
    (c) => c.status !== 'retired' && c.aircraft_id === null,
  );

  const isLoading = accessories.isLoading || batteries.isLoading || controllers.isLoading;
  const hasError = Boolean(accessories.error || batteries.error || controllers.error);
  const allEmpty =
    !isLoading &&
    !hasError &&
    mountableAccessories.length === 0 &&
    mountableBatteries.length === 0 &&
    mountableControllers.length === 0;

  return (
    <div className={styles.mountedConfirmOverlay} onClick={onClose}>
      <div
        className={`${styles.mountedConfirmModal} ${styles.mountedMountModal}`}
        onClick={(e) => e.stopPropagation()}
      >
        <h2 className={styles.mountedConfirmTitle}>Mount equipment</h2>

        {isLoading && <div className={styles.mountedMuted}>Loading equipment…</div>}
        {accessories.error && (
          <div className={styles.mountedError}>
            Could not load accessories: {(accessories.error as Error).message}
          </div>
        )}
        {batteries.error && (
          <div className={styles.mountedError}>
            Could not load batteries: {(batteries.error as Error).message}
          </div>
        )}
        {controllers.error && (
          <div className={styles.mountedError}>
            Could not load controllers: {(controllers.error as Error).message}
          </div>
        )}

        {allEmpty && <div className={styles.mountedMuted}>No unmounted equipment available.</div>}

        {mountableAccessories.length > 0 && (
          <div className={styles.mountedGroup}>
            <div className={styles.mountedGroupHeading}>Accessories</div>
            <div className={styles.mountedRowList}>
              {mountableAccessories.map((a) => {
                const meta: string[] = [];
                if (a.serial_number) meta.push(a.serial_number);
                if (a.weight_kg != null) meta.push(`${a.weight_kg} kg`);
                return (
                  <MountableRow
                    key={a.id}
                    primary={a.equipment_id ?? a.name}
                    meta={meta}
                    pending={isPendingFor('accessory', a.id)}
                    onMount={() => mount.mutate({ kind: 'accessory', id: a.id })}
                  />
                );
              })}
            </div>
          </div>
        )}

        {mountableBatteries.length > 0 && (
          <div className={styles.mountedGroup}>
            <div className={styles.mountedGroupHeading}>Batteries</div>
            <div className={styles.mountedRowList}>
              {mountableBatteries.map((b) => {
                const meta: string[] = [];
                if (b.equipment_id) meta.push(b.serial_number);
                if (b.weight_kg != null) meta.push(`${b.weight_kg} kg`);
                return (
                  <MountableRow
                    key={b.id}
                    primary={b.equipment_id ?? b.serial_number}
                    meta={meta}
                    pending={isPendingFor('battery', b.id)}
                    onMount={() => mount.mutate({ kind: 'battery', id: b.id })}
                  />
                );
              })}
            </div>
          </div>
        )}

        {mountableControllers.length > 0 && (
          <div className={styles.mountedGroup}>
            <div className={styles.mountedGroupHeading}>Controllers</div>
            <div className={styles.mountedRowList}>
              {mountableControllers.map((c) => {
                const meta: string[] = [];
                if (c.equipment_id) meta.push(c.serial_number);
                return (
                  <MountableRow
                    key={c.id}
                    primary={c.equipment_id ?? c.serial_number}
                    meta={meta}
                    pending={isPendingFor('controller', c.id)}
                    onMount={() => mount.mutate({ kind: 'controller', id: c.id })}
                  />
                );
              })}
            </div>
          </div>
        )}

        {mountError && <div className={styles.mountedError}>Mount failed: {mountError}</div>}

        <div className={styles.mountedConfirmActions}>
          <button type="button" className={styles.mountedBtnCancel} onClick={onClose}>
            Done
          </button>
        </div>
      </div>
    </div>
  );
}

function MountableRow({
  primary,
  meta,
  pending,
  onMount,
}: {
  primary: string;
  meta: string[];
  pending: boolean;
  onMount: () => void;
}) {
  return (
    <div className={styles.mountedRow}>
      <div className={styles.mountedRowMain}>
        <span className={styles.mountedRowTitle}>{primary}</span>
        {meta.length > 0 && <span className={styles.mountedRowMeta}>{meta.join(' · ')}</span>}
      </div>
      <button
        type="button"
        className={styles.mountedUnmountBtn}
        onClick={onMount}
        disabled={pending}
      >
        {pending ? 'Mounting…' : 'Mount'}
      </button>
    </div>
  );
}

function UnmountConfirmModal({
  target,
  isPending,
  error,
  onConfirm,
  onCancel,
}: {
  target: UnmountTarget;
  isPending: boolean;
  error: string | null;
  onConfirm: () => void;
  onCancel: () => void;
}) {
  return (
    <div className={styles.mountedConfirmOverlay} onClick={onCancel}>
      <div className={styles.mountedConfirmModal} onClick={(e) => e.stopPropagation()}>
        <h2 className={styles.mountedConfirmTitle}>Unmount equipment?</h2>
        <p className={styles.mountedConfirmBody}>
          Unmount <strong>{target.label}</strong> from this aircraft? The item stays in the
          fleet inventory and can be mounted again at any time.
        </p>
        {error && <div className={styles.mountedError}>Unmount failed: {error}</div>}
        <div className={styles.mountedConfirmActions}>
          <button type="button" className={styles.mountedBtnCancel} onClick={onCancel}>
            Cancel
          </button>
          <button
            type="button"
            className={styles.mountedBtnConfirm}
            onClick={onConfirm}
            disabled={isPending}
          >
            {isPending ? 'Unmounting…' : 'Unmount'}
          </button>
        </div>
      </div>
    </div>
  );
}

function EquipmentRow({
  primary,
  primaryLinkTo,
  meta,
  status,
  canEdit,
  onUnmount,
}: {
  primary: string;
  primaryLinkTo?: string;
  meta: string[];
  status: string;
  canEdit: boolean;
  onUnmount: () => void;
}) {
  const pillClass =
    status === 'active' ? styles.mountedStatusActive : styles.mountedStatusMaintenance;
  return (
    <div className={styles.mountedRow}>
      <div className={styles.mountedRowMain}>
        <span className={styles.mountedRowTitle}>
          {primaryLinkTo ? (
            <Link to={primaryLinkTo} className={styles.mountedRowLink}>
              {primary}
            </Link>
          ) : (
            primary
          )}
        </span>
        {meta.length > 0 && <span className={styles.mountedRowMeta}>{meta.join(' · ')}</span>}
      </div>
      <span className={`${styles.statusPill} ${pillClass}`}>
        {STATUS_LABEL[status] ?? status}
      </span>
      {canEdit && (
        <button type="button" className={styles.mountedUnmountBtn} onClick={onUnmount}>
          Unmount
        </button>
      )}
    </div>
  );
}
