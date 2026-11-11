// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::FromRow;

#[derive(Debug, Serialize, Clone)]
pub struct EstimatedRange {
    pub low: i32,
    pub mid: i32,
    pub high: i32,
}

#[derive(Debug, Serialize, FromRow)]
pub struct BatteryStats {
    pub battery_id: i32,
    pub serial_number: String,
    pub equipment_id: Option<String>,
    pub model: Option<String>,
    pub status: String,

    pub cycle_count: i32,
    pub max_cycles: i32,
    pub nominal_max_cycles: Option<i32>,
    pub cycle_uncertainty: Option<i32>,

    pub total_flights: i64,
    pub total_flight_seconds: i64,
    pub avg_discharge_per_flight: Option<f32>,
    pub last_used: Option<DateTime<Utc>>,

    pub avg_cell_variance_v: Option<f32>,
    pub avg_min_temp_c: Option<f32>,
    pub avg_min_battery_pct: Option<f32>,

    #[sqlx(skip)]
    pub estimated_remaining_cycles: Option<EstimatedRange>,
    #[sqlx(skip)]
    pub estimated_remaining_pct: Option<EstimatedRange>,
}

#[derive(Debug, Serialize, FromRow, Clone)]
pub struct BatteryStatsSummary {
    pub battery_id: i32,
    pub serial_number: String,
    pub equipment_id: Option<String>,
    pub model: Option<String>,
    pub cycle_count: i32,
    pub total_flights: i64,
    pub total_flight_seconds: i64,
    pub last_used: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
pub struct BatteryModelComparison {
    pub model: String,
    pub battery_count: i32,
    pub nominal_max_cycles: Option<i32>,
    pub cycle_uncertainty: Option<i32>,
    pub nominal_capacity_wh: Option<f32>,
    pub avg_cycle_count: f32,
    pub min_cycle_count: i32,
    pub max_cycle_count: i32,
    pub avg_discharge_per_flight: Option<f32>,
    pub avg_cell_variance_v: Option<f32>,
    pub cycle_distribution: Vec<HistogramBucket>,
    pub batteries: Vec<BatteryStatsSummary>,
}

#[derive(Debug, Serialize, Clone)]
pub struct HistogramBucket {
    pub lower: i32,
    pub upper: i32,
    pub count: i32,
}

#[derive(Debug, Serialize, FromRow, Clone)]
pub struct ControllerAircraftUsage {
    pub aircraft_id: i32,
    pub aircraft_serial: String,
    pub aircraft_equipment_id: Option<String>,
    pub aircraft_model: Option<String>,
    pub flight_count: i64,
    pub flight_seconds: i64,
    pub last_flight: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
pub struct ControllerStats {
    pub controller_id: i32,
    pub serial_number: String,
    pub equipment_id: Option<String>,
    pub model: Option<String>,
    pub status: String,

    pub total_flights: i64,
    pub total_flight_seconds: i64,
    pub first_used: Option<DateTime<Utc>>,
    pub last_used: Option<DateTime<Utc>>,
    pub days_since_last_use: Option<i32>,

    pub usage_by_aircraft: Vec<ControllerAircraftUsage>,
    pub inactivity_warning: bool,
}

#[derive(Debug, Serialize, FromRow, Clone)]
pub struct ControllerStatsSummary {
    pub controller_id: i32,
    pub serial_number: String,
    pub equipment_id: Option<String>,
    pub model: Option<String>,
    pub total_flights: i64,
    pub total_flight_seconds: i64,
    pub last_used: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
pub struct FleetStatsOverview {
    pub total_batteries_active: i64,
    pub total_batteries_near_eol: i64,
    pub total_batteries_past_eol: i64,
    pub total_controllers_active: i64,
    pub total_controllers_inactivity_warning: i64,

    pub battery_model_summary: Vec<BatteryModelComparison>,
    pub recently_flown_batteries_top5: Vec<BatteryStatsSummary>,
    pub recently_flown_controllers_top5: Vec<ControllerStatsSummary>,
}

#[derive(Debug, Serialize, FromRow)]
pub struct HealthSample {
    pub flight_id: i32,
    pub flight_datetime: Option<DateTime<Utc>>,
    pub min_battery_pct: Option<i32>,
    pub battery_temp_c: Option<f32>,
    pub cell_variance_v: Option<f32>,
    pub duration_seconds: i32,
}

#[derive(Debug, Serialize)]
pub struct BatteryStatsDetail {
    #[serde(flatten)]
    pub stats: BatteryStats,
    pub health_timeseries: Vec<HealthSample>,
}
