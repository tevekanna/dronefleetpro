// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { apiClient } from './client';

export interface AircraftDetailModel {
  id: number;
  name: string;
  manufacturer: string | null;
}

export interface AircraftDetailInfo {
  id: number;
  equipment_id: string | null;
  serial_number: string;
  nickname: string | null;
  model: AircraftDetailModel | null;
  status: string;
  registered_at: string;
  retirement_notes: string | null;
  needs_manual_sn: boolean;
  /** Measured empty weight of this airframe (kg), migration 049. null when
   *  not yet recorded — render as "not recorded", never 0 kg. */
  empty_weight_kg: number | null;
}

export interface AircraftDetailStats {
  total_flights: number;
  total_seconds: number;
  total_distance_m: number;
  last_flight: string | null;
  flights_last_30d: number;
  seconds_last_30d: number;
}

/** Equipment currently mounted on this airframe — active (non-retired)
 *  rows only, mirroring the backend `mounted` block. */
export interface MountedAccessory {
  id: number;
  name: string;
  category: string | null;
  serial_number: string | null;
  equipment_id: string | null;
  weight_kg: number | null;
  status: string;
}

export interface MountedBattery {
  id: number;
  equipment_id: string | null;
  serial_number: string;
  compatible_model: string | null;
  weight_kg: number | null;
  status: string;
}

export interface MountedController {
  id: number;
  equipment_id: string | null;
  serial_number: string;
  model: string | null;
  nickname: string | null;
  status: string;
}

export interface MountedEquipment {
  accessories: MountedAccessory[];
  batteries: MountedBattery[];
  controllers: MountedController[];
}

/** Current-configuration takeoff mass, computed server-side per request —
 *  never stored. `total_kg` is null whenever ANY input is missing: an
 *  incomplete mass is flagged explicitly, never silently under-reported.
 *  The MTOM comparison is advisory only. */
export interface TakeoffMass {
  /** Effective empty weight: airframe value falling back to the model's
   *  catalogue value. null when neither exists. */
  empty_weight_kg: number | null;
  /** Which source supplied empty_weight_kg. null when missing. */
  empty_weight_source: 'aircraft' | 'model' | null;
  /** Sum of mounted batteries' weight_kg; null when any battery lacks one. */
  batteries_kg: number | null;
  /** Sum of mounted accessories' weight_kg; null when any accessory lacks one. */
  accessories_kg: number | null;
  /** Grand total — only present when every input is known. */
  total_kg: number | null;
  /** Model's MTOM for the advisory comparison; null when unset or no model. */
  mtom_kg: number | null;
  empty_weight_missing: boolean;
  batteries_missing_weight: number;
  accessories_missing_weight: number;
}

export interface AircraftDetail {
  aircraft: AircraftDetailInfo;
  stats: AircraftDetailStats;
  mounted: MountedEquipment;
  takeoff_mass: TakeoffMass;
}

export function fetchAircraftDetail(aircraftId: number): Promise<AircraftDetail> {
  return apiClient.get<AircraftDetail>(`/aircraft/${aircraftId}/detail`);
}

// =====================================================================
// /aircraft/:id/breakdowns — categorical breakdowns for the activity
// panel on AircraftDetailPage. Same shape as PilotDetailBreakdowns minus
// the pilot-only `by_aircraft_model` field (irrelevant on a per-aircraft
// page).
// =====================================================================

export interface AircraftCategoryStats {
  label: string;
  flights: number;
  total_seconds: number;
}

export interface AircraftBreakdowns {
  by_easa_category: AircraftCategoryStats[];
  by_mission_type: AircraftCategoryStats[];
  by_visual_regime: AircraftCategoryStats[];
  by_daylight: AircraftCategoryStats[];
}

export function fetchAircraftBreakdowns(
  aircraftId: number,
): Promise<AircraftBreakdowns> {
  return apiClient.get<AircraftBreakdowns>(
    `/aircraft/${aircraftId}/breakdowns`,
  );
}
