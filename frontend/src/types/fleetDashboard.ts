// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

export type KpiSeverity = 'neutral' | 'amber' | 'red';

export interface KpiCard {
  value: number;
  secondary: string | null;
  severity: KpiSeverity;
  sparkline: number[];
}

export interface FleetDashboardKpis {
  active_aircraft: KpiCard;
  flight_hours_30d: KpiCard;
  open_defects: KpiCard;
  overdue_maintenance: KpiCard;
  batteries_near_eol: KpiCard;
}

export interface MostUsedAircraftRow {
  aircraft_id: number;
  equipment_id: string | null;
  serial_number: string;
  model_name: string | null;
  total_hours: number;
  total_flights: number;
  last_flight: string | null;
}

export interface BatteryNearEolRow {
  battery_id: number;
  equipment_id: string | null;
  serial_number: string;
  model: string | null;
  cycle_count: number;
  max_cycles: number;
  cycle_pct: number;
  last_used: string | null;
}

export interface MaintenanceAlertRow {
  schedule_id: number;
  aircraft_id: number;
  aircraft_equipment_id: string | null;
  aircraft_serial_number: string;
  schedule_name: string;
  status: 'overdue' | 'warning' | 'ok';
  days_overdue: number | null;
  consumption_pct: number;
  is_critical: boolean;
}

export interface FleetDashboard {
  kpis: FleetDashboardKpis;
  most_used_aircraft: MostUsedAircraftRow[];
  batteries_near_eol_top: BatteryNearEolRow[];
  maintenance_alerts: MaintenanceAlertRow[];
}

export interface AircraftByModelRow {
  model_id: number | null;
  model_name: string;
  aircraft_count: number;
  total_hours: number;
  total_flights: number;
  open_defects_count: number;
}

export interface AircraftByModelResponse {
  by_model: AircraftByModelRow[];
  top_aircraft_by_hours: MostUsedAircraftRow[];
}

export interface BatteryCycleBucket {
  label: string;
  lower_pct: number;
  upper_pct: number;
  count: number;
}

export interface BatteryCycleDistribution {
  buckets: BatteryCycleBucket[];
  total_active: number;
}
