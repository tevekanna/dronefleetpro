// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

export type IntervalType = 'flight_hours' | 'calendar_days' | 'cycles';
export type SecondaryIntervalType = 'flight_hours' | 'calendar_days';
export type ScheduleStatus = 'ok' | 'warning' | 'overdue';
export type MaintenanceType = 'Repair' | 'Modification' | 'Overhaul' | 'Test';

export interface MaintenanceSchedule {
  id: number;
  org_id: number;
  aircraft_id: number;
  name: string;
  description: string | null;
  interval_type: IntervalType;
  interval_value: number;
  warning_threshold: number;
  is_critical: boolean;
  last_performed_at: string | null;
  last_performed_flight_hours: number;
  last_performed_cycles: number;
  next_due_at: string | null;
  next_due_flight_hours: number | null;
  next_due_cycles: number | null;
  secondary_interval_type: SecondaryIntervalType | null;
  secondary_interval_value: number | null;
  secondary_next_due_at: string | null;
  secondary_next_due_hours: number | null;
  is_active: boolean;
  created_by: string | null;
  created_at: string;
  instruction_template_id: number | null;
}

export interface ScheduleWithStatus extends MaintenanceSchedule {
  status: ScheduleStatus;
  consumption_pct: number;
  secondary_consumption_pct: number | null;
  aircraft_equipment_id: string | null;
  aircraft_model: string | null;
  aircraft_serial_number: string;
  aircraft_model_id: number | null;
}

export interface ScheduleDashboard {
  overdue: number;
  warning: number;
  ok: number;
  items: ScheduleWithStatus[];
}

export interface ScheduleHistoryItem {
  maintenance_id: number;
  performed_date: string;
  action_taken: string;
  remark: string;
  maint_type: string;
  sign_off: string;
  created_by: string;
}

export interface ScheduleDetail extends ScheduleWithStatus {
  history: ScheduleHistoryItem[];
}

export interface CompleteSchedulePayload {
  performed_date: string;
  action_taken: string;
  remark: string;
  maint_type: MaintenanceType;
  sign_off: string;
}

export interface CompleteScheduleResponse {
  maintenance_id: number;
  schedule: MaintenanceSchedule;
}

export interface CreateSchedulePayload {
  aircraft_id: number;
  name: string;
  description: string | null;
  interval_type: IntervalType;
  interval_value: number;
  warning_threshold: number;
  is_critical: boolean;
  secondary_interval_type: SecondaryIntervalType | null;
  secondary_interval_value: number | null;
  instruction_template_id: number | null;
}

export type UpdateSchedulePayload = Partial<CreateSchedulePayload> & { is_active?: boolean };

export interface ListSchedulesParams {
  aircraft_id?: number;
  status?: ScheduleStatus;
  include_inactive?: boolean;
}
