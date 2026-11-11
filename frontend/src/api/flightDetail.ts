// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { apiClient } from './client';
import type { Flight } from '../types/flight';
import type { ScheduleWithStatus } from '../types/maintenanceSchedule';

export interface AircraftRef {
  id: number;
  equipment_id: string | null;
  serial_number: string;
  nickname: string | null;
  model: string | null;
  status: string;
}

export interface BatteryRef {
  id: number;
  equipment_id: string | null;
  serial_number: string;
  compatible_model: string | null;
  status: string;
}

export interface ControllerRef {
  id: number;
  equipment_id: string | null;
  serial_number: string;
  model: string | null;
  nickname: string | null;
  status: string;
}

export interface PilotRef {
  id: number;
  name: string;
  user_code: string | null;
}

export interface FlightAudit {
  imported_by_user_id: number | null;
  imported_by_user_code: string | null;
  imported_at: string;
}

/// Mirrors backend `FlightFinding` (handlers/flights/detail.rs). Findings are
/// operationally-relevant events extracted from the flight log (ArduPilot .bin
/// today). `first/last_offset_seconds` are relative to arm and may be negative
/// (pre-arm ground events). `subsys_id`/`ecode` are raw ArduPilot values.
export interface FlightFinding {
  id: number;
  source: string;
  code: string;
  subsystem: string | null;
  subsys_id: number | null;
  ecode: number | null;
  severity: string;
  message: string;
  occurrence_count: number;
  first_offset_seconds: number | null;
  last_offset_seconds: number | null;
  resolved: boolean;
}

export interface FlightDetail {
  flight: Flight;
  aircraft: AircraftRef | null;
  battery: BatteryRef | null;
  controller: ControllerRef | null;
  pilot: PilotRef | null;
  aircraft_schedules: ScheduleWithStatus[];
  audit: FlightAudit | null;
  findings: FlightFinding[];
}

export function fetchFlightDetail(flightId: number): Promise<FlightDetail> {
  return apiClient.get<FlightDetail>(`/flights/${flightId}/detail`);
}
