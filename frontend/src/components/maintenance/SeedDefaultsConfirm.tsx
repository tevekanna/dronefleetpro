// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useSeedDefaultSchedules } from '../../hooks/useMaintenanceScheduleTemplates';
import { showToast } from '../../store/toastStore';
import styles from './MaintenanceModal.module.css';

interface Props {
  isOpen: boolean;
  onClose: () => void;
}

export function SeedDefaultsConfirm({ isOpen, onClose }: Props) {
  const seed = useSeedDefaultSchedules();
  if (!isOpen) return null;

  const onConfirm = () => {
    seed.mutate(
      { seed_all: true },
      {
        onSuccess: (data) => {
          const nmsSkipped = data.aircraft_skipped_needs_manual_sn;
          const nmsSuffix =
            nmsSkipped > 0 ? ` · ${nmsSkipped} skipped (unconfirmed serial)` : '';
          if (data.aircraft_seeded > 0) {
            showToast(
              `Seeded ${data.aircraft_seeded} aircraft · ${data.total_schedules_created} schedules created${nmsSuffix}`,
              'success',
            );
          } else if (nmsSkipped > 0) {
            showToast(`No aircraft seeded · ${nmsSkipped} skipped (unconfirmed serial)`, 'info');
          } else {
            showToast('No aircraft to seed (everything already has schedules)', 'info');
          }
          onClose();
        },
        onError: (e) => {
          showToast(`Seed failed: ${(e as Error).message}`, 'error');
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
        <h2 className={styles.title}>Seed default schedules?</h2>

        {seed.isError && (
          <div className={styles.errorBanner}>
            {(seed.error as Error).message ?? 'Failed to seed defaults'}
          </div>
        )}

        <p className={styles.confirmBody}>
          This creates a maintenance schedule on every aircraft without any
          schedules, using the templates configured for that aircraft's model. It
          never overwrites existing schedules. Aircraft without a model, or with a
          model that has no templates, are skipped.
        </p>

        <div className={styles.actions}>
          <button type="button" className={styles.btnCancel} onClick={onClose}>
            Cancel
          </button>
          <button
            type="button"
            className={styles.btnPrimary}
            onClick={onConfirm}
            disabled={seed.isPending}
          >
            {seed.isPending ? 'Seeding…' : 'Seed defaults'}
          </button>
        </div>
      </div>
    </div>
  );
}
