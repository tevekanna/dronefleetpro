// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

use axum::{
    extract::{Path, State},
    Json,
};
use chrono::Utc;
use std::collections::HashMap;

use crate::auth::middleware::{Claims, require_role};
use crate::db::DbPool;
use crate::errors::AppError;
use crate::models::maintenance::{
    AircraftMaintenanceDetail, DraftSummary, FleetMaintenanceRow, FleetMaintenanceStatus,
    HubOpenDefect, HubRecentActivity,
};
use crate::models::maintenance_schedule::{MaintenanceSchedule, ScheduleWithStatus};
use crate::services::schedule_calc::{
    build_with_status, get_fleet_totals, status_rank, AircraftTotalsRow,
};

/// Row shape returned by the aircraft detail SELECT. Matches the join
/// projection `am.name AS model` used everywhere else; we keep the
/// query local rather than pulling in the aircraft handler's
/// `AIRCRAFT_SELECT` const to avoid a cross-module dependency.
#[derive(sqlx::FromRow)]
struct AircraftDetailRow {
    id: i32,
    equipment_id: Option<String>,
    nickname: Option<String>,
    serial_number: String,
    model: Option<String>,
    model_id: Option<i32>,
    status: String,
    airworthiness_status: String,
    total_flights: i32,
    total_seconds: i32,
    last_flight: Option<chrono::DateTime<Utc>>,
}

