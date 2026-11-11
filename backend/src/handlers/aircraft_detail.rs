// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

//! GET /api/v1/aircraft/:id/detail — minimal data block for the aircraft
//! detail page (header card + stats cards).
//!
//! Read-only, org-scoped, any authenticated user. Also carries the
//! `mounted` block: equipment (accessories, batteries, controllers)
//! currently mounted on the airframe. The remaining richer sections
//! (maintenance schedules, defect history, per-aircraft flight log)
//! live on their own endpoints.
//!
//! Aggregates come from `services::aircraft_aggregates` so this endpoint
//! and any future aircraft-stats endpoint can never disagree on what
//! counts as "a flight".

use axum::{
    extract::{Path, State},
    Json,
};
use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::auth::middleware::Claims;
use crate::db::DbPool;
use crate::errors::AppError;
use crate::services::aircraft_aggregates as agg;
use crate::services::sun_position::{
    classify_flight, split_duration_by_daylight, DayNight,
};

#[cfg(test)]
#[path = "aircraft_detail_tests.rs"]
mod tests;

#[derive(Debug, Serialize)]
pub struct AircraftDetailResponse {
    pub aircraft: AircraftInfo,
    pub stats: AircraftStats,
    pub mounted: MountedEquipment,
    pub takeoff_mass: TakeoffMass,
}

