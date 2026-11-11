// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use chrono::{DateTime, NaiveDate, Utc};

/// A single battery health/capacity test record. Maps the full
/// `battery_tests` table, including the legacy columns reused by this
/// feature: `capacity_measured` is in Wh and `internal_resistance` is in
/// mΩ (milliohms) by convention.
#[derive(Debug, Serialize, Deserialize, FromRow, Clone)]
pub struct BatteryTest {
    pub id: i32,
    pub org_id: i32,
    pub battery_id: i32,
    pub test_date: NaiveDate,
    pub test_type: String,
    pub result: String,
    pub capacity_measured: Option<f32>,
    pub internal_resistance: Option<f32>,
    pub swelling_status: String,
    pub cell_voltages: Option<serde_json::Value>,
    pub notes: Option<String>,
    pub tested_by: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Request body for creating a battery test. `test_type` and `tested_by`
/// are server-set (never taken from the client).
#[derive(Debug, Deserialize)]
pub struct CreateBatteryTestRequest {
    pub test_date: NaiveDate,
    pub result: String,
    pub swelling_status: Option<String>,
    pub capacity_measured: Option<f32>,
    pub internal_resistance: Option<f32>,
    pub cell_voltages: Option<Vec<f32>>,
    pub notes: Option<String>,
}
