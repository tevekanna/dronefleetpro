// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useDeleteScheduleTemplate } from '../../hooks/useMaintenanceScheduleTemplates';
import { showToast } from '../../store/toastStore';
import type { MaintenanceScheduleTemplate } from '../../types/maintenanceScheduleTemplate';
import styles from '../maintenance/MaintenanceModal.module.css';

interface Props {
  isOpen: boolean;
  onClose: () => void;
  template: MaintenanceScheduleTemplate;
}

export function DeleteTemplateConfirm({ isOpen, onClose, template }: Props) {
  const del = useDeleteScheduleTemplate();
  if (!isOpen) return null;

  const onConfirm = () => {
    del.mutate(template.id, {
      onSuccess: () => {
        showToast(`Template deactivated · ${template.name}`, 'success');
        onClose();
      },
      onError: (e) => {
        showToast(`Delete failed: ${(e as Error).message}`, 'error');
      },
    });
  };

  return (
    <div className={styles.overlay} onClick={onClose}>
      <div
        className={`${styles.modal} ${styles.modalNarrow}`}
        onClick={(e) => e.stopPropagation()}
      >
        <h2 className={styles.title}>Deactivate template?</h2>

        {del.isError && (
          <div className={styles.errorBanner}>
            {(del.error as Error).message ?? 'Failed to deactivate'}
          </div>
        )}

        <p className={styles.confirmBody}>
          Deactivate <strong>{template.name}</strong>? It will stop spawning new
          per-aircraft schedules and disappear from the active list. Existing
          schedules already created from this template are not affected.
        </p>

        <div className={styles.actions}>
          <button type="button" className={styles.btnCancel} onClick={onClose}>
            Cancel
          </button>
          <button
            type="button"
            className={styles.btnDanger}
            onClick={onConfirm}
            disabled={del.isPending}
          >
            {del.isPending ? 'Deactivating…' : 'Deactivate'}
          </button>
        </div>
      </div>
    </div>
  );
}
