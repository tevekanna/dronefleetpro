// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { apiClient } from './client';
import type {
  CreateTemplateInput,
  ListTemplatesParams,
  MaintenanceScheduleTemplate,
  SeedDefaultsRequest,
  SeedDefaultsResponse,
  SyncTemplateRequest,
  SyncTemplateResponse,
  UpdateTemplateInput,
} from '../types/maintenanceScheduleTemplate';

const BASE = '/maintenance-schedule-templates';

function toQuery(params?: ListTemplatesParams): string {
  if (!params) return '';
  const parts: string[] = [];
  if (params.model_id !== undefined) parts.push(`model_id=${params.model_id}`);
  if (params.include_inactive) parts.push('include_inactive=true');
  return parts.length ? `?${parts.join('&')}` : '';
}

export function fetchTemplates(
  params?: ListTemplatesParams,
): Promise<MaintenanceScheduleTemplate[]> {
  return apiClient.get<MaintenanceScheduleTemplate[]>(`${BASE}${toQuery(params)}`);
}

export function getTemplate(id: number): Promise<MaintenanceScheduleTemplate> {
  return apiClient.get<MaintenanceScheduleTemplate>(`${BASE}/${id}`);
}

export function createTemplate(
  input: CreateTemplateInput,
): Promise<MaintenanceScheduleTemplate> {
  return apiClient.post<MaintenanceScheduleTemplate>(BASE, input);
}

export function updateTemplate(
  id: number,
  input: UpdateTemplateInput,
): Promise<MaintenanceScheduleTemplate> {
  return apiClient.put<MaintenanceScheduleTemplate>(`${BASE}/${id}`, input);
}

export function deleteTemplate(id: number): Promise<{ deleted: boolean; id: number }> {
  return apiClient.delete<{ deleted: boolean; id: number }>(`${BASE}/${id}`);
}

export function permanentDeleteTemplate(
  id: number,
): Promise<MaintenanceScheduleTemplate> {
  return apiClient.post<MaintenanceScheduleTemplate>(
    `${BASE}/${id}/permanent-delete`,
    undefined,
  );
}

export function syncTemplate(
  id: number,
  request: SyncTemplateRequest,
): Promise<SyncTemplateResponse> {
  return apiClient.post<SyncTemplateResponse>(`${BASE}/${id}/sync`, request);
}

/**
 * Seed default schedules for one or all aircraft. Lives on a separate
 * endpoint from the templates CRUD but is conceptually part of the same
 * admin workflow, so we group it here.
 */
export function seedDefaultSchedules(
  request: SeedDefaultsRequest,
): Promise<SeedDefaultsResponse> {
  return apiClient.post<SeedDefaultsResponse>(
    '/maintenance-schedules/seed-defaults',
    request,
  );
}
