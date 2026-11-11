// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { apiClient } from './client';
import type { Aircraft, CreateAircraftRequest, UpdateAircraftRequest } from '../types/aircraft';

export async function fetchAircraft(includeRetired = false): Promise<Aircraft[]> {
  const q = includeRetired ? '?include_retired=true' : '';
  return apiClient.get(`/aircraft${q}`);
}

export async function fetchAircraftById(id: number): Promise<Aircraft> {
  return apiClient.get(`/aircraft/${id}`);
}

export async function createAircraft(data: CreateAircraftRequest): Promise<Aircraft> {
  return apiClient.post('/aircraft', data);
}

export async function updateAircraft(id: number, data: UpdateAircraftRequest): Promise<Aircraft> {
  return apiClient.put(`/aircraft/${id}`, data);
}

export async function retireAircraft(id: number): Promise<void> {
  return apiClient.delete(`/aircraft/${id}`);
}

export interface AircraftMergeResult {
  deleted_aircraft_id: number;
  merged_into_aircraft_id: number;
  flights_moved: number;
  batteries_moved: number;
  controllers_moved: number;
  defects_moved: number;
  incidents_moved: number;
  maintenance_moved: number;
  schedules_deleted_as_duplicates: number;
  identifiers_migrated: number;
}

export function mergeAircraft(sourceId: number, targetId: number): Promise<AircraftMergeResult> {
  return apiClient.post<AircraftMergeResult>(`/aircraft/${sourceId}/merge`, { target_id: targetId });
}
