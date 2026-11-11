// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use chrono::{DateTime, Utc};

#[derive(Debug, Serialize, Deserialize, FromRow, Clone)]
pub struct Battery {
    pub id: i32,
    pub org_id: i32,
    pub aircraft_id: Option<i32>,
    pub serial_number: String,
    pub equipment_id: Option<String>,
    pub cycle_count: i32,
    pub max_cycles: i32,
    pub capacity_wh: Option<f32>,
    /// Measured per-unit battery mass in kg (migration 050). Distinct from
    /// `capacity_wh`, which is energy, not mass. Input to the planned
    /// take-off-mass vs MTOM check.
    pub weight_kg: Option<f32>,
    pub compatible_model: Option<String>,
    /// Structured compatibility (migration 052) — nullable FK to
    /// aircraft_models. The free-text compatible_model stays as
    /// log-derived source data; this is the opportunistically matched
    /// or manually set relation.
    pub compatible_model_id: Option<i32>,
    pub last_used: Option<DateTime<Utc>>,
    pub status: String,
    pub retirement_notes: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateBatteryRequest {
    pub serial_number: String,
    pub equipment_id: Option<String>,
    pub aircraft_id: Option<i32>,
    pub capacity_wh: Option<f32>,
    pub weight_kg: Option<f32>,
    pub max_cycles: Option<i32>,
    pub compatible_model: Option<String>,
    pub compatible_model_id: Option<i32>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateBatteryRequest {
    pub equipment_id: Option<String>,
    #[serde(default, deserialize_with = "crate::models::serde_helpers::double_option")]
    pub aircraft_id: Option<Option<i32>>,
    pub capacity_wh: Option<f32>,
    pub weight_kg: Option<f32>,
    pub max_cycles: Option<i32>,
    pub compatible_model: Option<String>,
    #[serde(default, deserialize_with = "crate::models::serde_helpers::double_option")]
    pub compatible_model_id: Option<Option<i32>>,
    pub status: Option<String>,
    pub retirement_notes: Option<String>,
}
