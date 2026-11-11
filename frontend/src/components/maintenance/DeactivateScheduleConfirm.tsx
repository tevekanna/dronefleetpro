// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useDeactivateMaintenanceSchedule } from '../../hooks/useMaintenanceSchedules';
import { showToast } from '../../store/toastStore';
import type { ScheduleWithStatus } from '../../types/maintenanceSchedule';
import styles from './MaintenanceModal.module.css';

interface Props {
  isOpen: boolean;
  onClose: () => void;
  schedule: ScheduleWithStatus;
}

export function DeactivateScheduleConfirm({ isOpen, onClose, schedule }: Props) {
  const deactivate = useDeactivateMaintenanceSchedule();

  if (!isOpen) return null;

  const aircraftLabel =
    schedule.aircraft_equipment_id ?? schedule.aircraft_serial_number;

  const onConfirm = () => {
    deactivate.mutate(schedule.id, {
      onSuccess: () => {
        showToast(`Schedule deactivated · ${schedule.name}`, 'success');
        onClose();
      },
    });
  };

  return (
    <div className={styles.overlay} onClick={onClose}>
      <div
        className={`${styles.modal} ${styles.modalNarrow}`}
        onClick={(e) => e.stopPropagation()}
      >
        <h2 className={styles.title}>Deactivate schedule?</h2>

        {deactivate.isError && (
          <div className={styles.errorBanner}>
            {(deactivate.error as Error).message ?? 'Failed to deactivate'}
          </div>
        )}

        <p className={styles.confirmBody}>
          Deactivate <strong>{schedule.name}</strong> on{' '}
          <strong>{aircraftLabel}</strong>? The schedule will no longer appear in active
          lists, but completed maintenance records are retained per EASA.
        </p>

        <div className={styles.actions}>
          <button type="button" className={styles.btnCancel} onClick={onClose}>
            Cancel
          </button>
          <button
            type="button"
            className={styles.btnDanger}
            onClick={onConfirm}
            disabled={deactivate.isPending}
          >
            {deactivate.isPending ? 'Deactivating…' : 'Deactivate'}
          </button>
        </div>
      </div>
    </div>
  );
}
