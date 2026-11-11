// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { apiClient } from './client';
import type { HubOpenDefect, HubRecentActivity } from './maintenanceHub';
import type { ScheduleWithStatus } from '../types/maintenanceSchedule';

export interface FleetMaintenanceRow {
  aircraft_id:          number;
  equipment_id:         string | null;
  aircraft_name:        string;
  serial_number:        string;
  model:                string | null;
  airworthiness_status: 'airworthy' | 'not_airworthy';
  overdue_count:        number;
  due_soon_count:       number;
  open_defect_count:    number;
  next_due_label:       string | null;
  next_due_at:          string | null;
}

export interface FleetMaintenanceStatus {
  airworthy_count:     number;
  not_airworthy_count: number;
  overdue_count:       number;
  due_soon_count:      number;
  open_defect_count:   number;
  aircraft:            FleetMaintenanceRow[];
}

export interface DraftSummary {
  schedule_id: number;
  items_marked: number;
  items_total: number;
}

export interface AircraftMaintenanceDetail {
  aircraft_id:          number;
  equipment_id:         string | null;
  aircraft_name:        string;
  serial_number:        string;
  model:                string | null;
  model_id:             number | null;
  status:               string;
  airworthiness_status: 'airworthy' | 'not_airworthy';
  airworthiness_reason: string | null;
  total_flights:        number;
  total_hours:          number;
  last_flight:          string | null;
  schedules:            ScheduleWithStatus[];
  open_defects:         HubOpenDefect[];
  recent_maintenance:   HubRecentActivity[];
  drafts:               DraftSummary[];
}

export function fetchFleetMaintenanceStatus(): Promise<FleetMaintenanceStatus> {
  return apiClient.get<FleetMaintenanceStatus>('/maintenance/fleet-status');
}

export function fetchAircraftMaintenanceDetail(
  id: number,
): Promise<AircraftMaintenanceDetail> {
  return apiClient.get<AircraftMaintenanceDetail>(`/maintenance/aircraft/${id}`);
}

export function patchAirworthinessStatus(
  id: number,
  airworthiness_status: 'airworthy' | 'not_airworthy',
  comment: string,
): Promise<unknown> {
  return apiClient.patch(`/aircraft/${id}/airworthiness`, {
    airworthiness_status,
    comment,
  });
}

export interface CreateDefectPayload {
  aircraft_id: number;
  defect_date: string;
  reported_by: string;
  description: string;
  flight_id?: number | null;
}

export function createDefect(payload: CreateDefectPayload): Promise<unknown> {
  return apiClient.post('/defects', payload);
}
