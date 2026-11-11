// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

export interface AircraftLabelFields {
  equipment_id: string | null;
  model: string | null;
  serial_number: string;
  nickname?: string | null;
}

/** Single-line label with em-dash, e.g. "UAV-060 — Inspire 1 X5".
 *  For <select> options, breadcrumbs, modal context strips, page H1s. */
export function formatAircraftLabel(a: AircraftLabelFields): string {
  const left = a.equipment_id ?? a.serial_number;
  const right = a.model ?? a.nickname ?? null;
  if (!right || right === left) return left;
  return `${left} — ${right}`;
}

/** Two-line cell tuple. Primary line and optional secondary line
 *  (model · SN). For table cells and detail headers. */
export function formatAircraftCell(a: AircraftLabelFields): { primary: string; secondary: string | null } {
  const primary = a.equipment_id ?? a.serial_number;
  const parts: string[] = [];
  if (a.model) parts.push(a.model);
  if (a.equipment_id) parts.push(a.serial_number);
  return { primary, secondary: parts.length ? parts.join(' · ') : null };
}
