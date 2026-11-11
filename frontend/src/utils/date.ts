// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

/**
 * Format any date input as ISO 8601 calendar date (YYYY-MM-DD).
 *
 * Returns "" for null, undefined, or any value that does not parse as a
 * valid date. This is the project-wide convention for date display
 * (all dates are stored as TIMESTAMPTZ in PostgreSQL and displayed as
 * ISO 8601).
 */
export function formatISODate(input: string | Date | null | undefined): string {
  if (input === null || input === undefined || input === '') return '';
  const d = input instanceof Date ? input : new Date(input);
  if (Number.isNaN(d.getTime())) return '';
  return d.toISOString().slice(0, 10);
}

/** Today's date as ISO 8601 (YYYY-MM-DD) in the local timezone. */
export function todayISO(): string {
  const d = new Date();
  const y = d.getFullYear();
  const m = String(d.getMonth() + 1).padStart(2, '0');
  const day = String(d.getDate()).padStart(2, '0');
  return `${y}-${m}-${day}`;
}
