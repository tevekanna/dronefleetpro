// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::FromRow;

#[derive(Debug, Serialize)]
pub struct KpiCard {
    pub value: f64,
    pub secondary: Option<String>,
    pub severity: String, // "neutral" | "amber" | "red"
    pub sparkline: Vec<f64>,
}

#[derive(Debug, Serialize)]
pub struct FleetDashboardKpis {
    pub active_aircraft: KpiCard,
    pub flight_hours_30d: KpiCard,
    pub open_defects: KpiCard,
    pub overdue_maintenance: KpiCard,
    pub batteries_near_eol: KpiCard,
}

#[derive(Debug, Serialize, FromRow)]
pub struct MostUsedAircraftRow {
    pub aircraft_id: i32,
    pub equipment_id: Option<String>,
    pub serial_number: String,
    pub model_name: Option<String>,
    pub total_hours: f64,
    pub total_flights: i64,
    pub last_flight: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize, FromRow)]
pub struct BatteryNearEolRow {
    pub battery_id: i32,
    pub equipment_id: Option<String>,
    pub serial_number: String,
    pub model: Option<String>,
    pub cycle_count: i32,
    pub max_cycles: i32,
    pub cycle_pct: f64,
    pub last_used: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
pub struct MaintenanceAlertRow {
    pub schedule_id: i32,
    pub aircraft_id: i32,
    pub aircraft_equipment_id: Option<String>,
    pub aircraft_serial_number: String,
    pub schedule_name: String,
    pub status: String, // "overdue" | "warning"
    pub days_overdue: Option<i64>,
    pub consumption_pct: f64,
    pub is_critical: bool,
}

#[derive(Debug, Serialize)]
pub struct FleetDashboard {
    pub kpis: FleetDashboardKpis,
    pub most_used_aircraft: Vec<MostUsedAircraftRow>,
    pub batteries_near_eol_top: Vec<BatteryNearEolRow>,
    pub maintenance_alerts: Vec<MaintenanceAlertRow>,
}

#[derive(Debug, Serialize, FromRow)]
pub struct AircraftByModelRow {
    pub model_id: Option<i32>,
    pub model_name: String,
    pub aircraft_count: i64,
    pub total_hours: f64,
    pub total_flights: i64,
    pub open_defects_count: i64,
}

#[derive(Debug, Serialize)]
pub struct AircraftByModelResponse {
    pub by_model: Vec<AircraftByModelRow>,
    pub top_aircraft_by_hours: Vec<MostUsedAircraftRow>,
}

#[derive(Debug, Serialize)]
pub struct BatteryCycleBucket {
    pub label: String,
    pub lower_pct: i32,
    pub upper_pct: i32,
    pub count: i64,
}

#[derive(Debug, Serialize)]
pub struct BatteryCycleDistribution {
    pub buckets: Vec<BatteryCycleBucket>,
    pub total_active: i64,
}
