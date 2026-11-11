// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import styles from './RouteFallback.module.css';

export function RouteFallback() {
  return (
    <div className={styles.wrap} role="status" aria-live="polite" aria-busy="true">
      <span className={styles.spinner} aria-hidden="true" />
      <span className={styles.label}>Loading…</span>
    </div>
  );
}
