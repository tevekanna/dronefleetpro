// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useSyncScheduleTemplate } from '../../hooks/useMaintenanceScheduleTemplates';
import { showToast } from '../../store/toastStore';
import type { MaintenanceScheduleTemplate } from '../../types/maintenanceScheduleTemplate';
import styles from '../maintenance/MaintenanceModal.module.css';

interface Props {
  isOpen: boolean;
  onClose: () => void;
  template: MaintenanceScheduleTemplate;
  modelName: string;
}

export function SyncTemplateModal({ isOpen, onClose, template, modelName }: Props) {
  const sync = useSyncScheduleTemplate();
  if (!isOpen) return null;

  const onConfirm = () => {
    sync.mutate(
      { id: template.id, request: {} },
      {
        onSuccess: (data) => {
          if (data.synced > 0) {
            showToast(
              `Synced ${data.synced} schedule${data.synced === 1 ? '' : 's'} from "${template.name}"`,
              'success',
            );
          } else if (data.skipped_no_schedule > 0) {
            showToast(
              `No matching schedules to sync — ${data.skipped_no_schedule} aircraft have no "${template.name}" schedule yet`,
              'info',
            );
          } else {
            showToast('No aircraft found with this model', 'info');
          }
          onClose();
        },
        onError: (e) => {
          showToast(`Sync failed: ${(e as Error).message}`, 'error');
        },
      },
    );
  };

  return (
    <div className={styles.overlay} onClick={onClose}>
      <div
        className={`${styles.modal} ${styles.modalNarrow}`}
        onClick={(e) => e.stopPropagation()}
      >
        <h2 className={styles.title}>Sync template to existing schedules?</h2>

        {sync.isError && (
          <div className={styles.errorBanner}>
            {(sync.error as Error).message ?? 'Failed to sync'}
          </div>
        )}

        <p className={styles.confirmBody}>
          This will overwrite the interval, warning threshold, secondary trigger, and
          critical flag on every <strong>{modelName}</strong> aircraft's existing{' '}
          <strong>{template.name}</strong> schedule. Aircraft without a matching
          schedule are not touched.
        </p>
        <p className={styles.confirmBody}>
          <span className={styles.hint}>
            Last-performed dates and counters are preserved; next-due is recomputed
            from the new interval.
          </span>
        </p>

        <div className={styles.actions}>
          <button type="button" className={styles.btnCancel} onClick={onClose}>
            Cancel
          </button>
          <button
            type="button"
            className={styles.btnPrimary}
            onClick={onConfirm}
            disabled={sync.isPending}
          >
            {sync.isPending ? 'Syncing…' : 'Sync now'}
          </button>
        </div>
      </div>
    </div>
  );
}
