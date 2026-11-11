// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Serialize, Deserialize, FromRow, Clone)]
pub struct AircraftModel {
    pub id: i32,
    pub org_id: i32,
    pub name: String,
    pub manufacturer: Option<String>,
    pub category: Option<String>,
    pub is_active: bool,
    pub notes: Option<String>,
    pub default_max_flight_time_minutes: Option<i32>,
    /// Legacy free-text weight from migration 012; superseded for regulatory
    /// purposes by the fields below but retained for export/import stability.
    pub default_weight_kg: Option<f32>,
    /// Regulatory weight + EASA class fields (migration 049).
    pub mtom_kg: Option<f32>,
    pub empty_weight_kg: Option<f32>,
    pub max_payload_kg: Option<f32>,
    pub class_marking: Option<String>,
    pub max_dimension_cm: Option<f32>,
    /// Whether the design requires a certificate or declaration under
    /// Article 56(1) and (5) of Reg (EU) 2018/1139 (migration 054):
    /// 'required' | 'not_required'. None means NOT DETERMINED and must
    /// never be treated as 'not_required'.
    pub design_approval_required: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Row returned by the legacy list endpoint variant: model + count of aircraft
/// pointing at it. Retained so any internal handlers and tests still on this
/// shape keep compiling. The public list endpoint now returns
/// `AircraftModelWithStats` instead.
#[derive(Debug, Serialize, FromRow)]
pub struct AircraftModelWithCount {
    pub id: i32,
    pub org_id: i32,
    pub name: String,
    pub manufacturer: Option<String>,
    pub category: Option<String>,
    pub is_active: bool,
    pub notes: Option<String>,
    pub default_max_flight_time_minutes: Option<i32>,
    pub default_weight_kg: Option<f32>,
    pub mtom_kg: Option<f32>,
    pub empty_weight_kg: Option<f32>,
    pub max_payload_kg: Option<f32>,
    pub class_marking: Option<String>,
    pub max_dimension_cm: Option<f32>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub aircraft_count: i64,
}

/// Row returned by `GET /aircraft-models` — model fields plus aggregate flight
/// stats SUM'd over the denormalized counters on the aircraft table. No JOIN
/// to `flights` is required because `aircraft.total_flights` /
/// `aircraft.total_seconds` / `aircraft.last_flight` are already maintained.
#[derive(Debug, Serialize, FromRow)]
pub struct AircraftModelWithStats {
    pub id: i32,
    pub org_id: i32,
    pub name: String,
    pub manufacturer: Option<String>,
    pub category: Option<String>,
    pub is_active: bool,
    pub notes: Option<String>,
    pub default_max_flight_time_minutes: Option<i32>,
    pub default_weight_kg: Option<f32>,
    pub mtom_kg: Option<f32>,
    pub empty_weight_kg: Option<f32>,
    pub max_payload_kg: Option<f32>,
    pub class_marking: Option<String>,
    pub max_dimension_cm: Option<f32>,
    /// See `AircraftModel::design_approval_required`. Carried on the list/
    /// stats shape so the edit form can show (and preserve) the stored
    /// assessment instead of silently clearing it on save.
    pub design_approval_required: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub aircraft_count: i64,
    pub active_count: i64,
    pub maintenance_count: i64,
    pub retired_count: i64,
    pub total_flights: i64,
    pub total_flight_seconds: i64,
    pub last_flight: Option<DateTime<Utc>>,
}

/// One row of the per-model individuals list returned by
/// `GET /aircraft-models/:id/stats`. `total_flights` and `total_seconds`
/// match the underlying integer columns on `aircraft` (i32, not i64) —
/// SUM aggregates elsewhere widen to bigint, but raw row reads stay i32.
#[derive(Debug, Serialize, FromRow)]
pub struct AircraftModelIndividual {
    pub id: i32,
    pub serial_number: String,
    pub equipment_id: Option<String>,
    pub nickname: Option<String>,
    pub status: String,
    pub total_flights: i32,
    pub total_seconds: i32,
    pub last_flight: Option<DateTime<Utc>>,
    pub needs_manual_sn: bool,
}

/// Response shape for `GET /aircraft-models/:id/stats` — aggregate model row
/// flattened at the top level, plus an embedded list of individuals.
#[derive(Debug, Serialize)]
pub struct AircraftModelStatsDetail {
    #[serde(flatten)]
    pub model: AircraftModelWithStats,
    pub individuals: Vec<AircraftModelIndividual>,
}

#[derive(Debug, Deserialize)]
pub struct CreateAircraftModelRequest {
    pub name: String,
    pub manufacturer: Option<String>,
    pub category: Option<String>,
    pub notes: Option<String>,
    pub default_max_flight_time_minutes: Option<i32>,
    pub default_weight_kg: Option<f32>,
    pub mtom_kg: Option<f32>,
    pub empty_weight_kg: Option<f32>,
    pub max_payload_kg: Option<f32>,
    pub class_marking: Option<String>,
    pub max_dimension_cm: Option<f32>,
    /// 'required' | 'not_required' | omitted/null = NOT DETERMINED. Plain
    /// Option (not double_option) — on create there is no stored value to
    /// keep, so null and omitted both mean "not determined".
    pub design_approval_required: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateAircraftModelRequest {
    pub name: Option<String>,
    pub manufacturer: Option<String>,
    pub category: Option<String>,
    pub is_active: Option<bool>,
    pub notes: Option<String>,
    pub default_max_flight_time_minutes: Option<i32>,
    pub default_weight_kg: Option<f32>,
    pub mtom_kg: Option<f32>,
    pub empty_weight_kg: Option<f32>,
    pub max_payload_kg: Option<f32>,
    pub class_marking: Option<String>,
    pub max_dimension_cm: Option<f32>,
    /// Tri-state: omitted -> keep, explicit null -> back to NOT DETERMINED,
    /// value -> set. Clearing must be possible because a wrong applicability
    /// assessment silently changes whether the 72-hour reporting clock is
    /// presented as a legal obligation.
    #[serde(default, deserialize_with = "crate::models::serde_helpers::double_option")]
    pub design_approval_required: Option<Option<String>>,
}

#[derive(Debug, Deserialize)]
pub struct MergeAircraftModelRequest {
    pub target_id: i32,
}
