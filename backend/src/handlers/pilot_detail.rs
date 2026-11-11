// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

//! GET /api/v1/pilots/:id/detail — full data block for the pilot detail page.
//!
//! Read-only, org-scoped. Pilot-role users can only see their own detail
//! (matching `pilot_stats`). Other roles can see any pilot in their org.
//!
//! All aggregates come from `services::pilot_aggregates` so this endpoint
//! and `/pilots/:id/stats` cannot drift on what "a flight" means.
//!
//! `/detail` uses 90d / 12mo recency windows (EASA recency for Specific
//! operations is 90 days). `/stats` keeps its 30d / 365d windows.
//!
//! Day/night classification runs in Rust over each non-soft-deleted
//! flight using `services::sun_position`. Only flights with a valid
//! `flight_start` AND `home_lat`/`home_lon` are classified — flights
//! missing either land in the `Unknown` bucket.
//!
//! The full flight log is NOT embedded in this response — the page
//! consumes the existing `GET /flights?pilot_id=X` endpoint with its
//! own pagination so we don't duplicate that machinery here.

use axum::{
    extract::{Path, State},
    Json,
};
use chrono::{DateTime, NaiveDate, Utc};
use serde::Serialize;

use crate::auth::middleware::Claims;
use crate::db::DbPool;
use crate::errors::AppError;
use crate::services::pilot_aggregates as agg;
use crate::services::sun_position::{
    classify_flight, split_duration_by_daylight, DayNight,
};

#[derive(Debug, Serialize)]
pub struct PilotDetailResponse {
    pub pilot: PilotHeader,
    pub stats: PilotStatsBlock,
    pub qualifications: Vec<QualificationItem>,
    pub breakdowns: PilotBreakdowns,
    pub recent_incidents: Vec<IncidentSummary>,
}

