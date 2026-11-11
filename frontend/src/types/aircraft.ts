// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

export interface Aircraft {
  id: number;
  org_id: number;
  serial_number: string;
  equipment_id: string | null;
  /** FK into aircraft_models (nullable). */
  model_id: number | null;
  /** Joined `aircraft_models.name`; populated by the backend via JOIN. */
  model: string | null;
  nickname: string | null;
  status: 'active' | 'maintenance' | 'retired';
  /** Maintenance Manager-controlled airworthiness flag (orthogonal to
   *  `status`). Set via PATCH /aircraft/:id/airworthiness. New aircraft
   *  default to `not_airworthy` and must be cleared before first flight. */
  airworthiness_status: 'airworthy' | 'not_airworthy';
  retirement_notes: string | null;
  total_flights: number;
  total_seconds: number;
  last_flight: string | null;
  created_at: string;
  /** Extra QR scan data (SSID, WiFi password, raw QR, source_format). NULL
   *  for aircraft entered manually. */
  device_metadata: Record<string, unknown> | null;
  /** TRUE when the QR scan didn't yield a usable SN (e.g. Tello). UI shows
   *  a warning until the operator corrects it. */
  needs_manual_sn: boolean;
  /** Measured empty weight of this airframe (kg), migration 049. null when
   *  not yet recorded — render as "not recorded", never 0 kg. */
  empty_weight_kg: number | null;
}

export interface CreateAircraftRequest {
  serial_number: string;
  equipment_id?: string;
  /** Preferred: the model_id from the aircraft_models catalog. */
  model_id?: number;
  /** Alternative: a model name string, resolved case-insensitively to an
   *  existing aircraft_models row. 400 if no match. model_id wins if both. */
  model?: string;
  nickname?: string;
}

export interface UpdateAircraftRequest {
  equipment_id?: string;
  model_id?: number;
  model?: string;
  nickname?: string;
  status?: string;
  retirement_notes?: string;
  /** Only sent when the operator is correcting a needs_manual_sn aircraft.
   *  Backend auto-clears needs_manual_sn iff this differs from the existing SN. */
  serial_number?: string;
  /** Measured empty weight of this airframe (kg). Must be > 0 when present. */
  empty_weight_kg?: number | null;
}
