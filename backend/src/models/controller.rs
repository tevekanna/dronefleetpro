// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use chrono::{DateTime, Utc};

#[derive(Debug, Serialize, Deserialize, FromRow, Clone)]
pub struct Controller {
    pub id: i32,
    pub org_id: i32,
    pub serial_number: String,
    pub equipment_id: Option<String>,
    pub model: Option<String>,
    pub nickname: Option<String>,
    pub aircraft_id: Option<i32>,
    pub status: String,
    pub notes: Option<String>,
    pub compatible_models: Option<String>,
    pub retirement_notes: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateControllerRequest {
    pub serial_number: String,
    pub equipment_id: Option<String>,
    pub model: Option<String>,
    pub nickname: Option<String>,
    pub aircraft_id: Option<i32>,
    pub notes: Option<String>,
    pub compatible_models: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateControllerRequest {
    pub equipment_id: Option<String>,
    pub model: Option<String>,
    pub nickname: Option<String>,
    #[serde(default, deserialize_with = "crate::models::serde_helpers::double_option")]
    pub aircraft_id: Option<Option<i32>>,
    pub status: Option<String>,
    pub notes: Option<String>,
    pub compatible_models: Option<String>,
    pub retirement_notes: Option<String>,
}
