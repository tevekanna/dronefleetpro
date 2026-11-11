// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { apiClient } from './client';

export interface DraftFormData {
  performed_date: string;
  maint_type: string;
  remark: string;
  action_taken: string;
  sign_off: string;
}

export interface MaintenanceDraft {
  id: number;
  org_id: number;
  schedule_id: number;
  saved_by: string;
  form_data: DraftFormData;
  checklist_status: Record<string, 'ok' | 'nok' | 'na' | 'pending'>;
  checklist_notes: Record<string, string>;
  created_at: string;
  updated_at: string;
}

export interface UpsertDraftPayload {
  saved_by: string;
  form_data: DraftFormData;
  checklist_status: Record<string, 'ok' | 'nok' | 'na' | 'pending'>;
  checklist_notes: Record<string, string>;
}

export function fetchDraft(scheduleId: number): Promise<MaintenanceDraft> {
  return apiClient.get<MaintenanceDraft>(`/maintenance/drafts/${scheduleId}`);
}

export function upsertDraft(
  scheduleId: number,
  payload: UpsertDraftPayload,
): Promise<MaintenanceDraft> {
  return apiClient.put<MaintenanceDraft>(`/maintenance/drafts/${scheduleId}`, payload);
}

export function deleteDraft(scheduleId: number): Promise<unknown> {
  return apiClient.delete(`/maintenance/drafts/${scheduleId}`);
}