#[derive(Debug, Serialize)]
pub struct PilotHeader {
    pub id: i32,
    pub name: String,
    pub email: Option<String>,
    pub phone: Option<String>,
    /// 3-letter user code from the linked user account, if any. NULL when
    /// the pilot has no associated login user.
    pub user_code: Option<String>,
    pub license_type: Option<String>,
    pub license_number: Option<String>,
    pub license_expires: Option<NaiveDate>,
    pub is_active: bool,
    pub notes: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct PilotStatsBlock {
    pub total_flights: i64,
    pub total_seconds: i64,
    pub last_flight: Option<DateTime<Utc>>,
    pub last_90_days_flights: i64,
    pub last_90_days_seconds: i64,
    pub last_12_months_flights: i64,
    pub last_12_months_seconds: i64,
}

#[derive(Debug, Serialize)]
pub struct QualificationItem {
    /// pilot_qualifications.id when sourced from that table; NULL for the
    /// synthesised primary-license row drawn from pilots.license_type.
    pub id: Option<i32>,
    pub type_name: String,
    pub issued_date: Option<NaiveDate>,
    pub expiry_date: Option<NaiveDate>,
    pub status: &'static str,
    pub days_until_expiry: Option<i32>,
    pub notes: Option<String>,
    pub certificate_ref: Option<String>,
    pub is_primary_license: bool,
}

#[derive(Debug, Serialize)]
pub struct PilotBreakdowns {
    pub by_easa_category: Vec<CategoryStats>,
    pub by_visual_regime: Vec<CategoryStats>,
    pub by_daylight: Vec<CategoryStats>,
    pub by_aircraft_model: Vec<CategoryStats>,
    pub by_mission_type: Vec<CategoryStats>,
}

#[derive(Debug, Serialize)]
pub struct CategoryStats {
    pub label: String,
    pub flights: i64,
    pub total_seconds: i64,
}

#[derive(Debug, Serialize)]
pub struct IncidentSummary {
    pub id: i32,
    pub occurred_at: DateTime<Utc>,
    pub category: String,
    pub severity: String,
    pub status: String,
    /// First 100 chars of the incident description.
    pub description_preview: String,
}

pub async fn get_pilot_detail(
    claims: Claims,
    State(pool): State<DbPool>,
    Path(pilot_id): Path<i32>,
) -> Result<Json<PilotDetailResponse>, AppError> {
    // RBAC: pilot-role users can only view their own detail.
    if claims.role == "pilot" {
        let own_pilot_id =
            crate::services::pilot_lookup::resolve_pilot_id_for_self(&pool, &claims).await?;
        if own_pilot_id != pilot_id {
            return Err(AppError::Forbidden(
                "Pilots can only view their own detail page".to_string(),
            ));
        }
    }

    // Pilot header — LEFT JOIN on users so user_code is NULL when the
    // pilot is not linked to a login account. Multiple users could in
    // theory share a pilot_id; we take the first by user.id as a simple
    // tie-break — the linkage is intended to be 1:1 by convention.
    #[derive(sqlx::FromRow)]
    struct HeaderRow {
        id: i32,
        name: String,
        email: Option<String>,
        phone: Option<String>,
        user_code: Option<String>,
        license_type: Option<String>,
        license_number: Option<String>,
        license_expires: Option<NaiveDate>,
        is_active: bool,
        notes: Option<String>,
    }
    let header = sqlx::query_as::<_, HeaderRow>(
        "SELECT p.id, p.name, p.email, p.phone, \
                u.user_code, \
                p.license_type, p.license_number, p.license_expires, \
                p.is_active, p.notes \
         FROM pilots p \
         LEFT JOIN LATERAL ( \
           SELECT user_code FROM users \
           WHERE pilot_id = p.id AND org_id = p.org_id \
           ORDER BY id LIMIT 1 \
         ) u ON true \
         WHERE p.id = $1 AND p.org_id = $2",
    )
    .bind(pilot_id)
    .bind(claims.org_id)
    .fetch_optional(&pool)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Pilot {} not found", pilot_id)))?;

    let pilot = PilotHeader {
        id: header.id,
        name: header.name,
        email: header.email,
        phone: header.phone,
        user_code: header.user_code,
        license_type: header.license_type,
        license_number: header.license_number,
        license_expires: header.license_expires,
        is_active: header.is_active,
        notes: header.notes,
    };

    // Window aggregates — 90d (EASA recency) + 365d (rolling year).
    let (totals, windows) =
        agg::flight_window_aggregates(&pool, pilot_id, claims.org_id, &[90, 365]).await?;
    let w90 = windows[0];
    let w365 = windows[1];

    // Last flight time, separate single query.
    let last_flight: Option<DateTime<Utc>> = sqlx::query_scalar(
        "SELECT MAX(flight_start) \
         FROM flights \
         WHERE pilot_id = $1 AND org_id = $2 AND deleted_at IS NULL",
    )
    .bind(pilot_id)
    .bind(claims.org_id)
    .fetch_optional(&pool)
    .await?
    .flatten();

    let stats = PilotStatsBlock {
        total_flights: totals.count,
        total_seconds: totals.seconds,
        last_flight,
        last_90_days_flights: w90.count,
        last_90_days_seconds: w90.seconds,
        last_12_months_flights: w365.count,
        last_12_months_seconds: w365.seconds,
    };

    // Qualifications: shared helper handles primary license + endorsements
    // and computes status. Map directly to public shape.
    let today = Utc::now().date_naive();
    let qualifications: Vec<QualificationItem> =
        agg::qualifications_with_status(&pool, pilot_id, claims.org_id, today)
            .await?
            .into_iter()
            .map(|q| QualificationItem {
                id: q.id,
                type_name: q.type_name,
                issued_date: q.issued_date,
                expiry_date: q.expiry_date,
                status: q.status,
                days_until_expiry: q.days_until_expiry,
                notes: q.notes,
                certificate_ref: q.certificate_ref,
                is_primary_license: q.is_primary_license,
            })
            .collect();

    // Breakdowns: SQL aggregates for everything except day/night.
    let by_easa_category: Vec<CategoryStats> =
        agg::easa_category_breakdown(&pool, pilot_id, claims.org_id)
            .await?
            .into_iter()
            .map(|b| CategoryStats {
                label: b.label,
                flights: b.flights,
                total_seconds: b.total_seconds,
            })
            .collect();

    let by_visual_regime: Vec<CategoryStats> =
        agg::visual_regime_breakdown(&pool, pilot_id, claims.org_id)
            .await?
            .into_iter()
            .map(|b| CategoryStats {
                label: b.label,
                flights: b.flights,
                total_seconds: b.total_seconds,
            })
            .collect();

    let by_aircraft_model: Vec<CategoryStats> =
        agg::model_breakdown(&pool, pilot_id, claims.org_id, None)
            .await?
            .into_iter()
            .map(|b| CategoryStats {
                label: b.label,
                flights: b.flights,
                total_seconds: b.total_seconds,
            })
            .collect();

    let by_mission_type: Vec<CategoryStats> =
        agg::mission_type_breakdown(&pool, pilot_id, claims.org_id)
            .await?
            .into_iter()
            .map(|b| CategoryStats {
                label: b.label,
                flights: b.flights,
                total_seconds: b.total_seconds,
            })
            .collect();

    // Day/night classification: runs in Rust because sunrise/sunset depends
    // on home_lat/lon + date, not just SQL state. Costs one extra scan of
    // the pilot's flights — fine for per-pilot detail; would be expensive
    // for fleet-wide reports.
    #[derive(sqlx::FromRow)]
    struct FlightRow {
        flight_start: Option<DateTime<Utc>>,
        duration_seconds: i32,
        home_lat: Option<f64>,
        home_lon: Option<f64>,
    }
    // sun-times wants f64 lat/lon but our DB stores REAL (f32). Fetch the
    // raw f32 values and cast.
    #[derive(sqlx::FromRow)]
    struct FlightRowReal {
        flight_start: Option<DateTime<Utc>>,
        duration_seconds: i32,
        home_lat: Option<f32>,
        home_lon: Option<f32>,
    }
    let flights_raw: Vec<FlightRow> = sqlx::query_as::<_, FlightRowReal>(
        "SELECT flight_start, duration_seconds, home_lat, home_lon \
         FROM flights \
         WHERE pilot_id = $1 AND org_id = $2 AND deleted_at IS NULL",
    )
    .bind(pilot_id)
    .bind(claims.org_id)
    .fetch_all(&pool)
    .await?
    .into_iter()
    .map(|r| FlightRow {
        flight_start: r.flight_start,
        duration_seconds: r.duration_seconds,
        home_lat: r.home_lat.map(|v| v as f64),
        home_lon: r.home_lon.map(|v| v as f64),
    })
    .collect();

    let mut day_secs = 0i64;
    let mut night_secs = 0i64;
    let mut day_flights = 0i64;
    let mut night_flights = 0i64;
    let mut mixed_flights = 0i64;
    let mut unknown_flights = 0i64;
    let mut mixed_secs = 0i64;
    let mut unknown_secs = 0i64;

    for f in &flights_raw {
        let cls = classify_flight(f.flight_start, f.duration_seconds, f.home_lat, f.home_lon);
        let (d, n) =
            split_duration_by_daylight(f.flight_start, f.duration_seconds, f.home_lat, f.home_lon);
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
                // For Mixed flights we also split-attribute the time into
                // day_secs / night_secs so total_seconds across the four
                // labels equals the pilot's total flight time.
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

    // Recent incidents (5 most recent by occurred_at).
    #[derive(sqlx::FromRow)]
    struct IncidentRow {
        id: i32,
        occurred_at: DateTime<Utc>,
        category: String,
        severity: String,
        status: String,
        description_preview: String,
    }
    let recent_incidents: Vec<IncidentSummary> = sqlx::query_as::<_, IncidentRow>(
        "SELECT id, occurred_at, category, severity, status, \
                LEFT(description, 100) AS description_preview \
         FROM incidents \
         WHERE pilot_id = $1 AND org_id = $2 \
         ORDER BY occurred_at DESC \
         LIMIT 5",
    )
    .bind(pilot_id)
    .bind(claims.org_id)
    .fetch_all(&pool)
    .await?
    .into_iter()
    .map(|r| IncidentSummary {
        id: r.id,
        occurred_at: r.occurred_at,
        category: r.category,
        severity: r.severity,
        status: r.status,
        description_preview: r.description_preview,
    })
    .collect();

    Ok(Json(PilotDetailResponse {
        pilot,
        stats,
        qualifications,
        breakdowns: PilotBreakdowns {
            by_easa_category,
            by_visual_regime,
            by_daylight,
            by_aircraft_model,
            by_mission_type,
        },
        recent_incidents,
    }))
}
