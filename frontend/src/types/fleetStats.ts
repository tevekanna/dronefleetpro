// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

export interface EstimatedRange {
  low: number;
  mid: number;
  high: number;
}

export interface BatteryStats {
  battery_id: number;
  serial_number: string;
  equipment_id: string | null;
  model: string | null;
  status: string;
  cycle_count: number;
  max_cycles: number;
  nominal_max_cycles: number | null;
  cycle_uncertainty: number | null;
  total_flights: number;
  total_flight_seconds: number;
  avg_discharge_per_flight: number | null;
  last_used: string | null;
  avg_cell_variance_v: number | null;
  avg_min_temp_c: number | null;
  avg_min_battery_pct: number | null;
  estimated_remaining_cycles: EstimatedRange | null;
  estimated_remaining_pct: EstimatedRange | null;
}

export interface BatteryStatsSummary {
  battery_id: number;
  serial_number: string;
  equipment_id: string | null;
  model: string | null;
  cycle_count: number;
  total_flights: number;
  total_flight_seconds: number;
  last_used: string | null;
}

export interface HistogramBucket {
  lower: number;
  upper: number;
  count: number;
}

export interface BatteryModelComparison {
  model: string;
  battery_count: number;
  nominal_max_cycles: number | null;
  cycle_uncertainty: number | null;
  nominal_capacity_wh: number | null;
  avg_cycle_count: number;
  min_cycle_count: number;
  max_cycle_count: number;
  avg_discharge_per_flight: number | null;
  avg_cell_variance_v: number | null;
  cycle_distribution: HistogramBucket[];
  batteries: BatteryStatsSummary[];
}

export interface ControllerAircraftUsage {
  aircraft_id: number;
  aircraft_serial: string;
  aircraft_equipment_id: string | null;
  aircraft_model: string | null;
  flight_count: number;
  flight_seconds: number;
  last_flight: string | null;
}

export interface ControllerStats {
  controller_id: number;
  serial_number: string;
  equipment_id: string | null;
  model: string | null;
  status: string;
  total_flights: number;
  total_flight_seconds: number;
  first_used: string | null;
  last_used: string | null;
  days_since_last_use: number | null;
  usage_by_aircraft: ControllerAircraftUsage[];
  inactivity_warning: boolean;
}

export interface ControllerStatsSummary {
  controller_id: number;
  serial_number: string;
  equipment_id: string | null;
  model: string | null;
  total_flights: number;
  total_flight_seconds: number;
  last_used: string | null;
}

export interface FleetStatsOverview {
  total_batteries_active: number;
  total_batteries_near_eol: number;
  total_batteries_past_eol: number;
  total_controllers_active: number;
  total_controllers_inactivity_warning: number;
  battery_model_summary: BatteryModelComparison[];
  recently_flown_batteries_top5: BatteryStatsSummary[];
  recently_flown_controllers_top5: ControllerStatsSummary[];
}

export interface HealthSample {
  flight_id: number;
  flight_datetime: string | null;
  min_battery_pct: number | null;
  battery_temp_c: number | null;
  cell_variance_v: number | null;
  duration_seconds: number;
}

export interface BatteryStatsDetail extends BatteryStats {
  health_timeseries: HealthSample[];
}
