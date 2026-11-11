// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import styles from './AuditActionBadge.module.css';

interface Props {
  action: string;
}

const KIND_CLASS: Record<string, string> = {
  INSERT: styles.insert,
  UPDATE: styles.update,
  DELETE: styles.delete,
  EXPORT: styles.export,
  IMPORT: styles.import,
  PURGE: styles.purge,
  LOGIN_SUCCESS: styles.loginSuccess,
  LOGIN_FAILURE: styles.loginFailure,
  LOGOUT: styles.logout,
  TOKEN_REDEEMED: styles.tokenRedeemed,
};

export function AuditActionBadge({ action }: Props) {
  const cls = KIND_CLASS[action] ?? styles.unknown;
  return (
    <span className={`${styles.badge} ${cls}`}>
      <span className={styles.dot} aria-hidden="true" />
      {action}
    </span>
  );
}
