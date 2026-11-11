// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { apiClient } from './client';

export type QualificationStatus = 'ok' | 'expiring_soon' | 'expired' | 'no_expiry';

export interface PilotDetailHeader {
  id: number;
  name: string;
  email: string | null;
  phone: string | null;
  user_code: string | null;
  license_type: string | null;
  license_number: string | null;
  license_expires: string | null;
  is_active: boolean;
  notes: string | null;
}

export interface PilotDetailStats {
  total_flights: number;
  total_seconds: number;
  last_flight: string | null;
  last_90_days_flights: number;
  last_90_days_seconds: number;
  last_12_months_flights: number;
  last_12_months_seconds: number;
}

export interface PilotDetailQualification {
  id: number | null;
  type_name: string;
  issued_date: string | null;
  expiry_date: string | null;
  status: QualificationStatus;
  days_until_expiry: number | null;
  notes: string | null;
  certificate_ref: string | null;
  is_primary_license: boolean;
}

export interface CategoryStats {
  label: string;
  flights: number;
  total_seconds: number;
}

export interface PilotDetailBreakdowns {
  by_easa_category: CategoryStats[];
  by_visual_regime: CategoryStats[];
  by_daylight: CategoryStats[];
  by_aircraft_model: CategoryStats[];
  by_mission_type: CategoryStats[];
}

export interface PilotDetailIncident {
  id: number;
  occurred_at: string;
  category: string;
  severity: string;
  status: string;
  description_preview: string;
}

export interface PilotDetail {
  pilot: PilotDetailHeader;
  stats: PilotDetailStats;
  qualifications: PilotDetailQualification[];
  breakdowns: PilotDetailBreakdowns;
  recent_incidents: PilotDetailIncident[];
}

export function fetchPilotDetail(pilotId: number): Promise<PilotDetail> {
  return apiClient.get<PilotDetail>(`/pilots/${pilotId}/detail`);
}
