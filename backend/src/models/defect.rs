// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use chrono::{DateTime, Utc, NaiveDate};

#[derive(Debug, Serialize, Deserialize, FromRow, Clone)]
pub struct DefectRecord {
    pub id: i32,
    pub org_id: i32,
    pub aircraft_id: Option<i32>,
    pub flight_id: Option<i32>,
    pub defect_date: NaiveDate,
    pub reported_by: String,
    pub description: String,
    pub status: String,
    pub action_taken: Option<String>,
    pub closed_by: Option<String>,
    pub closed_date: Option<NaiveDate>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateDefectRequest {
    pub aircraft_id: i32,
    pub defect_date: String,
    pub flight_id: Option<i32>,
    pub reported_by: String,
    pub description: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateDefectRequest {
    pub status: String,
    pub action_taken: Option<String>,
    pub closed_by: Option<String>,
}
