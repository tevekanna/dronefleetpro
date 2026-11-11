// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

use axum::{
    extract::{Path, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use chrono::Utc;

use crate::auth::middleware::Claims;
use crate::db::DbPool;
use crate::errors::AppError;
use crate::models::fleet_stats::{
    BatteryModelComparison, BatteryStatsDetail, ControllerStats, FleetStatsOverview,
};
use crate::models::fleet_stats_dashboard::{
    AircraftByModelResponse, BatteryCycleDistribution, FleetDashboard,
};
use crate::services::fleet_stats;
use crate::services::fleet_stats_dashboard;

/// GET /api/v1/fleet-stats/overview
pub async fn overview(
    claims: Claims,
    State(pool): State<DbPool>,
) -> Result<Json<FleetStatsOverview>, AppError> {
    let overview = fleet_stats::get_fleet_overview(&pool, claims.org_id).await?;
    Ok(Json(overview))
}

/// GET /api/v1/fleet-stats/dashboard
/// Returns the 5 KPIs + 3 widget arrays for the redesigned Fleet
/// Statistics page (KPI strip + Översikt tab content in one round-trip).
pub async fn fleet_dashboard(
    claims: Claims,
    State(pool): State<DbPool>,
) -> Result<Json<FleetDashboard>, AppError> {
    let data = fleet_stats_dashboard::get_fleet_dashboard(&pool, claims.org_id).await?;
    Ok(Json(data))
}

/// GET /api/v1/fleet-stats/aircraft-by-model
/// Per-model aggregation (counts, hours, flights, open defects) plus a
/// longer "top aircraft by hours" list for the Aircraft tab.
pub async fn aircraft_by_model(
    claims: Claims,
    State(pool): State<DbPool>,
) -> Result<Json<AircraftByModelResponse>, AppError> {
    let data = fleet_stats_dashboard::get_aircraft_by_model(&pool, claims.org_id).await?;
    Ok(Json(data))
}

/// GET /api/v1/fleet-stats/battery-cycle-distribution
/// Histogram bucketing of active batteries by cycle-count percentage.
pub async fn battery_cycle_distribution(
    claims: Claims,
    State(pool): State<DbPool>,
) -> Result<Json<BatteryCycleDistribution>, AppError> {
    let data =
        fleet_stats_dashboard::get_battery_cycle_distribution(&pool, claims.org_id).await?;
    Ok(Json(data))
}

/// GET /api/v1/fleet-stats/batteries/:id
pub async fn battery_detail(
    claims: Claims,
    State(pool): State<DbPool>,
    Path(id): Path<i32>,
) -> Result<Json<BatteryStatsDetail>, AppError> {
    let detail = fleet_stats::get_battery_stats_detail(&pool, claims.org_id, id).await?;
    Ok(Json(detail))
}

/// GET /api/v1/fleet-stats/batteries/model/:model
pub async fn battery_model_comparison(
    claims: Claims,
    State(pool): State<DbPool>,
    Path(model): Path<String>,
) -> Result<Json<BatteryModelComparison>, AppError> {
    let cmp = fleet_stats::get_model_comparison(&pool, claims.org_id, &model).await?;
    Ok(Json(cmp))
}

/// GET /api/v1/fleet-stats/controllers/:id
pub async fn controller_detail(
    claims: Claims,
    State(pool): State<DbPool>,
    Path(id): Path<i32>,
) -> Result<Json<ControllerStats>, AppError> {
    let stats = fleet_stats::get_controller_stats(&pool, claims.org_id, id).await?;
    Ok(Json(stats))
}

// =========================================================================
// CSV exports
// =========================================================================

fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn csv_cell<T: std::fmt::Display>(v: Option<T>) -> String {
    match v {
        Some(x) => csv_escape(&x.to_string()),
        None => String::new(),
    }
}

fn csv_bytes(mut csv: String, filename_stem: &str) -> Response {
    // Ensure trailing newline
    if !csv.ends_with('\n') {
        csv.push('\n');
    }
    let date = Utc::now().format("%Y-%m-%d");
    let filename = format!("{filename_stem}-{date}.csv");
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/csv; charset=utf-8"),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&format!("attachment; filename=\"{filename}\""))
            .unwrap_or_else(|_| HeaderValue::from_static("attachment")),
    );
    (StatusCode::OK, headers, csv).into_response()
}

