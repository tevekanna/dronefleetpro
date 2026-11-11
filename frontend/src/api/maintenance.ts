// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { apiClient } from './client';

export interface MaintenanceRecord {
  id: number;
  org_id: number;
  aircraft_id: number | null;
  controller_id: number | null;
  performed_date: string;
  flight_id: number | null;
  schedule_id: number | null;
  created_by: string;
  remark: string | null;
  action_taken: string | null;
  maint_category: string | null;
  maint_type: string | null;
  sign_off: string;
  created_at: string;
  aircraft_name: string | null;
  controller_name: string | null;
}

export interface MaintenanceAuditEntry {
  id: number;
  action: string;
  user_id: number | null;
  user_code: string | null;
  user_name: string | null;
  ip_address: string | null;
  created_at: string;
  changed_fields: string[];
  diff_summary: string | null;
}

export interface MaintenanceDetail {
  record: MaintenanceRecord;
  audit: MaintenanceAuditEntry[];
}

export interface DefectRecord {
  id: number;
  org_id: number;
  aircraft_id: number | null;
  flight_id: number | null;
  defect_date: string;
  reported_by: string;
  description: string;
  status: string;
  action_taken: string | null;
  closed_by: string | null;
  closed_date: string | null;
  created_at: string;
}

export interface MaintenanceFilters {
  aircraft_id?: number;
  controller_id?: number;
}

export interface CompletionInput {
  checklist_item_id: number;
  status: 'ok' | 'nok' | 'na' | 'pending';
  note?: string;
  completed_by: string;
}

export interface SaveCompletionsRequest {
  template_id: number;
  completions: CompletionInput[];
}

export interface SaveCompletionsResponse {
  maintenance_id: number;
  template_id: number;
  saved: number;
  defects_created: number;
}

export async function fetchMaintenance(
  filters: MaintenanceFilters = {},
): Promise<MaintenanceRecord[]> {
  const params = new URLSearchParams();
  if (filters.aircraft_id !== undefined) {
    params.set('aircraft_id', String(filters.aircraft_id));
  }
  if (filters.controller_id !== undefined) {
    params.set('controller_id', String(filters.controller_id));
  }
  const qs = params.toString();
  return apiClient.get(qs ? `/maintenance?${qs}` : '/maintenance');
}

export async function fetchMaintenanceDetail(id: number): Promise<MaintenanceDetail> {
  return apiClient.get(`/maintenance/${id}/detail`);
}

export async function createMaintenance(data: object): Promise<MaintenanceRecord> {
  return apiClient.post('/maintenance', data);
}

export async function deleteMaintenance(id: number): Promise<void> {
  return apiClient.delete(`/maintenance/${id}`);
}

export interface DefectFilters {
  aircraft_id?: number;
}

export async function fetchDefects(
  filters: DefectFilters = {},
): Promise<DefectRecord[]> {
  const params = new URLSearchParams();
  if (filters.aircraft_id !== undefined) {
    params.set('aircraft_id', String(filters.aircraft_id));
  }
  const qs = params.toString();
  return apiClient.get(qs ? `/defects?${qs}` : '/defects');
}

export async function createDefect(data: object): Promise<DefectRecord> {
  return apiClient.post('/defects', data);
}

export async function closeDefect(id: number, closed_by: string, action_taken: string): Promise<DefectRecord> {
  return apiClient.put(`/defects/${id}`, { status: 'closed', closed_by, action_taken });
}

export async function deleteDefect(id: number): Promise<void> {
  return apiClient.delete(`/defects/${id}`);
}

export async function saveMaintenanceCompletions(
  maintenanceId: number,
  body: SaveCompletionsRequest,
): Promise<SaveCompletionsResponse> {
  return apiClient.post(`/maintenance/${maintenanceId}/completions`, body);
}
