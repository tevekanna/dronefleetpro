// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { apiClient } from './client';
import type {
  CompleteSchedulePayload,
  CompleteScheduleResponse,
  CreateSchedulePayload,
  ListSchedulesParams,
  MaintenanceSchedule,
  ScheduleDashboard,
  ScheduleDetail,
  ScheduleWithStatus,
  UpdateSchedulePayload,
} from '../types/maintenanceSchedule';

const BASE = '/maintenance-schedules';

function toQuery(params?: ListSchedulesParams): string {
  if (!params) return '';
  const parts: string[] = [];
  if (params.aircraft_id !== undefined) parts.push(`aircraft_id=${params.aircraft_id}`);
  if (params.status !== undefined) parts.push(`status=${params.status}`);
  if (params.include_inactive) parts.push('include_inactive=true');
  return parts.length ? `?${parts.join('&')}` : '';
}

export function listSchedules(params?: ListSchedulesParams): Promise<ScheduleWithStatus[]> {
  return apiClient.get<ScheduleWithStatus[]>(`${BASE}${toQuery(params)}`);
}

export function getSchedule(id: number): Promise<ScheduleDetail> {
  return apiClient.get<ScheduleDetail>(`${BASE}/${id}`);
}

export function getDashboard(): Promise<ScheduleDashboard> {
  return apiClient.get<ScheduleDashboard>(`${BASE}/dashboard`);
}

export function createSchedule(payload: CreateSchedulePayload): Promise<MaintenanceSchedule> {
  return apiClient.post<MaintenanceSchedule>(BASE, payload);
}

export function updateSchedule(
  id: number,
  payload: UpdateSchedulePayload,
): Promise<MaintenanceSchedule> {
  return apiClient.put<MaintenanceSchedule>(`${BASE}/${id}`, payload);
}

export function deactivateSchedule(id: number): Promise<void> {
  return apiClient.delete<void>(`${BASE}/${id}`);
}

export function completeSchedule(
  id: number,
  payload: CompleteSchedulePayload,
): Promise<CompleteScheduleResponse> {
  return apiClient.post<CompleteScheduleResponse>(`${BASE}/${id}/complete`, payload);
}
