// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

use axum::{
    extract::{Path, State},
    Json,
};
use chrono::Utc;
use std::collections::HashMap;

use crate::auth::middleware::Claims;
use crate::db::DbPool;
use crate::errors::AppError;
use crate::auth::middleware::require_admin_or_technician;
use crate::models::maintenance::MaintenanceRecord;
use crate::models::maintenance_schedule::{
    CompleteScheduleRequest, MaintenanceSchedule, ScheduleDashboard, ScheduleStatus,
    ScheduleWithStatus,
};
use crate::services::audit;
use crate::services::schedule_calc::{
    build_with_status, compute_next_due, compute_secondary_next_due, get_aircraft_totals,
    get_fleet_totals, status_rank, AircraftTotalsRow,
};

#[derive(Debug, serde::Serialize)]
pub struct CompleteScheduleResponse {
    pub maintenance_id: i32,
    pub schedule: MaintenanceSchedule,
}

const VALID_MAINT_TYPES: &[&str] = &["Repair", "Modification", "Overhaul", "Test"];
const MIN_DESC_LEN: usize = 5;
const DASHBOARD_ITEM_LIMIT: usize = 10;

fn validate_user_code(code: &str, field: &str) -> Result<(), AppError> {
    if code.len() != 3 || !code.chars().all(|c| c.is_ascii_uppercase()) {
        return Err(AppError::BadRequest(format!(
            "{field} must be exactly 3 uppercase letters"
        )));
    }
    Ok(())
}

/// POST /api/v1/maintenance-schedules/:id/complete
pub async fn complete_schedule(
    claims: Claims,
    State(pool): State<DbPool>,
    Path(id): Path<i32>,
    Json(body): Json<CompleteScheduleRequest>,
) -> Result<Json<CompleteScheduleResponse>, AppError> {
    require_admin_or_technician(&claims)?;

    if body.action_taken.trim().len() < MIN_DESC_LEN {
        return Err(AppError::BadRequest(format!(
            "action_taken must be at least {MIN_DESC_LEN} characters (EASA OSO #3)"
        )));
    }
    if body.remark.trim().len() < MIN_DESC_LEN {
        return Err(AppError::BadRequest(format!(
            "remark must be at least {MIN_DESC_LEN} characters (EASA OSO #3)"
        )));
    }
    if !VALID_MAINT_TYPES.contains(&body.maint_type.as_str()) {
        return Err(AppError::BadRequest(format!(
            "maint_type must be one of: {VALID_MAINT_TYPES:?}"
        )));
    }
    validate_user_code(&body.sign_off, "sign_off")?;

    let sched = sqlx::query_as::<_, MaintenanceSchedule>(
        "SELECT * FROM maintenance_schedules WHERE id = $1 AND org_id = $2 AND is_active = TRUE",
    )
    .bind(id)
    .bind(claims.org_id)
    .fetch_optional(&pool)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Active maintenance schedule {id} not found")))?;

    let totals = get_aircraft_totals(&pool, claims.org_id, sched.aircraft_id).await?;
    let performed_at = body
        .performed_date
        .and_hms_opt(12, 0, 0)
        .ok_or_else(|| AppError::BadRequest("invalid performed_date".into()))?
        .and_utc();

    let new_last_fh = totals.flight_hours as f32;
    let new_last_cycles = totals.flight_count as i32;
    let (next_due_at, next_due_flight_hours, next_due_cycles) = compute_next_due(
        &sched.interval_type,
        sched.interval_value,
        Some(performed_at),
        Some(new_last_fh),
        Some(new_last_cycles),
    );
    let (secondary_next_due_at, secondary_next_due_hours) = compute_secondary_next_due(
        sched.secondary_interval_type.as_deref(),
        sched.secondary_interval_value,
        Some(performed_at),
        Some(new_last_fh),
    );

    let mut tx = pool.begin().await?;

    let maint_row: (i32,) = sqlx::query_as(
        "INSERT INTO maintenance (\
            org_id, aircraft_id, performed_date, created_by, remark, action_taken, \
            maint_category, maint_type, sign_off, schedule_id\
         ) VALUES ($1,$2,$3,$4,$5,$6,'Scheduled',$7,$8,$9) RETURNING id",
    )
    .bind(claims.org_id)
    .bind(sched.aircraft_id)
    .bind(body.performed_date)
    .bind(&claims.user_code)
    .bind(body.remark.trim())
    .bind(body.action_taken.trim())
    .bind(&body.maint_type)
    .bind(&body.sign_off)
    .bind(id)
    .fetch_one(&mut *tx)
    .await?;
    let maint_id = maint_row.0;

    // Refetch the maintenance row with the aircraft_name + controller_name
    // joins so the audit snapshot matches the shape the maintenance endpoint
    // returns. Both computed columns are required: MaintenanceRecord has
    // `controller_name`, so omitting it makes sqlx FromRow fail with
    // ColumnNotFound and rolls back the whole completion. Mirrors the canonical
    // projection in handlers/maintenance.rs (MAINTENANCE_SELECT_COLUMNS/JOINS).
    let maint_full = sqlx::query_as::<_, MaintenanceRecord>(
        "SELECT m.*, COALESCE(a.nickname, am.name, a.serial_number) as aircraft_name, \
         COALESCE(rc.nickname, rc.model, rc.serial_number) as controller_name \
         FROM maintenance m LEFT JOIN aircraft a ON m.aircraft_id = a.id \
         LEFT JOIN aircraft_models am ON a.model_id = am.id \
         LEFT JOIN remote_controllers rc ON rc.id = m.controller_id \
         WHERE m.id = $1"
    )
    .bind(maint_id)
    .fetch_one(&mut *tx)
    .await?;

    let updated = sqlx::query_as::<_, MaintenanceSchedule>(
        "UPDATE maintenance_schedules SET \
            last_performed_at = $1, \
            last_performed_flight_hours = $2, \
            last_performed_cycles = $3, \
            next_due_at = $4, \
            next_due_flight_hours = $5, \
            next_due_cycles = $6, \
            secondary_next_due_at = $7, \
            secondary_next_due_hours = $8 \
         WHERE id = $9 AND org_id = $10 RETURNING *",
    )
    .bind(performed_at)
    .bind(new_last_fh)
    .bind(new_last_cycles)
    .bind(next_due_at)
    .bind(next_due_flight_hours)
    .bind(next_due_cycles)
    .bind(secondary_next_due_at)
    .bind(secondary_next_due_hours)
    .bind(id)
    .bind(claims.org_id)
    .fetch_one(&mut *tx)
    .await?;

    tx.commit().await?;

    // INSERT audit for the new maintenance record (full row snapshot).
    audit::log_insert_full(
        &pool,
        claims.org_id,
        claims.user_id,
        &claims.user_code,
        "maintenance",
        maint_id,
        &maint_full,
    )
    .await;
    // UPDATE audit for the schedule — real before/after diff of baseline dates.
    // Per task decision 3, we dropped the custom {action:"complete",...} meta;
    // the sibling maintenance INSERT row (same timestamp + user) is the
    // cross-reference for auditors.
    audit::log_update_diff(
        &pool,
        claims.org_id,
        claims.user_id,
        &claims.user_code,
        "maintenance_schedules",
        id,
        &sched,
        &updated,
    )
    .await;

    Ok(Json(CompleteScheduleResponse {
        maintenance_id: maint_id,
        schedule: updated,
    }))
}

