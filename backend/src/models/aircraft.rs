// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use chrono::{DateTime, Utc};

/// Aircraft row shape. `model_id` is the FK; `model` is the joined
/// `aircraft_models.name` and is projected by every query via the alias
/// `am.name AS model`. Bare `SELECT * FROM aircraft` is not used — always
/// JOIN through `aircraft_models` so `model` comes back populated.
#[derive(Debug, Serialize, Deserialize, FromRow, Clone)]
pub struct Aircraft {
    pub id: i32,
    pub org_id: i32,
    pub serial_number: String,
    pub equipment_id: Option<String>,
    pub model_id: Option<i32>,
    pub model: Option<String>,
    pub nickname: Option<String>,
    pub status: String,
    pub retirement_notes: Option<String>,
    pub airworthiness_status: String,
    pub total_flights: i32,
    pub total_seconds: i32,
    pub last_flight: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    /// Extra QR scan data: {ssid, wifi_password, raw_qr, source_format}.
    /// NULL for aircraft scanned manually or with no extras.
    pub device_metadata: Option<serde_json::Value>,
    /// TRUE when the QR scan didn't yield a usable SN (e.g. Tello). UI
    /// renders a warning until the operator manually corrects the SN.
    pub needs_manual_sn: bool,
    /// Measured empty weight of THIS airframe in its current configuration
    /// (airframe without battery or payload), migration 049. Distinct from
    /// the model-level nominal `aircraft_models.empty_weight_kg`.
    pub empty_weight_kg: Option<f32>,
}

/// Create request: accepts `model_id` (preferred) or `model` (name,
/// resolved case-insensitively to an existing aircraft_models row).
/// If both are present, `model_id` wins. If only `model` is present and
/// no matching row exists, the handler returns 400 — auto-creation of
/// model catalog rows is not allowed from the aircraft endpoints; use
/// `POST /api/v1/aircraft-models`. Auto-creation is only permitted in
/// the log-import path (services::aircraft_models::resolve_or_create).
#[derive(Debug, Serialize, Deserialize)]
pub struct CreateAircraftRequest {
    pub serial_number: String,
    pub equipment_id: Option<String>,
    pub model_id: Option<i32>,
    pub model: Option<String>,
    pub nickname: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateAircraftRequest {
    pub equipment_id: Option<String>,
    pub model_id: Option<i32>,
    pub model: Option<String>,
    pub nickname: Option<String>,
    pub status: Option<String>,
    pub retirement_notes: Option<String>,
    /// Only honoured for aircraft flagged `needs_manual_sn = TRUE` (e.g. Tello
    /// rows imported via QR scan). Updating the SN auto-clears the flag.
    pub serial_number: Option<String>,
    /// Measured empty weight of this airframe (kg). Must be > 0 when present.
    pub empty_weight_kg: Option<f32>,
}
