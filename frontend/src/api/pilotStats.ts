// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { apiClient } from './client';

export type QualificationStatus = 'ok' | 'expiring_soon' | 'expired' | 'no_expiry';

export interface TopAircraft {
  aircraft_id: number;
  equipment_id: string | null;
  model: string | null;
  flight_seconds: number;
  flight_count: number;
}

export interface TopModel {
  model_id: number | null;
  model: string | null;
  flight_seconds: number;
  flight_count: number;
}

export interface CategoryBreakdown {
  category: string;
  flight_count: number;
  pct: number;
}

export interface MissionBreakdown {
  mission_type: string;
  flight_count: number;
  pct: number;
}

export interface QualificationEntry {
  qualification_name: string;
  expires_on: string | null;  // ISO date YYYY-MM-DD
  days_until_expiry: number | null;
  status: QualificationStatus;
}

export interface PilotStats {
  pilot_id: number;
  pilot_name: string;
  flight_time_total_seconds: number;
  flight_time_30d_seconds: number;
  flight_time_365d_seconds: number;
  flight_count_total: number;
  flight_count_30d: number;
  flight_count_365d: number;
  top_aircraft: TopAircraft[];
  top_models: TopModel[];
  easa_categories: CategoryBreakdown[];
  mission_types: MissionBreakdown[];
  qualifications: QualificationEntry[];
}

export function fetchPilotStats(pilotId: number): Promise<PilotStats> {
  return apiClient.get<PilotStats>(`/pilots/${pilotId}/stats`);
}
