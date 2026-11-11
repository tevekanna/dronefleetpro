// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

export type IntervalType = 'flight_hours' | 'calendar_days' | 'cycles';
export type SecondaryIntervalType = 'flight_hours' | 'calendar_days';

export interface MaintenanceScheduleTemplate {
  id: number;
  org_id: number;
  model_id: number;
  name: string;
  description: string | null;
  interval_type: IntervalType;
  interval_value: number;
  warning_threshold: number;
  is_critical: boolean;
  secondary_interval_type: SecondaryIntervalType | null;
  secondary_interval_value: number | null;
  is_active: boolean;
  created_at: string;
  created_by: string | null;
  /** Count of maintenance_schedules rows referencing this template
   *  (matched by name + model). Only present on list-endpoint rows. */
  reference_count?: number;
}

export interface CreateTemplateInput {
  model_id: number;
  name: string;
  description?: string | null;
  interval_type: IntervalType;
  interval_value: number;
  warning_threshold?: number;
  is_critical?: boolean;
  secondary_interval_type?: SecondaryIntervalType | null;
  secondary_interval_value?: number | null;
}

export type UpdateTemplateInput = Partial<Omit<CreateTemplateInput, 'model_id'>> & {
  is_active?: boolean;
};

export interface ListTemplatesParams {
  model_id?: number;
  include_inactive?: boolean;
}

export interface SyncTemplateRequest {
  /** When omitted or empty, broadens to all aircraft of this model. */
  aircraft_ids?: number[];
}

export interface SyncTemplateResponse {
  synced: number;
  skipped_no_schedule: number;
  schedules_updated: number;
}

export interface SeedDefaultsRequest {
  aircraft_id?: number;
  seed_all?: boolean;
}

export interface SeedDefaultsResponse {
  aircraft_seeded: number;
  aircraft_skipped_had_schedules: number;
  /** Aircraft flagged `needs_manual_sn = TRUE` — skipped pending operator
   *  confirmation of the serial number via the pending-review flow. */
  aircraft_skipped_needs_manual_sn: number;
  aircraft_skipped_no_model: number;
  aircraft_skipped_no_templates: number;
  total_schedules_created: number;
}
