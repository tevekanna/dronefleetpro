// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { usePermanentDeleteScheduleTemplate } from '../../hooks/useMaintenanceScheduleTemplates';
import { showToast } from '../../store/toastStore';
import type { MaintenanceScheduleTemplate } from '../../types/maintenanceScheduleTemplate';
import styles from '../maintenance/MaintenanceModal.module.css';

interface Props {
  isOpen: boolean;
  onClose: () => void;
  template: MaintenanceScheduleTemplate;
}

export function PermanentDeleteTemplateConfirm({ isOpen, onClose, template }: Props) {
  const del = usePermanentDeleteScheduleTemplate();
  if (!isOpen) return null;

  const onConfirm = () => {
    del.mutate(template.id, {
      onSuccess: () => {
        showToast(`Template permanently deleted · ${template.name}`, 'success');
        onClose();
      },
      onError: () => {
        // Backend message is shown inline via del.isError below; don't close.
      },
    });
  };

  return (
    <div className={styles.overlay} onClick={onClose}>
      <div
        className={`${styles.modal} ${styles.modalNarrow}`}
        onClick={(e) => e.stopPropagation()}
      >
        <h2 className={styles.title}>Permanently delete template</h2>

        {del.isError && (
          <div className={styles.errorBanner}>
            {(del.error as Error).message ?? 'Failed to delete'}
          </div>
        )}

        <p className={styles.confirmBody}>
          This will permanently delete the template <strong>«{template.name}»</strong>.
          This action cannot be undone. No schedules currently use this template.
        </p>

        <div className={styles.actions}>
          <button
            type="button"
            className={styles.btnCancel}
            onClick={onClose}
            disabled={del.isPending}
          >
            Cancel
          </button>
          <button
            type="button"
            className={styles.btnDanger}
            onClick={onConfirm}
            disabled={del.isPending}
          >
            {del.isPending ? 'Deleting…' : 'Delete permanently'}
          </button>
        </div>
      </div>
    </div>
  );
}
