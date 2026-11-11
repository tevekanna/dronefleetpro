// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

export type EntityGroup =
  | 'equipment'
  | 'pilots'
  | 'flights'
  | 'maintenance'
  | 'incidents'
  | 'settings'
  | 'organisation'
  | 'audit_log';

export const ENTITY_GROUPS: { id: EntityGroup; label: string; description: string }[] = [
  { id: 'equipment',    label: 'Equipment',            description: 'Aircraft, batteries, controllers, models, and identifier aliases (FC-SN/UID matching)' },
  { id: 'pilots',       label: 'Pilots',               description: 'Pilot records and EASA qualifications' },
  { id: 'flights',      label: 'Flights',              description: 'Flight log entries (excludes soft-deleted)' },
  { id: 'maintenance',  label: 'Maintenance',          description: 'Records, defects, schedules, battery tests, schedule templates, inspection checklists, checklist completions, and drafts' },
  { id: 'incidents',    label: 'Incidents',            description: 'Incident reports and incident notes' },
  { id: 'settings',     label: 'Settings',             description: 'Org settings, drone type mappings, and checklists. Secrets are scrubbed.' },
  { id: 'organisation', label: 'Organisation branding', description: 'Sidebar logo (light/dark variants). Deselect when transferring data to another instance.' },
  { id: 'audit_log',    label: 'Audit Log',            description: 'Full audit history. Can be very large.' },
];

export interface ExportPreviewResponse {
  counts: Record<string, number>;
  estimated_size_bytes: number;
}

export interface DataTransferJob {
  id: number;
  job_type: 'export' | 'import';
  status: 'pending' | 'running' | 'completed' | 'failed';
  entity_groups: EntityGroup[] | string | null;
  user_code: string;
  summary: Record<string, unknown> | null;
  error_message: string | null;
  started_at: string;
  completed_at: string | null;
}

export type ConflictStrategy = 'skip' | 'update' | 'fail';

export interface ImportRequestPayload {
  document: unknown;
  strategy: ConflictStrategy;
  /** Entity groups to exclude from this import even if present in the file. */
  skip_groups: EntityGroup[];
}

export interface ConflictDetail {
  table: string;
  natural_key: Record<string, unknown> | unknown;
  action: string;
  reason: string | null;
}

export interface PerTableCounts {
  created?: number;
  updated?: number;
  skipped?: number;
  failed?: number;
}

export interface ImportSummary {
  per_table: Record<string, PerTableCounts>;
  total_created: number;
  total_updated: number;
  total_skipped: number;
  total_failed: number;
  conflicts: ConflictDetail[];
  conflicts_truncated: boolean;
  format_version: number;
  source_org_id: number | null;
  dry_run: boolean;
}