/// Equipment currently mounted on this airframe — the "linked equipment"
/// block deferred from the original thin endpoint. Active (non-retired)
/// rows only: a retired battery still pointing at the aircraft is an
/// inventory artefact, not something physically on the airframe.
#[derive(Debug, Serialize)]
pub struct MountedEquipment {
    pub accessories: Vec<MountedAccessory>,
    pub batteries: Vec<MountedBattery>,
    pub controllers: Vec<MountedController>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct MountedAccessory {
    pub id: i32,
    pub name: String,
    pub category: Option<String>,
    pub serial_number: Option<String>,
    pub equipment_id: Option<String>,
    pub weight_kg: Option<f32>,
    pub status: String,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct MountedBattery {
    pub id: i32,
    pub equipment_id: Option<String>,
    pub serial_number: String,
    pub compatible_model: Option<String>,
    pub weight_kg: Option<f32>,
    pub status: String,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct MountedController {
    pub id: i32,
    pub equipment_id: Option<String>,
    pub serial_number: String,
    pub model: Option<String>,
    pub nickname: Option<String>,
    pub status: String,
}

#[derive(Debug, Serialize)]
pub struct AircraftInfo {
    pub id: i32,
    pub equipment_id: Option<String>,
    pub serial_number: String,
    pub nickname: Option<String>,
    pub model: Option<ModelInfo>,
    pub status: String,
    pub registered_at: DateTime<Utc>,
    pub retirement_notes: Option<String>,
    pub needs_manual_sn: bool,
    /// Measured empty weight of this airframe (kg), migration 049. NULL when
    /// not yet recorded — the UI must render that as "not recorded", not 0 kg.
    pub empty_weight_kg: Option<f32>,
}

#[derive(Debug, Serialize)]
pub struct ModelInfo {
    pub id: i32,
    pub name: String,
    pub manufacturer: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AircraftStats {
    pub total_flights: i64,
    pub total_seconds: i64,
    pub total_distance_m: f64,
    pub last_flight: Option<DateTime<Utc>>,
    pub flights_last_30d: i64,
    pub seconds_last_30d: i64,
}

/// Current-configuration takeoff mass, computed per-request from the mounted
/// equipment vectors — never stored. The MTOM comparison is advisory only.
#[derive(Debug, Serialize)]
pub struct TakeoffMass {
    /// Effective empty weight used: the airframe's own measured value,
    /// falling back to the model's catalogue value. None when neither exists.
    pub empty_weight_kg: Option<f32>,
    /// "aircraft" | "model" — which source supplied empty_weight_kg. None when missing.
    pub empty_weight_source: Option<String>,
    /// Sum of mounted batteries' weight_kg. None when ANY mounted battery lacks a weight.
    pub batteries_kg: Option<f32>,
    /// Sum of mounted accessories' weight_kg. None when ANY mounted accessory lacks a weight.
    pub accessories_kg: Option<f32>,
    /// Grand total. Some ONLY when empty weight is known AND no mounted item
    /// is missing its weight. Never a silently-partial figure.
    pub total_kg: Option<f32>,
    /// Model's MTOM for the advisory comparison. None when the model has no
    /// MTOM (or the aircraft has no model).
    pub mtom_kg: Option<f32>,
    /// Explicit incompleteness accounting for the UI.
    pub empty_weight_missing: bool,
    pub batteries_missing_weight: i64,
    pub accessories_missing_weight: i64,
}

/// Computes the current-configuration takeoff mass from the already-fetched
/// mounted equipment. Controllers are excluded: they are ground equipment.
/// Zero mounted items of a kind contributes 0.0 (an accurate configuration,
/// not missing data). A mounted item WITHOUT a weight makes that component —
/// and therefore the total — None: an incomplete mass is flagged, never
/// silently under-reported.
fn compute_takeoff_mass(
    aircraft_empty_kg: Option<f32>,
    model_empty_kg: Option<f32>,
    model_mtom_kg: Option<f32>,
    batteries: &[MountedBattery],
    accessories: &[MountedAccessory],
) -> TakeoffMass {
    let (empty_weight_kg, empty_weight_source) = match (aircraft_empty_kg, model_empty_kg) {
        (Some(w), _) => (Some(w), Some("aircraft".to_string())),
        (None, Some(w)) => (Some(w), Some("model".to_string())),
        (None, None) => (None, None),
    };
    let batteries_missing_weight =
        batteries.iter().filter(|b| b.weight_kg.is_none()).count() as i64;
    let accessories_missing_weight =
        accessories.iter().filter(|a| a.weight_kg.is_none()).count() as i64;
    let batteries_kg = (batteries_missing_weight == 0)
        .then(|| batteries.iter().filter_map(|b| b.weight_kg).sum::<f32>());
    let accessories_kg = (accessories_missing_weight == 0)
        .then(|| accessories.iter().filter_map(|a| a.weight_kg).sum::<f32>());
    let total_kg = match (empty_weight_kg, batteries_kg, accessories_kg) {
        (Some(e), Some(b), Some(a)) => Some(e + b + a),
        _ => None,
    };
    TakeoffMass {
        empty_weight_kg,
        empty_weight_source,
        batteries_kg,
        accessories_kg,
        total_kg,
        mtom_kg: model_mtom_kg,
        empty_weight_missing: empty_weight_kg.is_none(),
        batteries_missing_weight,
        accessories_missing_weight,
    }
}

pub async fn get_aircraft_detail(
    claims: Claims,
    State(pool): State<DbPool>,
    Path(aircraft_id): Path<i32>,
) -> Result<Json<AircraftDetailResponse>, AppError> {
    // Aircraft header — LEFT JOIN aircraft_models so model is NULL when
    // model_id is unset. Same pattern as the canonical AIRCRAFT_SELECT.
    #[derive(sqlx::FromRow)]
    struct HeaderRow {
        id: i32,
        equipment_id: Option<String>,
        serial_number: String,
        nickname: Option<String>,
        status: String,
        registered_at: DateTime<Utc>,
        retirement_notes: Option<String>,
        needs_manual_sn: bool,
        empty_weight_kg: Option<f32>,
        model_id: Option<i32>,
        model_name: Option<String>,
        manufacturer: Option<String>,
        model_mtom_kg: Option<f32>,
        model_empty_weight_kg: Option<f32>,
    }
    let header = sqlx::query_as::<_, HeaderRow>(
        "SELECT a.id, a.equipment_id, a.serial_number, a.nickname, a.status, \
                a.created_at AS registered_at, a.retirement_notes, a.needs_manual_sn, \
                a.empty_weight_kg, \
                am.id AS model_id, am.name AS model_name, am.manufacturer, \
                am.mtom_kg AS model_mtom_kg, am.empty_weight_kg AS model_empty_weight_kg \
         FROM aircraft a \
         LEFT JOIN aircraft_models am ON a.model_id = am.id \
         WHERE a.id = $1 AND a.org_id = $2",
    )
    .bind(aircraft_id)
    .bind(claims.org_id)
    .fetch_optional(&pool)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Aircraft {} not found", aircraft_id)))?;

    let model = match (header.model_id, header.model_name) {
        (Some(id), Some(name)) => Some(ModelInfo {
            id,
            name,
            manufacturer: header.manufacturer,
        }),
        _ => None,
    };

    let aircraft = AircraftInfo {
        id: header.id,
        equipment_id: header.equipment_id,
        serial_number: header.serial_number,
        nickname: header.nickname,
        model,
        status: header.status,
        registered_at: header.registered_at,
        retirement_notes: header.retirement_notes,
        needs_manual_sn: header.needs_manual_sn,
        empty_weight_kg: header.empty_weight_kg,
    };

    // Window aggregates — 30d for the "last 30 days" card.
    let (totals, windows) =
        agg::flight_window_aggregates(&pool, aircraft_id, claims.org_id, &[30]).await?;
    let w30 = windows[0];

    let last_flight: Option<DateTime<Utc>> = sqlx::query_scalar(
        "SELECT MAX(flight_start) \
         FROM flights \
         WHERE aircraft_id = $1 AND org_id = $2 AND deleted_at IS NULL",
    )
    .bind(aircraft_id)
    .bind(claims.org_id)
    .fetch_optional(&pool)
    .await?
    .flatten();

    let total_distance_m = agg::total_distance_m(&pool, aircraft_id, claims.org_id).await?;

    let stats = AircraftStats {
        total_flights: totals.count,
        total_seconds: totals.seconds,
        total_distance_m,
        last_flight,
        flights_last_30d: w30.count,
        seconds_last_30d: w30.seconds,
    };

    let accessories = sqlx::query_as::<_, MountedAccessory>(
        "SELECT id, name, category, serial_number, equipment_id, weight_kg, status \
         FROM accessories WHERE aircraft_id = $1 AND org_id = $2 AND status != 'retired' \
         ORDER BY COALESCE(equipment_id, name)",
    )
    .bind(aircraft_id)
    .bind(claims.org_id)
    .fetch_all(&pool)
    .await?;

    let batteries = sqlx::query_as::<_, MountedBattery>(
        "SELECT id, equipment_id, serial_number, compatible_model, weight_kg, status \
         FROM batteries WHERE aircraft_id = $1 AND org_id = $2 AND status != 'retired' \
         ORDER BY COALESCE(equipment_id, serial_number)",
    )
    .bind(aircraft_id)
    .bind(claims.org_id)
    .fetch_all(&pool)
    .await?;

    let controllers = sqlx::query_as::<_, MountedController>(
        "SELECT id, equipment_id, serial_number, model, nickname, status \
         FROM remote_controllers WHERE aircraft_id = $1 AND org_id = $2 AND status != 'retired' \
         ORDER BY COALESCE(nickname, serial_number)",
    )
    .bind(aircraft_id)
    .bind(claims.org_id)
    .fetch_all(&pool)
    .await?;

    let takeoff_mass = compute_takeoff_mass(
        header.empty_weight_kg,
        header.model_empty_weight_kg,
        header.model_mtom_kg,
        &batteries,
        &accessories,
    );

    Ok(Json(AircraftDetailResponse {
        aircraft,
        stats,
        mounted: MountedEquipment {
            accessories,
            batteries,
            controllers,
        },
        takeoff_mass,
    }))
}

// =====================================================================
// GET /api/v1/aircraft/:id/breakdowns — categorical breakdowns scoped
// to a single airframe. Mirrors the four-section panel on the pilot
// detail page (minus by_aircraft_model, which is meaningless here, and
// minus by_pilot, which is out of scope per the roadmap).
//
// Day/night classification runs in Rust over each flight using
// `services::sun_position`. Only flights with a valid `flight_start`
// AND `home_lat`/`home_lon` are classified — flights missing either
// land in the `Unknown` bucket.
// =====================================================================

#[derive(Debug, Serialize)]
pub struct AircraftBreakdownsResponse {
    pub by_easa_category: Vec<CategoryStats>,
    pub by_mission_type: Vec<CategoryStats>,
    pub by_visual_regime: Vec<CategoryStats>,
    pub by_daylight: Vec<CategoryStats>,
}

#[derive(Debug, Serialize)]
pub struct CategoryStats {
    pub label: String,
    pub flights: i64,
    pub total_seconds: i64,
}

pub async fn get_aircraft_breakdowns(
    claims: Claims,
    State(pool): State<DbPool>,
    Path(aircraft_id): Path<i32>,
) -> Result<Json<AircraftBreakdownsResponse>, AppError> {
    // Existence + org-scope check — same shape as get_aircraft_detail.
    // 404 (not 403) on cross-org IDs so existence doesn't leak.
    let exists: Option<(i32,)> = sqlx::query_as(
        "SELECT id FROM aircraft WHERE id = $1 AND org_id = $2",
    )
    .bind(aircraft_id)
    .bind(claims.org_id)
    .fetch_optional(&pool)
    .await?;
    if exists.is_none() {
        return Err(AppError::NotFound(format!(
            "Aircraft {} not found",
            aircraft_id
        )));
    }

    let by_easa_category: Vec<CategoryStats> =
        agg::easa_category_breakdown(&pool, aircraft_id, claims.org_id)
            .await?
            .into_iter()
            .map(|b| CategoryStats {
                label: b.label,
                flights: b.flights,
                total_seconds: b.total_seconds,
            })
            .collect();

    let by_mission_type: Vec<CategoryStats> =
        agg::mission_type_breakdown(&pool, aircraft_id, claims.org_id)
            .await?
            .into_iter()
            .map(|b| CategoryStats {
                label: b.label,
                flights: b.flights,
                total_seconds: b.total_seconds,
            })
            .collect();

    let by_visual_regime: Vec<CategoryStats> =
        agg::visual_regime_breakdown(&pool, aircraft_id, claims.org_id)
            .await?
            .into_iter()
            .map(|b| CategoryStats {
                label: b.label,
                flights: b.flights,
                total_seconds: b.total_seconds,
            })
            .collect();

    // Day/night — same shape as pilot_detail. Cast f32 → f64 because the
    // DB stores REAL but sun-times wants f64.
    #[derive(sqlx::FromRow)]
    struct FlightRow {
        flight_start: Option<DateTime<Utc>>,
        duration_seconds: i32,
        home_lat: Option<f32>,
        home_lon: Option<f32>,
    }
    let flights_raw: Vec<FlightRow> = sqlx::query_as::<_, FlightRow>(
        "SELECT flight_start, duration_seconds, home_lat, home_lon \
         FROM flights \
         WHERE aircraft_id = $1 AND org_id = $2 AND deleted_at IS NULL",
    )
    .bind(aircraft_id)
    .bind(claims.org_id)
    .fetch_all(&pool)
    .await?;

    let mut day_secs = 0i64;
    let mut night_secs = 0i64;
    let mut day_flights = 0i64;
    let mut night_flights = 0i64;
    let mut mixed_flights = 0i64;
    let mut unknown_flights = 0i64;
    let mut mixed_secs = 0i64;
    let mut unknown_secs = 0i64;

    for f in &flights_raw {
        let lat = f.home_lat.map(|v| v as f64);
        let lon = f.home_lon.map(|v| v as f64);
        let cls = classify_flight(f.flight_start, f.duration_seconds, lat, lon);
        let (d, n) =
            split_duration_by_daylight(f.flight_start, f.duration_seconds, lat, lon);
        match cls {
            DayNight::Day => {
                day_flights += 1;
                day_secs += d as i64;
            }
            DayNight::Night => {
                night_flights += 1;
                night_secs += n as i64;
            }
            DayNight::Mixed => {
                // For Mixed flights, split-attribute the time into
                // day_secs / night_secs so total_seconds across the
                // four buckets equals the aircraft's total flight time.
                mixed_flights += 1;
                mixed_secs += f.duration_seconds as i64;
                day_secs += d as i64;
                night_secs += n as i64;
            }
            DayNight::Unknown => {
                unknown_flights += 1;
                unknown_secs += f.duration_seconds as i64;
            }
        }
    }

    let mut by_daylight: Vec<CategoryStats> = vec![
        CategoryStats {
            label: "Day".into(),
            flights: day_flights,
            total_seconds: day_secs,
        },
        CategoryStats {
            label: "Night".into(),
            flights: night_flights,
            total_seconds: night_secs,
        },
        CategoryStats {
            label: "Mixed".into(),
            flights: mixed_flights,
            total_seconds: mixed_secs,
        },
        CategoryStats {
            label: "Unknown".into(),
            flights: unknown_flights,
            total_seconds: unknown_secs,
        },
    ];
    by_daylight.retain(|c| c.flights > 0);

    Ok(Json(AircraftBreakdownsResponse {
        by_easa_category,
        by_mission_type,
        by_visual_regime,
        by_daylight,
    }))
}
