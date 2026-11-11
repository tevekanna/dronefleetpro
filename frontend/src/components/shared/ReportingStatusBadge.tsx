// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import styles from './ReportingStatusBadge.module.css';
import type { ReportingState } from '../../api/incidents';

/// Occurrence-reporting status pill for the seven states computed by the
/// backend (services/occurrence_reporting.rs). Follows the SeverityBadge
/// pattern so the colour mapping has a single source of truth. Raw state
/// strings are never rendered — every state maps to a human-readable label;
/// unknown values fall back to the muted `undetermined` treatment.
const STATE_LABELS: Record<ReportingState, string> = {
  overdue: 'Overdue',
  due: 'Report due',
  reported: 'Reported',
  awaiting_awareness: 'Awaiting awareness',
  unclassified: 'Not classified',
  undetermined: 'Not determined',
  not_applicable: 'Not applicable',
};

export function ReportingStatusBadge({ state }: { state: ReportingState }) {
  const variant = styles[state] ?? styles.undetermined;
  const label = STATE_LABELS[state] ?? 'Not determined';
  return <span className={`${styles.badge} ${variant}`}>{label}</span>;
}
