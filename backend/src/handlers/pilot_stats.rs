// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

//! GET /api/v1/pilots/:id/stats — per-pilot activity summary.
//!
//! Read-only, org-scoped. Authenticated-any (matches the rest of
//! /api/v1/pilots/*). Pilot-role users can only see their own stats.
//!
//! All aggregates are produced by `services::pilot_aggregates` so this
//! endpoint and `/pilots/:id/detail` cannot drift on what "a flight"
//! means (soft-delete handling, recency-window definition, NULL
//! treatment in breakdowns).

use axum::{
    extract::{Path, State},
    Json,
};
use chrono::NaiveDate;
use serde::Serialize;

use crate::auth::middleware::Claims;
use crate::db::DbPool;
use crate::errors::AppError;
use crate::services::pilot_aggregates as agg;

#[derive(Debug, Serialize)]
pub struct TopAircraft {
    pub aircraft_id: i32,
    pub equipment_id: Option<String>,
    pub model: Option<String>,
    pub flight_seconds: i64,
    pub flight_count: i64,
}

#[derive(Debug, Serialize)]
pub struct TopModel {
    pub model_id: Option<i32>,
    pub model: Option<String>,
    pub flight_seconds: i64,
    pub flight_count: i64,
}

#[derive(Debug, Serialize)]
pub struct CategoryBreakdown {
    pub category: String,
    pub flight_count: i64,
    pub pct: f64,
}

#[derive(Debug, Serialize)]
pub struct MissionBreakdown {
    pub mission_type: String,
    pub flight_count: i64,
    pub pct: f64,
}

#[derive(Debug, Serialize)]
pub struct QualificationEntry {
    pub qualification_name: String,
    pub expires_on: Option<NaiveDate>,
    pub days_until_expiry: Option<i32>,
    /// One of: "ok" | "expiring_soon" | "expired" | "no_expiry".
    pub status: &'static str,
}

#[derive(Debug, Serialize)]
pub struct PilotStats {
    pub pilot_id: i32,
    pub pilot_name: String,

    pub flight_time_total_seconds: i64,
    pub flight_time_30d_seconds: i64,
    pub flight_time_365d_seconds: i64,

    pub flight_count_total: i64,
    pub flight_count_30d: i64,
    pub flight_count_365d: i64,

    pub top_aircraft: Vec<TopAircraft>,
    pub top_models: Vec<TopModel>,
    /// Percentages are flight-count based, rounded to 1 decimal. Flights
    /// with NULL `easa_category` are omitted from this breakdown so the
    /// percentages sum to 100% across what's present.
    pub easa_categories: Vec<CategoryBreakdown>,
    /// Same NULL handling as `easa_categories`.
    pub mission_types: Vec<MissionBreakdown>,
    /// First entry is the pilot's primary license (from `pilots.license_type`
    /// + `pilots.license_expires`) if set; remaining entries come from
    /// `pilot_qualifications`, soonest-to-expire first (NULL-expiry last).
    pub qualifications: Vec<QualificationEntry>,
}

fn pct_of(count: i64, total: i64) -> f64 {
    if total <= 0 {
        0.0
    } else {
        ((count as f64 / total as f64) * 1000.0).round() / 10.0
    }
}

/// GET /api/v1/pilots/:id/stats
pub async fn get_pilot_stats(
    claims: Claims,
    State(pool): State<DbPool>,
    Path(pilot_id): Path<i32>,
) -> Result<Json<PilotStats>, AppError> {
    // RBAC: pilot-role users can only view their own stats. Other roles
    // (admin, safety_officer, technician, viewer) see any pilot in their org.
    if claims.role == "pilot" {
        let own_pilot_id =
            crate::services::pilot_lookup::resolve_pilot_id_for_self(&pool, &claims).await?;
        if own_pilot_id != pilot_id {
            return Err(AppError::Forbidden(
                "Pilots can only view their own statistics".to_string(),
            ));
        }
    }

    // Pilot existence check — name is the only thing this response needs
    // beyond aggregates.
    let pilot_name: Option<String> =
        sqlx::query_scalar("SELECT name FROM pilots WHERE id = $1 AND org_id = $2")
            .bind(pilot_id)
            .bind(claims.org_id)
            .fetch_optional(&pool)
            .await?;
    let pilot_name = pilot_name.ok_or_else(|| {
        AppError::NotFound(format!("Pilot {} not found", pilot_id))
    })?;

    // Window aggregates: 30d + 365d for /stats. /detail uses 90d + 365d.
    let (totals, windows) =
        agg::flight_window_aggregates(&pool, pilot_id, claims.org_id, &[30, 365]).await?;
    let w30 = windows[0];
    let w365 = windows[1];

    let top_aircraft: Vec<TopAircraft> =
        agg::aircraft_breakdown(&pool, pilot_id, claims.org_id, Some(3))
            .await?
            .into_iter()
            .map(|b| TopAircraft {
                aircraft_id: b.aircraft_id,
                equipment_id: b.equipment_id,
                model: b.model,
                flight_seconds: b.total_seconds,
                flight_count: b.flights,
            })
            .collect();

    let top_models: Vec<TopModel> = agg::model_breakdown(&pool, pilot_id, claims.org_id, Some(3))
        .await?
        .into_iter()
        .map(|b| TopModel {
            model_id: b.model_id,
            // /stats's existing contract uses `model: Option<String>`, so
            // map "Unknown" sentinel back to None for unmodelled rows.
            model: if b.model_id.is_some() {
                Some(b.label)
            } else {
                None
            },
            flight_seconds: b.total_seconds,
            flight_count: b.flights,
        })
        .collect();

    let easa_rows = agg::easa_category_breakdown(&pool, pilot_id, claims.org_id).await?;
    let easa_total: i64 = easa_rows.iter().map(|b| b.flights).sum();
    let easa_categories: Vec<CategoryBreakdown> = easa_rows
        .into_iter()
        .map(|b| CategoryBreakdown {
            pct: pct_of(b.flights, easa_total),
            category: b.label,
            flight_count: b.flights,
        })
        .collect();

    let mission_rows = agg::mission_type_breakdown(&pool, pilot_id, claims.org_id).await?;
    let mission_total: i64 = mission_rows.iter().map(|b| b.flights).sum();
    let mission_types: Vec<MissionBreakdown> = mission_rows
        .into_iter()
        .map(|b| MissionBreakdown {
            pct: pct_of(b.flights, mission_total),
            mission_type: b.label,
            flight_count: b.flights,
        })
        .collect();

    let today = chrono::Utc::now().date_naive();
    let qualifications: Vec<QualificationEntry> =
        agg::qualifications_with_status(&pool, pilot_id, claims.org_id, today)
            .await?
            .into_iter()
            .map(|q| QualificationEntry {
                qualification_name: q.type_name,
                expires_on: q.expiry_date,
                days_until_expiry: q.days_until_expiry,
                status: q.status,
            })
            .collect();

    Ok(Json(PilotStats {
        pilot_id,
        pilot_name,
        flight_time_total_seconds: totals.seconds,
        flight_time_30d_seconds: w30.seconds,
        flight_time_365d_seconds: w365.seconds,
        flight_count_total: totals.count,
        flight_count_30d: w30.count,
        flight_count_365d: w365.count,
        top_aircraft,
        top_models,
        easa_categories,
        mission_types,
        qualifications,
    }))
}
