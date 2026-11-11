// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::FromRow;
use chrono::{DateTime, Utc};

#[derive(Debug, Serialize, Deserialize, FromRow, Clone)]
pub struct Flight {
    pub id: i32,
    pub org_id: i32,
    pub pilot_id: Option<i32>,
    pub pilot_monitoring_id: Option<i32>,
    pub aircraft_id: Option<i32>,
    pub battery_id: Option<i32>,
    pub controller_id: Option<i32>,
    pub flight_datetime: Option<DateTime<Utc>>,
    pub flight_start: Option<DateTime<Utc>>,
    pub flight_end: Option<DateTime<Utc>>,
    pub duration_seconds: i32,
    pub max_altitude_m: f32,
    pub max_distance_m: f32,
    pub total_distance_m: f32,
    pub max_speed_ms: Option<f32>,
    pub home_lat: Option<f32>,
    pub home_lon: Option<f32>,
    pub satellite_count_min: Option<i32>,
    pub battery_temp_c: Option<f32>,
    pub cell_variance_v: Option<f32>,
    pub min_battery_pct: Option<i32>,
    pub easa_category: Option<String>,
    pub mission_type: Option<String>,
    pub operation_name: Option<String>,
    pub operation_description: Option<String>,
    pub preflight_done: bool,
    pub remarks: Option<String>,
    pub status: Option<String>,
    pub log_source: Option<String>,
    pub source_filename: Option<String>,
    pub flight_guid: Option<String>,
    pub trajectory_json: Option<Value>,
    pub photo_count: i32,
    pub video_seconds: i32,
    pub aircraft_name: Option<String>,
    pub aircraft_serial: Option<String>,
    pub pic_name: Option<String>,
    pub pm_name: Option<String>,
    pub deleted_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateFlightRequest {
    pub pilot_id: Option<i32>,
    pub aircraft_id: Option<i32>,
    pub battery_id: Option<i32>,
    pub flight_datetime: Option<String>,
    pub duration_seconds: Option<i32>,
    pub max_altitude_m: Option<f32>,
    pub max_distance_m: Option<f32>,
    pub total_distance_m: Option<f32>,
    pub easa_category: Option<String>,
    pub mission_type: Option<String>,
    pub operation_name: Option<String>,
    pub remarks: Option<String>,
    pub preflight_done: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, FromRow)]
pub struct FlightStats {
    pub total_flights: i64,
    pub total_seconds: i64,
    pub total_hours: f64,
    pub avg_duration_seconds: f64,
    pub max_altitude_m: f64,
    pub last_flight: Option<DateTime<Utc>>,
}
