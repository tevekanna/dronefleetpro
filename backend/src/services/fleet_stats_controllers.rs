// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

use chrono::{DateTime, Utc};
use sqlx::Row;

use crate::db::DbPool;
use crate::errors::AppError;
use crate::models::fleet_stats::{
    BatteryStatsSummary, ControllerAircraftUsage, ControllerStats, ControllerStatsSummary,
    FleetStatsOverview,
};

const INACTIVITY_WARNING_DAYS: i32 = 90;

pub async fn get_controller_stats(
    pool: &DbPool,
    org_id: i32,
    controller_id: i32,
) -> Result<ControllerStats, AppError> {
    let header = sqlx::query(
        "SELECT c.serial_number, c.equipment_id, c.model, c.status, \
                COALESCE(agg.total_flights, 0) AS total_flights, \
                COALESCE(agg.total_flight_seconds, 0) AS total_flight_seconds, \
                agg.first_used, agg.last_used \
         FROM remote_controllers c \
         LEFT JOIN LATERAL ( \
            SELECT COUNT(*)::bigint AS total_flights, \
                   COALESCE(SUM(duration_seconds), 0)::bigint AS total_flight_seconds, \
                   MIN(flight_datetime) AS first_used, \
                   MAX(flight_datetime) AS last_used \
            FROM flights WHERE controller_id = c.id AND deleted_at IS NULL \
         ) agg ON TRUE \
         WHERE c.id = $1 AND c.org_id = $2",
    )
    .bind(controller_id)
    .bind(org_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Controller {controller_id} not found")))?;

    let serial_number: String = header.get("serial_number");
    let equipment_id: Option<String> = header.get("equipment_id");
    let model: Option<String> = header.get("model");
    let status: String = header.get("status");
    let total_flights: i64 = header.get("total_flights");
    let total_flight_seconds: i64 = header.get("total_flight_seconds");
    let first_used: Option<DateTime<Utc>> = header.get("first_used");
    let last_used: Option<DateTime<Utc>> = header.get("last_used");

    let days_since_last_use: Option<i32> = last_used.map(|t| {
        let diff = Utc::now().signed_duration_since(t);
        diff.num_days() as i32
    });

    let usage_by_aircraft = sqlx::query_as::<_, ControllerAircraftUsage>(
        "SELECT \
            a.id AS aircraft_id, \
            a.serial_number AS aircraft_serial, \
            a.equipment_id AS aircraft_equipment_id, \
            am.name AS aircraft_model, \
            COUNT(f.id)::bigint AS flight_count, \
            COALESCE(SUM(f.duration_seconds), 0)::bigint AS flight_seconds, \
            MAX(f.flight_datetime) AS last_flight \
         FROM flights f \
         JOIN aircraft a ON a.id = f.aircraft_id \
         LEFT JOIN aircraft_models am ON a.model_id = am.id \
         WHERE f.controller_id = $1 AND f.org_id = $2 AND f.deleted_at IS NULL \
         GROUP BY a.id, am.name \
         ORDER BY flight_seconds DESC",
    )
    .bind(controller_id)
    .bind(org_id)
    .fetch_all(pool)
    .await?;

    let inactivity_warning = status == "active"
        && days_since_last_use
            .map(|d| d > INACTIVITY_WARNING_DAYS)
            .unwrap_or(false);

    Ok(ControllerStats {
        controller_id,
        serial_number,
        equipment_id,
        model,
        status,
        total_flights,
        total_flight_seconds,
        first_used,
        last_used,
        days_since_last_use,
        usage_by_aircraft,
        inactivity_warning,
    })
}

pub async fn get_fleet_overview(pool: &DbPool, org_id: i32) -> Result<FleetStatsOverview, AppError> {
    let bat_counts = sqlx::query(
        "SELECT \
            COUNT(*) FILTER (WHERE b.status = 'active') AS active, \
            COUNT(*) FILTER ( \
                WHERE b.status = 'active' \
                AND b.cycle_count >= COALESCE(s.nominal_max_cycles, b.max_cycles) * 0.8 \
                AND b.cycle_count <  COALESCE(s.nominal_max_cycles, b.max_cycles) \
            ) AS near_eol, \
            COUNT(*) FILTER ( \
                WHERE b.status = 'active' \
                AND b.cycle_count >= COALESCE(s.nominal_max_cycles, b.max_cycles) \
            ) AS past_eol \
         FROM batteries b \
         LEFT JOIN battery_model_specs s ON s.model = b.compatible_model \
         WHERE b.org_id = $1",
    )
    .bind(org_id)
    .fetch_one(pool)
    .await?;
    let total_batteries_active: i64 = bat_counts.get("active");
    let total_batteries_near_eol: i64 = bat_counts.get("near_eol");
    let total_batteries_past_eol: i64 = bat_counts.get("past_eol");

    let ctl_counts = sqlx::query(
        "SELECT \
            COUNT(*) FILTER (WHERE status = 'active') AS active, \
            COUNT(*) FILTER ( \
                WHERE status = 'active' \
                AND (SELECT MAX(flight_datetime) FROM flights \
                     WHERE controller_id = c.id AND deleted_at IS NULL) \
                    < NOW() - INTERVAL '90 days' \
            ) AS inactive_warning \
         FROM remote_controllers c WHERE org_id = $1",
    )
    .bind(org_id)
    .fetch_one(pool)
    .await?;
    let total_controllers_active: i64 = ctl_counts.get("active");
    let total_controllers_inactivity_warning: i64 = ctl_counts.get("inactive_warning");

    let models: Vec<(Option<String>,)> = sqlx::query_as(
        "SELECT DISTINCT compatible_model FROM batteries \
         WHERE org_id = $1 AND status = 'active' AND compatible_model IS NOT NULL \
         ORDER BY compatible_model",
    )
    .bind(org_id)
    .fetch_all(pool)
    .await?;
    let mut battery_model_summary = Vec::new();
    for (m,) in models {
        if let Some(model) = m {
            battery_model_summary.push(
                crate::services::fleet_stats::get_model_comparison(pool, org_id, &model).await?,
            );
        }
    }

    let recently_flown_batteries_top5 = sqlx::query_as::<_, BatteryStatsSummary>(
        "SELECT \
            b.id AS battery_id, b.serial_number, b.equipment_id, \
            b.compatible_model AS model, b.cycle_count, \
            COUNT(f.id)::bigint AS total_flights, \
            COALESCE(SUM(f.duration_seconds), 0)::bigint AS total_flight_seconds, \
            b.last_used \
         FROM batteries b \
         LEFT JOIN flights f ON f.battery_id = b.id AND f.deleted_at IS NULL \
              AND f.flight_datetime > NOW() - INTERVAL '90 days' \
         WHERE b.org_id = $1 \
         GROUP BY b.id \
         ORDER BY total_flight_seconds DESC \
         LIMIT 5",
    )
    .bind(org_id)
    .fetch_all(pool)
    .await?;

    let recently_flown_controllers_top5 = sqlx::query_as::<_, ControllerStatsSummary>(
        "SELECT \
            c.id AS controller_id, c.serial_number, c.equipment_id, c.model, \
            COUNT(f.id)::bigint AS total_flights, \
            COALESCE(SUM(f.duration_seconds), 0)::bigint AS total_flight_seconds, \
            MAX(f.flight_datetime) AS last_used \
         FROM remote_controllers c \
         LEFT JOIN flights f ON f.controller_id = c.id AND f.deleted_at IS NULL \
              AND f.flight_datetime > NOW() - INTERVAL '90 days' \
         WHERE c.org_id = $1 \
         GROUP BY c.id \
         ORDER BY total_flight_seconds DESC \
         LIMIT 5",
    )
    .bind(org_id)
    .fetch_all(pool)
    .await?;

    Ok(FleetStatsOverview {
        total_batteries_active,
        total_batteries_near_eol,
        total_batteries_past_eol,
        total_controllers_active,
        total_controllers_inactivity_warning,
        battery_model_summary,
        recently_flown_batteries_top5,
        recently_flown_controllers_top5,
    })
}