/// GET /api/v1/maintenance-schedules/dashboard
pub async fn schedule_dashboard(
    claims: Claims,
    State(pool): State<DbPool>,
) -> Result<Json<ScheduleDashboard>, AppError> {
    let schedules = sqlx::query_as::<_, MaintenanceSchedule>(
        "SELECT * FROM maintenance_schedules WHERE org_id = $1 AND is_active = TRUE",
    )
    .bind(claims.org_id)
    .fetch_all(&pool)
    .await?;

    let totals = get_fleet_totals(&pool, claims.org_id).await?;
    let map: HashMap<i32, &AircraftTotalsRow> =
        totals.iter().map(|r| (r.aircraft_id, r)).collect();
    let now = Utc::now();

    let mut items: Vec<ScheduleWithStatus> = schedules
        .into_iter()
        .map(|s| build_with_status(s, &map, now))
        .collect();

    let overdue = items.iter().filter(|i| i.status == ScheduleStatus::Overdue).count();
    let warning = items.iter().filter(|i| i.status == ScheduleStatus::Warning).count();
    let ok = items.iter().filter(|i| i.status == ScheduleStatus::Ok).count();

    // Sort: Overdue → Warning → Ok, then by due date (earliest/most-overdue first).
    // Schedules without a next_due_at (e.g. flight_hours-only) sort after calendar ones
    // within the same status band, ordered by consumption ratio as a fallback.
    items.sort_by(|a, b| {
        use std::cmp::Ordering;
        status_rank(a.status)
            .cmp(&status_rank(b.status))
            .then_with(|| match (a.schedule.next_due_at, b.schedule.next_due_at) {
                (Some(ad), Some(bd)) => ad.cmp(&bd),
                (Some(_), None) => Ordering::Less,
                (None, Some(_)) => Ordering::Greater,
                (None, None) => b
                    .consumption_pct
                    .partial_cmp(&a.consumption_pct)
                    .unwrap_or(Ordering::Equal),
            })
    });
    items.truncate(DASHBOARD_ITEM_LIMIT);

    Ok(Json(ScheduleDashboard {
        overdue,
        warning,
        ok,
        items,
    }))
}
