// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import type { User } from '../../types/user';
import styles from './DeactivateUserModal.module.css';

interface DeactivateUserModalProps {
  isOpen: boolean;
  user: User;
  onConfirm: () => void;
  onCancel: () => void;
  isSubmitting: boolean;
}

export function DeactivateUserModal({ isOpen, user, onConfirm, onCancel, isSubmitting }: DeactivateUserModalProps) {
  if (!isOpen) return null;

  return (
    <div className={styles.overlay} onClick={onCancel}>
      <div className={styles.modal} onClick={e => e.stopPropagation()}>
        <h2 className={styles.title}>Deactivate user?</h2>
        <p className={styles.body}>
          This will prevent <span className={styles.userName}>{user.name}</span> from
          logging in. Their data and history will be preserved. You can reactivate them
          by editing their profile.
        </p>
        <div className={styles.actions}>
          <button className={styles.btnCancel} onClick={onCancel}>Cancel</button>
          <button
            className={styles.btnDeactivate}
            onClick={onConfirm}
            disabled={isSubmitting}
          >
            {isSubmitting ? 'Deactivating...' : 'Deactivate'}
          </button>
        </div>
      </div>
    </div>
  );
}