/// GET /api/v1/fleet-stats/export/batteries
pub async fn export_batteries_csv(
    claims: Claims,
    State(pool): State<DbPool>,
) -> Result<Response, AppError> {
    // Fetch all battery ids for org, then join with per-battery stats.
    // Single wide query to avoid N+1.
    let rows = sqlx::query_as::<_, crate::models::fleet_stats::BatteryStats>(
        "SELECT \
            b.id AS battery_id, b.serial_number, b.equipment_id, \
            b.compatible_model AS model, b.status, b.cycle_count, b.max_cycles, \
            s.nominal_max_cycles, s.cycle_uncertainty, \
            COALESCE(agg.total_flights, 0) AS total_flights, \
            COALESCE(agg.total_flight_seconds, 0) AS total_flight_seconds, \
            agg.avg_discharge_per_flight, b.last_used, \
            agg_90d.avg_cell_variance_v, agg_90d.avg_min_temp_c, agg_90d.avg_min_battery_pct \
         FROM batteries b \
         LEFT JOIN battery_model_specs s ON s.model = b.compatible_model \
         LEFT JOIN LATERAL ( \
            SELECT COUNT(*)::bigint AS total_flights, \
                   COALESCE(SUM(duration_seconds), 0)::bigint AS total_flight_seconds, \
                   AVG(CASE WHEN min_battery_pct IS NOT NULL THEN 100 - min_battery_pct END)::real AS avg_discharge_per_flight \
            FROM flights f WHERE f.battery_id = b.id AND f.deleted_at IS NULL \
         ) agg ON TRUE \
         LEFT JOIN LATERAL ( \
            SELECT AVG(cell_variance_v)::real AS avg_cell_variance_v, \
                   AVG(battery_temp_c)::real AS avg_min_temp_c, \
                   AVG(min_battery_pct)::real AS avg_min_battery_pct \
            FROM flights f WHERE f.battery_id = b.id AND f.deleted_at IS NULL \
                 AND f.flight_datetime > NOW() - INTERVAL '90 days' \
         ) agg_90d ON TRUE \
         WHERE b.org_id = $1 \
         ORDER BY b.serial_number",
    )
    .bind(claims.org_id)
    .fetch_all(&pool)
    .await?;

    let mut csv = String::new();
    csv.push_str("battery_id,serial_number,equipment_id,model,status,cycle_count,max_cycles,nominal_max_cycles,cycle_uncertainty,total_flights,total_flight_seconds,avg_discharge_per_flight,last_used,avg_cell_variance_v,avg_min_temp_c,avg_min_battery_pct,estimated_remaining_cycles_mid\n");
    for r in rows {
        let mid = r
            .nominal_max_cycles
            .map(|n| (n - r.cycle_count).max(0))
            .map(|v| v.to_string())
            .unwrap_or_default();
        let line = format!(
            "{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}\n",
            r.battery_id,
            csv_escape(&r.serial_number),
            csv_cell(r.equipment_id.as_deref()),
            csv_cell(r.model.as_deref()),
            csv_escape(&r.status),
            r.cycle_count,
            r.max_cycles,
            csv_cell(r.nominal_max_cycles),
            csv_cell(r.cycle_uncertainty),
            r.total_flights,
            r.total_flight_seconds,
            csv_cell(r.avg_discharge_per_flight),
            csv_cell(r.last_used.map(|d| d.format("%Y-%m-%dT%H:%M:%SZ").to_string())),
            csv_cell(r.avg_cell_variance_v),
            csv_cell(r.avg_min_temp_c),
            csv_cell(r.avg_min_battery_pct),
            mid,
        );
        csv.push_str(&line);
    }
    Ok(csv_bytes(csv, "batteries-stats"))
}

/// GET /api/v1/fleet-stats/controllers/export
pub async fn export_controllers_csv(
    claims: Claims,
    State(pool): State<DbPool>,
) -> Result<Response, AppError> {
    let rows = sqlx::query_as::<_, crate::models::fleet_stats::ControllerStatsSummary>(
        "SELECT \
            c.id AS controller_id, c.serial_number, c.equipment_id, c.model, \
            COALESCE(agg.total_flights, 0) AS total_flights, \
            COALESCE(agg.total_flight_seconds, 0) AS total_flight_seconds, \
            agg.last_used \
         FROM remote_controllers c \
         LEFT JOIN LATERAL ( \
            SELECT COUNT(*)::bigint AS total_flights, \
                   COALESCE(SUM(duration_seconds), 0)::bigint AS total_flight_seconds, \
                   MAX(flight_datetime) AS last_used \
            FROM flights WHERE controller_id = c.id AND deleted_at IS NULL \
         ) agg ON TRUE \
         WHERE c.org_id = $1 \
         ORDER BY c.serial_number",
    )
    .bind(claims.org_id)
    .fetch_all(&pool)
    .await?;

    let mut csv = String::new();
    csv.push_str("controller_id,serial_number,equipment_id,model,total_flights,total_flight_seconds,last_used,days_since_last_use\n");
    let now = Utc::now();
    for r in rows {
        let days_since = r
            .last_used
            .map(|t| (now.signed_duration_since(t).num_days()).to_string())
            .unwrap_or_default();
        let line = format!(
            "{},{},{},{},{},{},{},{}\n",
            r.controller_id,
            csv_escape(&r.serial_number),
            csv_cell(r.equipment_id.as_deref()),
            csv_cell(r.model.as_deref()),
            r.total_flights,
            r.total_flight_seconds,
            csv_cell(r.last_used.map(|d| d.format("%Y-%m-%dT%H:%M:%SZ").to_string())),
            days_since,
        );
        csv.push_str(&line);
    }
    Ok(csv_bytes(csv, "controllers-stats"))
}
