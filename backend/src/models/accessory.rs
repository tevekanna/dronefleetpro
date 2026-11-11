// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use chrono::{DateTime, Utc};

/// A weighted piece of equipment (camera, gimbal, payload module, mount,
/// sensor, ...) held in the org inventory. Mirrors the battery / controller
/// equipment shape, including the nullable `aircraft_id` mount link
/// (migration 051); the take-off-mass calculation is deferred to a later
/// PR. `serial_number` is nullable because many accessories carry no
/// serial number at all.
#[derive(Debug, Serialize, Deserialize, FromRow, Clone)]
pub struct Accessory {
    pub id: i32,
    pub org_id: i32,
    pub aircraft_id: Option<i32>,
    pub name: String,
    pub category: Option<String>,
    pub manufacturer: Option<String>,
    pub serial_number: Option<String>,
    pub equipment_id: Option<String>,
    pub weight_kg: Option<f32>,
    pub status: String,
    pub retirement_notes: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateAccessoryRequest {
    pub name: String,
    pub aircraft_id: Option<i32>,
    pub category: Option<String>,
    pub manufacturer: Option<String>,
    pub serial_number: Option<String>,
    pub equipment_id: Option<String>,
    pub weight_kg: Option<f32>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateAccessoryRequest {
    pub name: Option<String>,
    #[serde(default, deserialize_with = "crate::models::serde_helpers::double_option")]
    pub aircraft_id: Option<Option<i32>>,
    pub category: Option<String>,
    pub manufacturer: Option<String>,
    pub serial_number: Option<String>,
    pub equipment_id: Option<String>,
    pub weight_kg: Option<f32>,
    pub status: Option<String>,
    pub retirement_notes: Option<String>,
}
