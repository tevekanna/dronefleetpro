// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

use axum::{extract::State, Json};
use serde::Serialize;
use chrono::{DateTime, Utc};

use crate::auth::middleware::Claims;
use crate::db::DbPool;
use crate::errors::AppError;

#[derive(Debug, Serialize)]
pub struct DashboardStats {
    pub total_flights: i64,
    pub total_seconds: i64,
    pub total_distance_km: f64,
    pub active_aircraft: i64,
    pub open_defects: i64,
    pub airworthy_aircraft: i64,
    pub not_airworthy_aircraft: i64,
    pub total_pilots: i64,
    pub total_aircraft_models: i64,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct RecentFlight {
    pub id: i32,
    pub flight_datetime: Option<DateTime<Utc>>,
    pub aircraft_name: Option<String>,
    pub equipment_id: Option<String>,
    pub duration_seconds: i32,
    pub max_altitude_m: f32,
    pub easa_category: Option<String>,
    pub mission_type: Option<String>,
    pub log_source: Option<String>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct FlightMapPoint {
    pub id: i32,
    pub lat: f64,
    pub lon: f64,
    pub flight_datetime: Option<DateTime<Utc>>,
    pub aircraft_name: Option<String>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct FleetStatusItem {
    pub id: i32,
    pub equipment_id: Option<String>,
    pub nickname: Option<String>,
    pub model: Option<String>,
    pub status: String,
    pub flight_count: i64,
    pub total_seconds: i64,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct ExpiringLicense {
    pub id: i32,
    pub name: String,
    pub license_type: Option<String>,
    pub license_expires: Option<chrono::NaiveDate>,
    pub days_until_expiry: Option<i64>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct OpenDefect {
    pub id: i32,
    pub aircraft_name: Option<String>,
    pub description: String,
    pub defect_date: chrono::NaiveDate,
    pub reported_by: String,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct RecentMaintenance {
    pub id: i32,
    pub aircraft_name: Option<String>,
    pub performed_date: chrono::NaiveDate,
    pub maint_category: Option<String>,
    pub maint_type: Option<String>,
    pub remark: Option<String>,
    pub sign_off: String,
}

#[derive(Debug, Serialize)]
pub struct DashboardData {
    pub stats: DashboardStats,
    pub recent_flights: Vec<RecentFlight>,
    pub fleet_status: Vec<FleetStatusItem>,
    pub expiring_licenses: Vec<ExpiringLicense>,
    pub open_defects: Vec<OpenDefect>,
    pub recent_maintenance: Vec<RecentMaintenance>,
}

/// GET /api/v1/dashboard — All dashboard data in one request.
pub async fn get_dashboard(
    claims: Claims,
    State(pool): State<DbPool>,
) -> Result<Json<DashboardData>, AppError> {
    let org_id = claims.org_id;

    // Stats
    let stats_row = sqlx::query(
        "SELECT
            COUNT(*)::bigint as total_flights,
            COALESCE(SUM(duration_seconds), 0)::bigint as total_seconds,
            COALESCE(SUM(total_distance_m), 0)::float8 / 1000.0 as total_distance_km,
            (SELECT COUNT(*)::bigint FROM aircraft WHERE org_id = $1 AND status = 'active') as active_aircraft,
            (SELECT COUNT(*)::bigint FROM defect_log WHERE org_id = $1 AND status = 'open') as open_defects,
            (SELECT COUNT(*)::bigint FROM aircraft WHERE org_id = $1 AND status = 'active' AND airworthiness_status = 'airworthy') as airworthy_aircraft,
            (SELECT COUNT(*)::bigint FROM aircraft WHERE org_id = $1 AND status = 'active' AND airworthiness_status = 'not_airworthy') as not_airworthy_aircraft,
            (SELECT COUNT(*)::bigint FROM pilots WHERE org_id = $1 AND is_active = true) as total_pilots,
            (SELECT COUNT(*)::bigint FROM aircraft_models WHERE org_id = $1) as total_aircraft_models
         FROM flights WHERE org_id = $1 AND deleted_at IS NULL"
    )
    .bind(org_id)
    .fetch_one(&pool)
    .await?;

    use sqlx::Row;
    let stats = DashboardStats {
        total_flights: stats_row.get("total_flights"),
        total_seconds: stats_row.get("total_seconds"),
        total_distance_km: stats_row.get("total_distance_km"),
        active_aircraft: stats_row.get("active_aircraft"),
        open_defects: stats_row.get("open_defects"),
        airworthy_aircraft: stats_row.get("airworthy_aircraft"),
        not_airworthy_aircraft: stats_row.get("not_airworthy_aircraft"),
        total_pilots: stats_row.get("total_pilots"),
        total_aircraft_models: stats_row.get("total_aircraft_models"),
    };

    // Recent flights (last 5)
    let recent_flights = sqlx::query_as::<_, RecentFlight>(
        "SELECT f.id, f.flight_datetime,
                COALESCE(f.aircraft_name, a.nickname, am.name) as aircraft_name,
                a.equipment_id as equipment_id,
                f.duration_seconds, f.max_altitude_m, f.easa_category,
                f.mission_type, f.log_source
         FROM flights f
         LEFT JOIN aircraft a ON f.aircraft_id = a.id
         LEFT JOIN aircraft_models am ON a.model_id = am.id
         WHERE f.org_id = $1 AND f.deleted_at IS NULL
         ORDER BY f.flight_datetime DESC NULLS LAST
         LIMIT 5"
    )
    .bind(org_id)
    .fetch_all(&pool)
    .await?;

    // Fleet status
    let fleet_status = sqlx::query_as::<_, FleetStatusItem>(
        "SELECT a.id, a.equipment_id, a.nickname, am.name AS model, a.status,
                COUNT(f.id)::bigint as flight_count,
                COALESCE(SUM(f.duration_seconds), 0)::bigint as total_seconds
         FROM aircraft a
         LEFT JOIN aircraft_models am ON a.model_id = am.id
         LEFT JOIN flights f ON a.id = f.aircraft_id AND f.deleted_at IS NULL
         WHERE a.org_id = $1
         GROUP BY a.id, am.name
         ORDER BY a.status, a.nickname NULLS LAST"
    )
    .bind(org_id)
    .fetch_all(&pool)
    .await?;

    // Expiring licenses (within 90 days or already expired)
    let expiring_licenses = sqlx::query_as::<_, ExpiringLicense>(
        "SELECT id, name, license_type, license_expires,
                (license_expires - CURRENT_DATE)::bigint as days_until_expiry
         FROM pilots
         WHERE org_id = $1
           AND is_active = true
           AND license_expires IS NOT NULL
           AND license_expires <= CURRENT_DATE + INTERVAL '90 days'
         ORDER BY license_expires ASC"
    )
    .bind(org_id)
    .fetch_all(&pool)
    .await?;

    // Open defects
    let open_defects = sqlx::query_as::<_, OpenDefect>(
        "SELECT d.id, COALESCE(a.nickname, am.name, a.serial_number) as aircraft_name,
                d.description, d.defect_date, d.reported_by
         FROM defect_log d
         LEFT JOIN aircraft a ON d.aircraft_id = a.id
         LEFT JOIN aircraft_models am ON a.model_id = am.id
         WHERE d.org_id = $1 AND d.status = 'open'
         ORDER BY d.defect_date DESC
         LIMIT 10"
    )
    .bind(org_id)
    .fetch_all(&pool)
    .await?;

    // Recent maintenance (last 5)
    let recent_maintenance = sqlx::query_as::<_, RecentMaintenance>(
        "SELECT m.id, COALESCE(a.nickname, am.name, a.serial_number) as aircraft_name,
                m.performed_date, m.maint_category, m.maint_type, m.remark, m.sign_off
         FROM maintenance m
         LEFT JOIN aircraft a ON m.aircraft_id = a.id
         LEFT JOIN aircraft_models am ON a.model_id = am.id
         WHERE m.org_id = $1
         ORDER BY m.performed_date DESC, m.created_at DESC
         LIMIT 5"
    )
    .bind(org_id)
    .fetch_all(&pool)
    .await?;

    Ok(Json(DashboardData {
        stats,
        recent_flights,
        fleet_status,
        expiring_licenses,
        open_defects,
        recent_maintenance,
    }))
}

/// GET /api/v1/dashboard/flight-map — a lightweight list of every flight's
/// representative take-off coordinate, for the dashboard overview map.
/// Only flights with a recorded home position are returned; manual or
/// GPS-less flights are omitted (nothing to plot). Org-scoped, soft-delete
/// excluded. trajectory_json is intentionally NOT touched (too heavy).
pub async fn get_flight_map(
    claims: Claims,
    State(pool): State<DbPool>,
) -> Result<Json<Vec<FlightMapPoint>>, AppError> {
    let points = sqlx::query_as::<_, FlightMapPoint>(
        "SELECT f.id,
                f.home_lat::float8 AS lat,
                f.home_lon::float8 AS lon,
                f.flight_datetime,
                COALESCE(f.aircraft_name, a.nickname, am.name) AS aircraft_name
         FROM flights f
         LEFT JOIN aircraft a ON f.aircraft_id = a.id
         LEFT JOIN aircraft_models am ON a.model_id = am.id
         WHERE f.org_id = $1
           AND f.deleted_at IS NULL
           AND f.home_lat IS NOT NULL
           AND f.home_lon IS NOT NULL
         ORDER BY f.flight_datetime DESC NULLS LAST",
    )
    .bind(claims.org_id)
    .fetch_all(&pool)
    .await?;

    Ok(Json(points))
}
