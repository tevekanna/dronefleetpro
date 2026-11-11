// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import type { ReactNode } from 'react';
import styles from './fleetStatsDashboard.module.css';

export interface EmptyStateProps {
  icon?: ReactNode;
  message: string;
  ok?: boolean;
}

export function EmptyState({ icon, message, ok }: EmptyStateProps) {
  return (
    <div className={`${styles.emptyState} ${ok ? styles.emptyStateOk : ''}`}>
      {icon}
      <span>{message}</span>
    </div>
  );
}
