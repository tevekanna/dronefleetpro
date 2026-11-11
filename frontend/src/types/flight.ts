// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import type { AircraftRef, BatteryRef } from '../api/flightDetail';

export interface Flight {
  id: number;
  org_id: number;
  pilot_id: number | null;
  aircraft_id: number | null;
  battery_id: number | null;
  flight_datetime: string | null;
  flight_start: string | null;
  flight_end: string | null;
  duration_seconds: number;
  max_altitude_m: number;
  max_distance_m: number;
  total_distance_m: number;
  max_speed_ms: number | null;
  home_lat: number | null;
  home_lon: number | null;
  satellite_count_min: number | null;
  battery_temp_c: number | null;
  cell_variance_v: number | null;
  min_battery_pct: number | null;
  easa_category: string | null;
  mission_type: string | null;
  operation_name: string | null;
  operation_description: string | null;
  preflight_done: boolean;
  remarks: string | null;
  status: string | null;
  log_source: string | null;
  source_filename: string | null;
  flight_guid: string | null;
  controller_id: number | null;
  pilot_monitoring_id: number | null;
  aircraft_name: string | null;
  aircraft_serial: string | null;
  pic_name: string | null;
  pm_name: string | null;
  photo_count: number;
  video_seconds: number;
  trajectory_json: Array<{lat: number; lon: number; alt: number; t_sec: number}> | null;
  deleted_at: string | null;
  created_at: string;
  /**
   * Resolved aircraft (equipment_id, model name, status). Populated by
   * `GET /flights` so list views can render a two-line cell without a
   * second round-trip. `null` when the flight has no linked aircraft.
   * Not present on the bare `GET /flights/:id` response — there the
   * /detail endpoint carries the same data under a separate field.
   */
  aircraft?: AircraftRef | null;
  /**
   * Resolved battery (equipment_id, compatible_model, SN, status).
   * Populated by `GET /flights` so list views can render a linked
   * battery cell without a second round-trip. `null` when the flight
   * has no linked battery. Not present on the bare `GET /flights/:id`
   * response — there the /detail endpoint carries the same data under a
   * separate field.
   */
  battery?: BatteryRef | null;
  /**
   * Sortie grouping metadata, populated by `GET /flights` only.
   * VIEW-LEVEL — computed per-request server-side, never persisted; the
   * individual flight stays the unit of truth for the EASA logbook.
   *
   * A sortie groups consecutive same-aircraft flights whose gap
   * (previous landing → next takeoff) is under the threshold. `sortie_id`
   * is stable only within one response (equal for flights in the same
   * sortie, distinct across sorties) — not a database id. `sortie_size`
   * and `sortie_total_seconds` are computed over the ENTIRE filtered set,
   * so a group split across a page boundary still reports correct totals.
   * Optional because the bare `GET /flights/:id` response omits them.
   */
  sortie_id?: number;
  sortie_size?: number;
  sortie_total_seconds?: number;
}

export interface CreateFlightRequest {
  pilot_id?: number;
  aircraft_id?: number;
  battery_id?: number;
  flight_datetime?: string;
  duration_seconds?: number;
  max_altitude_m?: number;
  max_distance_m?: number;
  easa_category?: string;
  mission_type?: string;
  operation_name?: string;
  remarks?: string;
  preflight_done?: boolean;
}

export interface FlightFilters {
  dateFrom?: string;
  dateTo?: string;
  aircraftId?: number;
  pilotId?: number;
  missionType?: string;
  easaCategory?: string;
  search?: string;
  page?: number;
  perPage?: number;
}

export interface FlightListResponse {
  flights: Flight[];
  total: number;
  page: number;
  per_page: number;
  total_pages: number;
}

export interface FlightStats {
  total_flights: number;
  total_seconds: number;
  total_hours: number;
  avg_duration_seconds: number;
  max_altitude_m: number;
  total_distance_km: number;
  last_flight: string | null;
}

export interface FlightFilterOptions {
  mission_types: string[];
  easa_categories: string[];
}
