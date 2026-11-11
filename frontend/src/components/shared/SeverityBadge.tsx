// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import styles from './SeverityBadge.module.css';

/// Severity pill for the shared {low, medium, high, critical} scale (incidents
/// and log findings). Extracted from the inline treatment that previously
/// lived in IncidentsPage so the colour mapping has a single source of truth.
/// Unknown values fall back to the `low` treatment.
export function SeverityBadge({ severity }: { severity: string }) {
  const variant = styles[severity] ?? styles.low;
  return <span className={`${styles.badge} ${variant}`}>{severity}</span>;
}