/// GET /api/v1/maintenance/aircraft/:id
///
/// All maintenance-relevant data for a single aircraft: identity,
/// airworthiness status, totals, schedules with computed status, open
/// defects (no LIMIT), and the last 20 maintenance records. Powers the
/// new AircraftMaintenancePage in the frontend.
pub async fn get_aircraft_maintenance_detail(
    claims: Claims,
    State(pool): State<DbPool>,
    Path(id): Path<i32>,
) -> Result<Json<AircraftMaintenanceDetail>, AppError> {
    require_role(&claims, &["admin", "technician", "safety_officer", "pilot"])?;

    let org_id = claims.org_id;

    let aircraft = sqlx::query_as::<_, AircraftDetailRow>(
        "SELECT a.id, a.equipment_id, a.nickname, a.serial_number, \
                am.name AS model, a.model_id, a.status, a.airworthiness_status, \
                a.total_flights, a.total_seconds, a.last_flight \
         FROM aircraft a LEFT JOIN aircraft_models am ON a.model_id = am.id \
         WHERE a.id = $1 AND a.org_id = $2",
    )
    .bind(id)
    .bind(org_id)
    .fetch_optional(&pool)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Aircraft {id} not found")))?;

    let aircraft_name = aircraft
        .nickname
        .clone()
        .or_else(|| aircraft.model.clone())
        .unwrap_or_else(|| aircraft.serial_number.clone());
    let total_hours = aircraft.total_seconds as f64 / 3600.0;

    // Schedules with computed status — same path used by
    // maintenance_schedules::list_schedules so consumption_pct and
    // ScheduleStatus stay in sync with the rest of the app.
    let schedule_rows = sqlx::query_as::<_, MaintenanceSchedule>(
        "SELECT * FROM maintenance_schedules \
         WHERE org_id = $1 AND aircraft_id = $2 AND is_active = TRUE \
         ORDER BY name",
    )
    .bind(org_id)
    .bind(id)
    .fetch_all(&pool)
    .await?;

    let totals = get_fleet_totals(&pool, org_id).await?;
    let map: HashMap<i32, &AircraftTotalsRow> =
        totals.iter().map(|r| (r.aircraft_id, r)).collect();
    let now = Utc::now();

    let mut schedules: Vec<ScheduleWithStatus> = schedule_rows
        .into_iter()
        .map(|s| build_with_status(s, &map, now))
        .collect();

    // Order: overdue first, then warning, then ok. Within each band, by
    // next_due_at ASC NULLS LAST so the most-pressing items rise.
    schedules.sort_by(|a, b| {
        use std::cmp::Ordering;
        status_rank(a.status).cmp(&status_rank(b.status)).then_with(|| {
            match (a.schedule.next_due_at, b.schedule.next_due_at) {
                (Some(ad), Some(bd)) => ad.cmp(&bd),
                (Some(_), None) => Ordering::Less,
                (None, Some(_)) => Ordering::Greater,
                (None, None) => Ordering::Equal,
            }
        })
    });

    let open_defects: Vec<HubOpenDefect> = sqlx::query_as(
        r#"
        SELECT d.id, d.defect_date, d.description, d.status, a.equipment_id
        FROM defect_log d
        LEFT JOIN aircraft a ON a.id = d.aircraft_id
        WHERE d.org_id = $1 AND d.aircraft_id = $2
          AND d.status IN ('open', 'in_progress')
        ORDER BY d.defect_date DESC
        "#,
    )
    .bind(org_id)
    .bind(id)
    .fetch_all(&pool)
    .await?;

    let airworthiness_reason: Option<String> = sqlx::query_scalar(
        "SELECT al.diff_jsonb -> 'after' ->> 'comment' \
         FROM audit_log al \
         WHERE al.org_id = $1 \
           AND al.table_name = 'aircraft' \
           AND al.record_id = $2 \
           AND al.diff_jsonb -> 'after' ->> 'airworthiness_status' IS NOT NULL \
           AND al.diff_jsonb -> 'after' ->> 'comment' IS NOT NULL \
           AND al.diff_jsonb -> 'after' ->> 'comment' <> '' \
         ORDER BY al.created_at DESC \
         LIMIT 1",
    )
    .bind(org_id)
    .bind(id)
    .fetch_optional(&pool)
    .await?
    .flatten();

    let recent_maintenance: Vec<HubRecentActivity> = sqlx::query_as(
        r#"
        SELECT
            m.id,
            m.performed_date,
            m.maint_type,
            m.created_by,
            a.equipment_id,
            COALESCE(a.nickname, am.name, a.serial_number) AS equipment_name
        FROM maintenance m
        LEFT JOIN aircraft a ON a.id = m.aircraft_id
        LEFT JOIN aircraft_models am ON am.id = a.model_id
        WHERE m.org_id = $1 AND m.aircraft_id = $2
        ORDER BY m.performed_date DESC, m.created_at DESC
        LIMIT 20
        "#,
    )
    .bind(org_id)
    .bind(id)
    .fetch_all(&pool)
    .await?;

    let drafts: Vec<DraftSummary> = sqlx::query_as(
        r#"
        SELECT
            md.schedule_id,
            (
                SELECT COUNT(*)::int
                FROM jsonb_each_text(md.checklist_status)
                WHERE value <> 'pending'
            ) AS items_marked,
            COALESCE(
                (
                    SELECT COUNT(mci.id)::int
                    FROM (
                        SELECT id FROM maintenance_instruction_templates
                        WHERE aircraft_model_id = a.model_id
                        ORDER BY id
                        LIMIT 1
                    ) first_tpl
                    JOIN maintenance_checklist_items mci ON mci.template_id = first_tpl.id
                ), 0
            ) AS items_total
        FROM maintenance_drafts md
        JOIN maintenance_schedules ms ON ms.id = md.schedule_id
        JOIN aircraft a ON a.id = ms.aircraft_id
        WHERE md.org_id = $1 AND ms.aircraft_id = $2
        "#,
    )
    .bind(org_id)
    .bind(id)
    .fetch_all(&pool)
    .await?;

    Ok(Json(AircraftMaintenanceDetail {
        aircraft_id: aircraft.id,
        equipment_id: aircraft.equipment_id,
        aircraft_name,
        serial_number: aircraft.serial_number,
        model: aircraft.model,
        model_id: aircraft.model_id,
        status: aircraft.status,
        airworthiness_status: aircraft.airworthiness_status,
        airworthiness_reason,
        total_flights: aircraft.total_flights,
        total_hours,
        last_flight: aircraft.last_flight,
        schedules,
        open_defects,
        recent_maintenance,
        drafts,
    }))
}

