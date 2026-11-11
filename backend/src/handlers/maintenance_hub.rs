// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

use axum::extract::State;
use axum::Json;

use crate::auth::middleware::Claims;
use crate::auth::middleware::require_role;
use crate::db::DbPool;
use crate::errors::AppError;
use crate::models::maintenance::{
    HubAlert, HubFleetSummary, HubOpenDefect, HubRecentActivity, HubUpcoming,
    MaintenanceHubData,
};

pub async fn get_maintenance_hub(
    claims: Claims,
    State(pool): State<DbPool>,
) -> Result<Json<MaintenanceHubData>, AppError> {
    require_role(&claims, &["admin", "technician", "safety_officer", "pilot"])?;

    let org_id = claims.org_id;

    let active_aircraft: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM aircraft WHERE org_id = $1 AND status = 'active'"
    )
    .bind(org_id)
    .fetch_one(&pool)
    .await?;

    let overdue_schedules: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*) FROM maintenance_schedules ms
        JOIN aircraft a ON a.id = ms.aircraft_id
        WHERE ms.org_id = $1 AND ms.is_active = true AND a.status = 'active'
          AND (
            (ms.interval_type = 'calendar_days'  AND ms.next_due_at < NOW())
            OR (ms.interval_type = 'flight_hours' AND ms.next_due_flight_hours IS NOT NULL
                AND (a.total_seconds / 3600.0) >= ms.next_due_flight_hours)
            OR (ms.interval_type = 'cycles'       AND ms.next_due_cycles IS NOT NULL
                AND a.total_flights >= ms.next_due_cycles)
          )
        "#
    )
    .bind(org_id)
    .fetch_one(&pool)
    .await?;

    let due_soon_schedules: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*) FROM maintenance_schedules ms
        JOIN aircraft a ON a.id = ms.aircraft_id
        WHERE ms.org_id = $1 AND ms.is_active = true AND a.status = 'active'
          AND (
            (ms.interval_type = 'calendar_days' AND ms.next_due_at >= NOW()
             AND ms.next_due_at <= NOW() + INTERVAL '30 days')
            OR (ms.interval_type = 'flight_hours' AND ms.next_due_flight_hours IS NOT NULL
                AND (a.total_seconds / 3600.0) >= ms.next_due_flight_hours * ms.warning_threshold
                AND (a.total_seconds / 3600.0) < ms.next_due_flight_hours)
            OR (ms.interval_type = 'cycles' AND ms.next_due_cycles IS NOT NULL
                AND a.total_flights >= ms.next_due_cycles * ms.warning_threshold
                AND a.total_flights < ms.next_due_cycles)
          )
        "#
    )
    .bind(org_id)
    .fetch_one(&pool)
    .await?;

    let open_defects_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM defect_log WHERE org_id = $1 AND status IN ('open', 'in_progress')"
    )
    .bind(org_id)
    .fetch_one(&pool)
    .await?;

    let summary = HubFleetSummary {
        active_aircraft,
        overdue_schedules,
        due_soon_schedules,
        open_defects: open_defects_count,
    };

    let alerts: Vec<HubAlert> = sqlx::query_as(
        r#"
        SELECT
            ms.id AS schedule_id,
            ms.name AS schedule_name,
            ms.interval_type,
            a.equipment_id,
            COALESCE(a.nickname, am.name, a.serial_number) AS aircraft_name,
            CASE
                WHEN ms.interval_type = 'calendar_days' AND ms.next_due_at < NOW()                                          THEN 'overdue'
                WHEN ms.interval_type = 'flight_hours'  AND ms.next_due_flight_hours IS NOT NULL
                     AND (a.total_seconds / 3600.0) >= ms.next_due_flight_hours                                             THEN 'overdue'
                WHEN ms.interval_type = 'cycles'        AND ms.next_due_cycles IS NOT NULL
                     AND a.total_flights >= ms.next_due_cycles                                                              THEN 'overdue'
                ELSE 'due_soon'
            END AS severity,
            CASE WHEN ms.interval_type = 'calendar_days' AND ms.next_due_at < NOW()
                 THEN EXTRACT(DAY FROM NOW() - ms.next_due_at)::bigint
                 ELSE NULL END AS overdue_days,
            CASE WHEN ms.interval_type = 'flight_hours' AND ms.next_due_flight_hours IS NOT NULL
                      AND (a.total_seconds / 3600.0) >= ms.next_due_flight_hours
                 THEN (a.total_seconds / 3600.0 - ms.next_due_flight_hours)
                 ELSE NULL END AS overdue_hours,
            CASE WHEN ms.interval_type = 'cycles' AND ms.next_due_cycles IS NOT NULL
                      AND a.total_flights >= ms.next_due_cycles
                 THEN (a.total_flights - ms.next_due_cycles)::bigint
                 ELSE NULL END AS overdue_cycles,
            CASE WHEN ms.interval_type = 'calendar_days' AND ms.next_due_at >= NOW()
                 THEN EXTRACT(DAY FROM ms.next_due_at - NOW())::bigint
                 ELSE NULL END AS due_in_days
        FROM maintenance_schedules ms
        JOIN aircraft a ON a.id = ms.aircraft_id
        LEFT JOIN aircraft_models am ON am.id = a.model_id
        WHERE ms.org_id = $1 AND ms.is_active = true AND a.status = 'active'
          AND (
            (ms.interval_type = 'calendar_days' AND (ms.next_due_at < NOW()
              OR ms.next_due_at <= NOW() + INTERVAL '30 days'))
            OR (ms.interval_type = 'flight_hours' AND ms.next_due_flight_hours IS NOT NULL
                AND (a.total_seconds / 3600.0) >= ms.next_due_flight_hours * ms.warning_threshold)
            OR (ms.interval_type = 'cycles' AND ms.next_due_cycles IS NOT NULL
                AND a.total_flights >= ms.next_due_cycles * ms.warning_threshold)
          )
        ORDER BY
            CASE
                WHEN ms.interval_type = 'calendar_days' AND ms.next_due_at < NOW()                                         THEN 0
                WHEN ms.interval_type = 'flight_hours'  AND ms.next_due_flight_hours IS NOT NULL
                     AND (a.total_seconds / 3600.0) >= ms.next_due_flight_hours                                            THEN 0
                WHEN ms.interval_type = 'cycles'        AND ms.next_due_cycles IS NOT NULL
                     AND a.total_flights >= ms.next_due_cycles                                                              THEN 0
                ELSE 1
            END,
            ms.next_due_at NULLS LAST
        LIMIT 50
        "#
    )
    .bind(org_id)
    .fetch_all(&pool)
    .await?;

    let upcoming: Vec<HubUpcoming> = sqlx::query_as(
        r#"
        SELECT
            ms.id AS schedule_id,
            ms.name AS schedule_name,
            ms.next_due_at,
            a.equipment_id,
            COALESCE(a.nickname, am.name, a.serial_number) AS aircraft_name
        FROM maintenance_schedules ms
        JOIN aircraft a ON a.id = ms.aircraft_id
        LEFT JOIN aircraft_models am ON am.id = a.model_id
        WHERE ms.org_id = $1 AND ms.is_active = true AND a.status = 'active'
          AND ms.interval_type = 'calendar_days'
          AND ms.next_due_at >= NOW()
          AND ms.next_due_at <= NOW() + INTERVAL '30 days'
        ORDER BY ms.next_due_at
        LIMIT 20
        "#
    )
    .bind(org_id)
    .fetch_all(&pool)
    .await?;

    let recent_activity: Vec<HubRecentActivity> = sqlx::query_as(
        r#"
        SELECT
            m.id,
            m.performed_date,
            m.maint_type,
            m.created_by,
            COALESCE(a.equipment_id, rc.equipment_id) AS equipment_id,
            COALESCE(a.nickname, am.name, a.serial_number,
                     rc.nickname, rc.model, rc.serial_number) AS equipment_name
        FROM maintenance m
        LEFT JOIN aircraft a ON a.id = m.aircraft_id
        LEFT JOIN aircraft_models am ON am.id = a.model_id
        LEFT JOIN remote_controllers rc ON rc.id = m.controller_id
        WHERE m.org_id = $1
        ORDER BY m.performed_date DESC, m.created_at DESC
        LIMIT 10
        "#
    )
    .bind(org_id)
    .fetch_all(&pool)
    .await?;

    let open_defects: Vec<HubOpenDefect> = sqlx::query_as(
        r#"
        SELECT d.id, d.defect_date, d.description, d.status, a.equipment_id
        FROM defect_log d
        LEFT JOIN aircraft a ON a.id = d.aircraft_id
        WHERE d.org_id = $1 AND d.status IN ('open', 'in_progress')
        ORDER BY d.defect_date DESC
        LIMIT 10
        "#
    )
    .bind(org_id)
    .fetch_all(&pool)
    .await?;

    Ok(Json(MaintenanceHubData {
        summary,
        alerts,
        upcoming,
        recent_activity,
        open_defects,
    }))
}
