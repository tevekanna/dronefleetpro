// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

import { apiClient } from './client';

export interface DashboardStats {
  total_flights: number;
  total_seconds: number;
  total_distance_km: number;
  active_aircraft: number;
  open_defects: number;
  airworthy_aircraft: number;
  not_airworthy_aircraft: number;
  total_pilots: number;
  total_aircraft_models: number;
}

export interface RecentFlight {
  id: number;
  flight_datetime: string | null;
  aircraft_name: string | null;
  equipment_id: string | null;
  duration_seconds: number;
  max_altitude_m: number;
  easa_category: string | null;
  mission_type: string | null;
  log_source: string | null;
}

export interface FleetStatusItem {
  id: number;
  equipment_id: string | null;
  nickname: string | null;
  model: string | null;
  status: string;
  flight_count: number;
  total_seconds: number;
}

export interface ExpiringLicense {
  id: number;
  name: string;
  license_type: string | null;
  license_expires: string | null;
  days_until_expiry: number | null;
}

export interface OpenDefect {
  id: number;
  aircraft_name: string | null;
  description: string;
  defect_date: string;
  reported_by: string;
}

export interface RecentMaintenance {
  id: number;
  aircraft_name: string | null;
  performed_date: string;
  maint_category: string | null;
  maint_type: string | null;
  remark: string | null;
  sign_off: string;
}

export interface DashboardData {
  stats: DashboardStats;
  recent_flights: RecentFlight[];
  fleet_status: FleetStatusItem[];
  expiring_licenses: ExpiringLicense[];
  open_defects: OpenDefect[];
  recent_maintenance: RecentMaintenance[];
}

export async function fetchDashboard(): Promise<DashboardData> {
  return apiClient.get('/dashboard');
}

export interface FlightMapPoint {
  id: number;
  lat: number;
  lon: number;
  flight_datetime: string | null;
  aircraft_name: string | null;
}

export async function fetchFlightMap(): Promise<FlightMapPoint[]> {
  return apiClient.get('/dashboard/flight-map');
}
