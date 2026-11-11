// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { apiClient } from './client';

export interface AircraftModel {
  id: number;
  org_id: number;
  name: string;
  manufacturer: string | null;
  category: string | null;
  is_active: boolean;
  notes: string | null;
  default_max_flight_time_minutes: number | null;
  /** Legacy free-text weight (migration 012); superseded for regulatory
   *  purposes by the fields below but kept for export/import stability. */
  default_weight_kg: number | null;
  /** Regulatory weight + EASA class fields (migration 049). null = not recorded. */
  mtom_kg: number | null;
  empty_weight_kg: number | null;
  max_payload_kg: number | null;
  /** EASA class marking C0..C6, or null when the airframe carries none. */
  class_marking: string | null;
  max_dimension_cm: number | null;
  /** Whether the design requires a certificate or declaration under
   *  Art. 56(1)/(5) of Reg (EU) 2018/1139 (migration 054). null means NOT
   *  DETERMINED and must never be treated as 'not_required' — it gates
   *  mandatory Annex VI occurrence reporting. */
  design_approval_required: 'required' | 'not_required' | null;
  created_at: string;
  updated_at: string;
}

export interface AircraftModelWithCount extends AircraftModel {
  aircraft_count: number;
}

/// Strict superset of AircraftModelWithCount returned by GET /aircraft-models
/// today. The extra fields are aggregated over the denormalized counters on
/// the aircraft table (no flights JOIN). Consumers typed against
/// AircraftModelWithCount keep working via TS structural subtyping.
export interface AircraftModelWithStats extends AircraftModel {
  aircraft_count: number;
  active_count: number;
  maintenance_count: number;
  retired_count: number;
  total_flights: number;
  total_flight_seconds: number;
  last_flight: string | null;
}

export interface AircraftModelIndividual {
  id: number;
  serial_number: string;
  equipment_id: string | null;
  nickname: string | null;
  status: string;
  total_flights: number;
  total_seconds: number;
  last_flight: string | null;
  needs_manual_sn: boolean;
}

/// Response shape for GET /aircraft-models/:id/stats — model fields +
/// aggregates flattened, plus the embedded individuals list.
export interface AircraftModelStatsDetail extends AircraftModelWithStats {
  individuals: AircraftModelIndividual[];
}

export interface CreateAircraftModelRequest {
  name: string;
  manufacturer?: string | null;
  category?: string | null;
  notes?: string | null;
  default_max_flight_time_minutes?: number | null;
  default_weight_kg?: number | null;
  mtom_kg?: number | null;
  empty_weight_kg?: number | null;
  max_payload_kg?: number | null;
  class_marking?: string | null;
  max_dimension_cm?: number | null;
  /** null (or omitted) = not determined. */
  design_approval_required?: 'required' | 'not_required' | null;
}

export interface UpdateAircraftModelRequest {
  name?: string;
  manufacturer?: string | null;
  category?: string | null;
  is_active?: boolean;
  notes?: string | null;
  default_max_flight_time_minutes?: number | null;
  default_weight_kg?: number | null;
  mtom_kg?: number | null;
  empty_weight_kg?: number | null;
  max_payload_kg?: number | null;
  class_marking?: string | null;
  max_dimension_cm?: number | null;
  /** Tri-state on the backend: omit = keep, explicit null = back to NOT
   *  DETERMINED, value = set. */
  design_approval_required?: 'required' | 'not_required' | null;
}

export interface MergeResult {
  message: string;
  source_id: number;
  target_id: number;
  source_name: string;
  target_name: string;
  aircraft_moved: number;
  mappings_moved: number;
  mappings_deleted_as_duplicates: number;
}

export function fetchAircraftModels(isActive?: boolean): Promise<AircraftModelWithStats[]> {
  const q = isActive === undefined ? '' : `?is_active=${isActive}`;
  return apiClient.get<AircraftModelWithStats[]>(`/aircraft-models${q}`);
}

export function fetchAircraftModelStats(id: number): Promise<AircraftModelStatsDetail> {
  return apiClient.get<AircraftModelStatsDetail>(`/aircraft-models/${id}/stats`);
}

export function createAircraftModel(body: CreateAircraftModelRequest): Promise<AircraftModel> {
  return apiClient.post<AircraftModel>('/aircraft-models', body);
}

export function updateAircraftModel(id: number, body: UpdateAircraftModelRequest): Promise<AircraftModel> {
  return apiClient.put<AircraftModel>(`/aircraft-models/${id}`, body);
}

export function retireAircraftModel(id: number): Promise<{ message: string; id: number }> {
  return apiClient.delete<{ message: string; id: number }>(`/aircraft-models/${id}`);
}

export function mergeAircraftModel(sourceId: number, targetId: number): Promise<MergeResult> {
  return apiClient.post<MergeResult>(`/aircraft-models/${sourceId}/merge`, { target_id: targetId });
}

export function permanentDeleteAircraftModel(id: number): Promise<AircraftModel> {
  return apiClient.post<AircraftModel>(`/aircraft-models/${id}/permanent-delete`, undefined);
}