/// GET /api/v1/maintenance/fleet-status
///
/// Per-aircraft aggregate counts (overdue, due-soon, open defects) plus
/// the nearest upcoming calendar schedule, for active aircraft only.
/// Summary counts are aggregated from the per-row results so callers
/// receive a single consistent snapshot.
pub async fn get_fleet_maintenance_status(
    claims: Claims,
    State(pool): State<DbPool>,
) -> Result<Json<FleetMaintenanceStatus>, AppError> {
    require_role(&claims, &["admin", "technician", "safety_officer", "pilot"])?;

    let org_id = claims.org_id;

    // Single query with CTEs. Overdue and due-soon counts mirror the
    // logic in maintenance_hub.rs (calendar_days / flight_hours /
    // cycles); the open-defect count comes from defect_log; the
    // next-due hint is the earliest future calendar_days schedule.
    let rows: Vec<FleetMaintenanceRow> = sqlx::query_as(
        r#"
        WITH overdue AS (
            SELECT ms.aircraft_id, COUNT(*)::bigint AS n
            FROM maintenance_schedules ms
            JOIN aircraft a ON a.id = ms.aircraft_id
            WHERE ms.org_id = $1 AND ms.is_active = TRUE AND a.status = 'active'
              AND (
                (ms.interval_type = 'calendar_days' AND ms.next_due_at < NOW())
                OR (ms.interval_type = 'flight_hours' AND ms.next_due_flight_hours IS NOT NULL
                    AND (a.total_seconds / 3600.0) >= ms.next_due_flight_hours)
                OR (ms.interval_type = 'cycles' AND ms.next_due_cycles IS NOT NULL
                    AND a.total_flights >= ms.next_due_cycles)
              )
            GROUP BY ms.aircraft_id
        ),
        due_soon AS (
            SELECT ms.aircraft_id, COUNT(*)::bigint AS n
            FROM maintenance_schedules ms
            JOIN aircraft a ON a.id = ms.aircraft_id
            WHERE ms.org_id = $1 AND ms.is_active = TRUE AND a.status = 'active'
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
            GROUP BY ms.aircraft_id
        ),
        open_defects AS (
            SELECT d.aircraft_id, COUNT(*)::bigint AS n
            FROM defect_log d
            WHERE d.org_id = $1 AND d.status IN ('open', 'in_progress')
              AND d.aircraft_id IS NOT NULL
            GROUP BY d.aircraft_id
        ),
        next_upcoming AS (
            SELECT DISTINCT ON (ms.aircraft_id)
                ms.aircraft_id,
                ms.name AS next_due_label,
                ms.next_due_at
            FROM maintenance_schedules ms
            JOIN aircraft a ON a.id = ms.aircraft_id
            WHERE ms.org_id = $1 AND ms.is_active = TRUE AND a.status = 'active'
              AND ms.interval_type = 'calendar_days'
              AND ms.next_due_at >= NOW()
            ORDER BY ms.aircraft_id, ms.next_due_at ASC
        )
        SELECT
            a.id AS aircraft_id,
            a.equipment_id,
            COALESCE(a.nickname, am.name, a.serial_number) AS aircraft_name,
            a.serial_number,
            am.name AS model,
            a.airworthiness_status,
            COALESCE(o.n, 0) AS overdue_count,
            COALESCE(ds.n, 0) AS due_soon_count,
            COALESCE(od.n, 0) AS open_defect_count,
            nu.next_due_label,
            nu.next_due_at
        FROM aircraft a
        LEFT JOIN aircraft_models am ON am.id = a.model_id
        LEFT JOIN overdue o      ON o.aircraft_id  = a.id
        LEFT JOIN due_soon ds    ON ds.aircraft_id = a.id
        LEFT JOIN open_defects od ON od.aircraft_id = a.id
        LEFT JOIN next_upcoming nu ON nu.aircraft_id = a.id
        WHERE a.org_id = $1 AND a.status = 'active'
        ORDER BY
            CASE WHEN a.airworthiness_status = 'not_airworthy' THEN 0 ELSE 1 END,
            COALESCE(o.n, 0)  DESC,
            COALESCE(ds.n, 0) DESC,
            COALESCE(a.nickname, am.name, a.serial_number) ASC
        "#,
    )
    .bind(org_id)
    .fetch_all(&pool)
    .await?;

    let mut airworthy_count = 0i64;
    let mut not_airworthy_count = 0i64;
    let mut overdue_count = 0i64;
    let mut due_soon_count = 0i64;
    let mut open_defect_count = 0i64;
    for r in &rows {
        if r.airworthiness_status == "not_airworthy" {
            not_airworthy_count += 1;
        } else {
            airworthy_count += 1;
        }
        overdue_count += r.overdue_count;
        due_soon_count += r.due_soon_count;
        open_defect_count += r.open_defect_count;
    }

    Ok(Json(FleetMaintenanceStatus {
        airworthy_count,
        not_airworthy_count,
        overdue_count,
        due_soon_count,
        open_defect_count,
        aircraft: rows,
    }))
}
