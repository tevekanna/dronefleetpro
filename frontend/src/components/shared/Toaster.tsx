// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { useToastStore } from '../../store/toastStore';
import type { ToastKind } from '../../store/toastStore';
import styles from './Toaster.module.css';

const KIND_CLASS: Record<ToastKind, string> = {
  success: styles.toastSuccess,
  error: styles.toastError,
  info: styles.toastInfo,
};

export function Toaster() {
  const toasts = useToastStore((s) => s.toasts);
  const dismiss = useToastStore((s) => s.dismiss);

  if (toasts.length === 0) return null;
  return (
    <div className={styles.container} role="status" aria-live="polite">
      {toasts.map((t) => (
        <div key={t.id} className={`${styles.toast} ${KIND_CLASS[t.kind]}`}>
          <span>{t.message}</span>
          <button
            type="button"
            className={styles.closeButton}
            onClick={() => dismiss(t.id)}
            aria-label="Dismiss"
          >
            ×
          </button>
        </div>
      ))}
    </div>
  );
}
