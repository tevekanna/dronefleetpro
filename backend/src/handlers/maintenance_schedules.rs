// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

use axum::{
    extract::{Path, Query, State},
    Json,
};
use chrono::Utc;
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;

use crate::auth::middleware::{require_admin, require_admin_or_technician, Claims};
use crate::db::DbPool;
use crate::errors::AppError;
use crate::models::maintenance_schedule::{
    CreateMaintenanceScheduleRequest, MaintenanceSchedule, ScheduleDetail, ScheduleHistoryItem,
    ScheduleWithStatus, UpdateMaintenanceScheduleRequest,
};
use crate::services::audit;
use crate::services::schedule_calc::{
    aircraft_belongs_to_org, build_with_status, compute_next_due, compute_secondary_next_due,
    get_fleet_totals, parse_status, validate_instruction_template, validate_primary_type,
    validate_secondary, AircraftTotalsRow,
};

#[derive(Debug, Deserialize, Default)]
pub struct ListQueryParams {
    pub aircraft_id: Option<i32>,
    pub include_inactive: Option<bool>,
    pub status: Option<String>,
}

/// GET /api/v1/maintenance-schedules
pub async fn list_schedules(
    claims: Claims,
    State(pool): State<DbPool>,
    Query(params): Query<ListQueryParams>,
) -> Result<Json<Vec<ScheduleWithStatus>>, AppError> {
    let include_inactive = params.include_inactive.unwrap_or(false);
    let status_filter = match params.status.as_deref() {
        Some(s) => Some(parse_status(s)?),
        None => None,
    };

    let rows = sqlx::query_as::<_, MaintenanceSchedule>(
        "SELECT * FROM maintenance_schedules \
         WHERE org_id = $1 \
           AND ($2::int IS NULL OR aircraft_id = $2) \
           AND ($3::bool OR is_active = TRUE) \
         ORDER BY aircraft_id, name",
    )
    .bind(claims.org_id)
    .bind(params.aircraft_id)
    .bind(include_inactive)
    .fetch_all(&pool)
    .await?;

    let totals = get_fleet_totals(&pool, claims.org_id).await?;
    let map: HashMap<i32, &AircraftTotalsRow> =
        totals.iter().map(|r| (r.aircraft_id, r)).collect();
    let now = Utc::now();

    let items: Vec<ScheduleWithStatus> = rows
        .into_iter()
        .map(|schedule| build_with_status(schedule, &map, now))
        .filter(|item| status_filter.map_or(true, |s| item.status == s))
        .collect();

    Ok(Json(items))
}

/// GET /api/v1/maintenance-schedules/:id — schedule + completion history.
pub async fn get_schedule(
    claims: Claims,
    State(pool): State<DbPool>,
    Path(id): Path<i32>,
) -> Result<Json<ScheduleDetail>, AppError> {
    let schedule = sqlx::query_as::<_, MaintenanceSchedule>(
        "SELECT * FROM maintenance_schedules WHERE id = $1 AND org_id = $2",
    )
    .bind(id)
    .bind(claims.org_id)
    .fetch_optional(&pool)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Maintenance schedule {id} not found")))?;

    let history = sqlx::query_as::<_, ScheduleHistoryItem>(
        "SELECT id AS maintenance_id, performed_date, created_by, \
                COALESCE(remark, '') AS remark, \
                COALESCE(action_taken, '') AS action_taken, \
                COALESCE(maint_type, '') AS maint_type, \
                sign_off \
         FROM maintenance WHERE schedule_id = $1 AND org_id = $2 \
         ORDER BY performed_date DESC, created_at DESC",
    )
    .bind(id)
    .bind(claims.org_id)
    .fetch_all(&pool)
    .await?;

    let totals = get_fleet_totals(&pool, claims.org_id).await?;
    let map: HashMap<i32, &AircraftTotalsRow> =
        totals.iter().map(|r| (r.aircraft_id, r)).collect();
    let with_status = build_with_status(schedule, &map, Utc::now());

    Ok(Json(ScheduleDetail { with_status, history }))
}

/// POST /api/v1/maintenance-schedules
pub async fn create_schedule(
    claims: Claims,
    State(pool): State<DbPool>,
    Json(body): Json<CreateMaintenanceScheduleRequest>,
) -> Result<Json<MaintenanceSchedule>, AppError> {
    require_admin_or_technician(&claims)?;

    let name = body.name.trim();
    if name.is_empty() {
        return Err(AppError::BadRequest("name is required".into()));
    }
    validate_primary_type(&body.interval_type)?;
    if body.interval_value <= 0.0 {
        return Err(AppError::BadRequest(
            "interval_value must be greater than 0".into(),
        ));
    }
    validate_secondary(
        body.secondary_interval_type.as_deref(),
        body.secondary_interval_value,
    )?;

    if !aircraft_belongs_to_org(&pool, body.aircraft_id, claims.org_id).await? {
        let aid = body.aircraft_id;
        return Err(AppError::BadRequest(format!(
            "aircraft_id {aid} not found in this organisation"
        )));
    }

    if let Some(tid) = body.instruction_template_id {
        validate_instruction_template(&pool, claims.org_id, tid, body.aircraft_id).await?;
    }

    let warning_threshold = body.warning_threshold.unwrap_or(crate::services::schedule_calc::DEFAULT_WARNING_THRESHOLD);
    let is_critical = body.is_critical.unwrap_or(false);
    let last_performed_flight_hours = body.last_performed_flight_hours.unwrap_or(0.0);
    let last_performed_cycles = body.last_performed_cycles.unwrap_or(0);

    let (next_due_at, next_due_flight_hours, next_due_cycles) = compute_next_due(
        &body.interval_type,
        body.interval_value,
        body.last_performed_at,
        body.last_performed_flight_hours,
        body.last_performed_cycles,
    );
    let (secondary_next_due_at, secondary_next_due_hours) = compute_secondary_next_due(
        body.secondary_interval_type.as_deref(),
        body.secondary_interval_value,
        body.last_performed_at,
        body.last_performed_flight_hours,
    );

    let row = sqlx::query_as::<_, MaintenanceSchedule>(
        "INSERT INTO maintenance_schedules (\
            org_id, aircraft_id, name, description, interval_type, interval_value, \
            warning_threshold, is_critical, last_performed_at, last_performed_flight_hours, \
            last_performed_cycles, next_due_at, next_due_flight_hours, next_due_cycles, \
            is_active, secondary_interval_type, secondary_interval_value, \
            secondary_next_due_at, secondary_next_due_hours, created_by, \
            instruction_template_id\
         ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,TRUE,$15,$16,$17,$18,$19,$20) \
         RETURNING *",
    )
    .bind(claims.org_id)
    .bind(body.aircraft_id)
    .bind(name)
    .bind(&body.description)
    .bind(&body.interval_type)
    .bind(body.interval_value)
    .bind(warning_threshold)
    .bind(is_critical)
    .bind(body.last_performed_at)
    .bind(last_performed_flight_hours)
    .bind(last_performed_cycles)
    .bind(next_due_at)
    .bind(next_due_flight_hours)
    .bind(next_due_cycles)
    .bind(&body.secondary_interval_type)
    .bind(body.secondary_interval_value)
    .bind(secondary_next_due_at)
    .bind(secondary_next_due_hours)
    .bind(&claims.user_code)
    .bind(body.instruction_template_id)
    .fetch_one(&pool)
    .await?;

    audit::log_insert_full(
        &pool,
        claims.org_id,
        claims.user_id,
        &claims.user_code,
        "maintenance_schedules",
        row.id,
        &row,
    )
    .await;

    Ok(Json(row))
}

/// PUT /api/v1/maintenance-schedules/:id
pub async fn update_schedule(
    claims: Claims,
    State(pool): State<DbPool>,
    Path(id): Path<i32>,
    Json(body): Json<UpdateMaintenanceScheduleRequest>,
) -> Result<Json<MaintenanceSchedule>, AppError> {
    require_admin_or_technician(&claims)?;

    let existing = sqlx::query_as::<_, MaintenanceSchedule>(
        "SELECT * FROM maintenance_schedules WHERE id = $1 AND org_id = $2",
    )
    .bind(id)
    .bind(claims.org_id)
    .fetch_optional(&pool)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Maintenance schedule {id} not found")))?;

    if let Some(ref name) = body.name {
        if name.trim().is_empty() {
            return Err(AppError::BadRequest("name cannot be empty".into()));
        }
    }
    if let Some(ref it) = body.interval_type {
        validate_primary_type(it)?;
    }
    if let Some(iv) = body.interval_value {
        if iv <= 0.0 {
            return Err(AppError::BadRequest(
                "interval_value must be greater than 0".into(),
            ));
        }
    }
    if let Some(aid) = body.aircraft_id {
        if !aircraft_belongs_to_org(&pool, aid, claims.org_id).await? {
            return Err(AppError::BadRequest(format!(
                "aircraft_id {aid} not found in this organisation"
            )));
        }
    }
    let new_secondary_type = body
        .secondary_interval_type
        .clone()
        .or_else(|| existing.secondary_interval_type.clone());
    let new_secondary_value = body
        .secondary_interval_value
        .or(existing.secondary_interval_value);
    validate_secondary(new_secondary_type.as_deref(), new_secondary_value)?;

    let new_aircraft_id = body.aircraft_id.unwrap_or(existing.aircraft_id);
    let new_name = body
        .name
        .as_deref()
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| existing.name.clone());
    let new_description = body.description.clone().or_else(|| existing.description.clone());
    let new_interval_type = body
        .interval_type
        .clone()
        .unwrap_or_else(|| existing.interval_type.clone());
    let new_interval_value = body.interval_value.unwrap_or(existing.interval_value);
    let new_warning_threshold = body.warning_threshold.unwrap_or(existing.warning_threshold);
    let new_is_critical = body.is_critical.unwrap_or(existing.is_critical);
    let new_last_performed_at = body.last_performed_at.or(existing.last_performed_at);
    let new_last_performed_flight_hours = body
        .last_performed_flight_hours
        .unwrap_or(existing.last_performed_flight_hours);
    let new_last_performed_cycles = body
        .last_performed_cycles
        .unwrap_or(existing.last_performed_cycles);
    let new_is_active = body.is_active.unwrap_or(existing.is_active);
    let new_instruction_template_id =
        body.instruction_template_id.or(existing.instruction_template_id);

    if body.instruction_template_id.is_some() {
        validate_instruction_template(
            &pool,
            claims.org_id,
            new_instruction_template_id.unwrap(),
            new_aircraft_id,
        )
        .await?;
    }

    let recompute = body.interval_type.is_some()
        || body.interval_value.is_some()
        || body.last_performed_at.is_some()
        || body.last_performed_flight_hours.is_some()
        || body.last_performed_cycles.is_some()
        || body.secondary_interval_type.is_some()
        || body.secondary_interval_value.is_some();

    // The baseline is "semantically set" when the user provides it, or when the existing row
    // already had a corresponding next_due value (meaning the baseline was set at creation).
    let effective_flight_hours: Option<f32> = body
        .last_performed_flight_hours
        .or_else(|| existing.next_due_flight_hours.map(|_| existing.last_performed_flight_hours));
    let effective_cycles: Option<i32> = body
        .last_performed_cycles
        .or_else(|| existing.next_due_cycles.map(|_| existing.last_performed_cycles));

    let (next_due_at, next_due_flight_hours, next_due_cycles) = if recompute {
        compute_next_due(
            &new_interval_type,
            new_interval_value,
            new_last_performed_at,
            effective_flight_hours,
            effective_cycles,
        )
    } else {
        (existing.next_due_at, existing.next_due_flight_hours, existing.next_due_cycles)
    };

    let (secondary_next_due_at, secondary_next_due_hours) = if recompute {
        compute_secondary_next_due(
            new_secondary_type.as_deref(),
            new_secondary_value,
            new_last_performed_at,
            effective_flight_hours,
        )
    } else {
        (existing.secondary_next_due_at, existing.secondary_next_due_hours)
    };

    let row = sqlx::query_as::<_, MaintenanceSchedule>(
        "UPDATE maintenance_schedules SET \
            aircraft_id = $1, name = $2, description = $3, interval_type = $4, \
            interval_value = $5, warning_threshold = $6, is_critical = $7, \
            last_performed_at = $8, last_performed_flight_hours = $9, \
            last_performed_cycles = $10, next_due_at = $11, next_due_flight_hours = $12, \
            next_due_cycles = $13, is_active = $14, \
            secondary_interval_type = $15, secondary_interval_value = $16, \
            secondary_next_due_at = $17, secondary_next_due_hours = $18, \
            instruction_template_id = $19 \
         WHERE id = $20 AND org_id = $21 RETURNING *",
    )
    .bind(new_aircraft_id)
    .bind(&new_name)
    .bind(&new_description)
    .bind(&new_interval_type)
    .bind(new_interval_value)
    .bind(new_warning_threshold)
    .bind(new_is_critical)
    .bind(new_last_performed_at)
    .bind(new_last_performed_flight_hours)
    .bind(new_last_performed_cycles)
    .bind(next_due_at)
    .bind(next_due_flight_hours)
    .bind(next_due_cycles)
    .bind(new_is_active)
    .bind(&new_secondary_type)
    .bind(new_secondary_value)
    .bind(secondary_next_due_at)
    .bind(secondary_next_due_hours)
    .bind(new_instruction_template_id)
    .bind(id)
    .bind(claims.org_id)
    .fetch_one(&pool)
    .await?;

    audit::log_update_diff(
        &pool,
        claims.org_id,
        claims.user_id,
        &claims.user_code,
        "maintenance_schedules",
        row.id,
        &existing,
        &row,
    )
    .await;

    Ok(Json(row))
}

/// DELETE /api/v1/maintenance-schedules/:id — soft delete. EASA 4.2.3 retention.
pub async fn delete_schedule(
    claims: Claims,
    State(pool): State<DbPool>,
    Path(id): Path<i32>,
) -> Result<Json<Value>, AppError> {
    require_admin(&claims)?;

    let mut tx = pool.begin().await?;
    let before = sqlx::query_as::<_, MaintenanceSchedule>(
        "SELECT * FROM maintenance_schedules WHERE id = $1 AND org_id = $2 AND is_active = TRUE",
    )
    .bind(id)
    .bind(claims.org_id)
    .fetch_optional(&mut *tx)
    .await?;

    let Some(before) = before else {
        let still_exists: Option<(i32,)> = sqlx::query_as(
            "SELECT id FROM maintenance_schedules WHERE id = $1 AND org_id = $2",
        )
        .bind(id)
        .bind(claims.org_id)
        .fetch_optional(&mut *tx)
        .await?;
        tx.commit().await?;
        if still_exists.is_none() {
            return Err(AppError::NotFound(format!(
                "Maintenance schedule {id} not found"
            )));
        }
        return Ok(Json(serde_json::json!({
            "message": "Maintenance schedule already inactive",
            "id": id
        })));
    };

    let after = sqlx::query_as::<_, MaintenanceSchedule>(
        "UPDATE maintenance_schedules SET is_active = FALSE \
         WHERE id = $1 AND org_id = $2 RETURNING *",
    )
    .bind(id)
    .bind(claims.org_id)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;

    audit::log_update_diff(
        &pool,
        claims.org_id,
        claims.user_id,
        &claims.user_code,
        "maintenance_schedules",
        id,
        &before,
        &after,
    )
    .await;

    Ok(Json(serde_json::json!({
        "message": "Maintenance schedule deactivated",
        "id": id
    })))
}

#[derive(Debug, Deserialize)]
pub struct SeedDefaultsRequest {
    /// Seed defaults for a single aircraft.
    pub aircraft_id: Option<i32>,
    /// Seed every aircraft in the org that currently has zero schedules.
    /// Mutually exclusive with `aircraft_id` — if both are present,
    /// `aircraft_id` wins.
    pub seed_all: Option<bool>,
}

#[derive(Debug, serde::Serialize)]
pub struct SeedDefaultsResponse {
    pub aircraft_seeded: i64,
    pub aircraft_skipped_had_schedules: i64,
    /// Aircraft flagged `needs_manual_sn = TRUE` — skipped pending operator
    /// confirmation of the SN via the pending-review UI. Tracked separately
    /// so the admin sees the count rather than it being silently absorbed
    /// into another bucket.
    #[serde(default)]
    pub aircraft_skipped_needs_manual_sn: i64,
    pub aircraft_skipped_no_model: i64,
    /// Aircraft whose model has no active templates in the catalog.
    /// Counted separately from `skipped_no_model` so the admin can tell
    /// "no model" from "no templates seeded yet for this model".
    pub aircraft_skipped_no_templates: i64,
    pub total_schedules_created: i64,
}

/// POST /api/v1/maintenance-schedules/seed-defaults — admin-only.
///
/// Body accepts either `{ "aircraft_id": 42 }` to seed a single aircraft
/// or `{ "seed_all": true }` to seed every aircraft in the org that has
/// zero schedules. The seeder is idempotent per aircraft (see
/// `insert_defaults_for_aircraft`): aircraft with any existing schedule
/// are skipped regardless of how they got there.
pub async fn seed_defaults(
    claims: Claims,
    State(pool): State<DbPool>,
    Json(body): Json<SeedDefaultsRequest>,
) -> Result<Json<SeedDefaultsResponse>, AppError> {
    require_admin(&claims)?;

    #[derive(sqlx::FromRow)]
    struct AircraftRow {
        id: i32,
        model_id: Option<i32>,
    }

    // Target set: either a single aircraft or every aircraft in the org.
    let targets: Vec<AircraftRow> = if let Some(id) = body.aircraft_id {
        sqlx::query_as::<_, AircraftRow>(
            "SELECT id, model_id FROM aircraft \
             WHERE id = $1 AND org_id = $2",
        )
        .bind(id)
        .bind(claims.org_id)
        .fetch_all(&pool)
        .await?
    } else if body.seed_all.unwrap_or(false) {
        sqlx::query_as::<_, AircraftRow>(
            "SELECT id, model_id FROM aircraft WHERE org_id = $1",
        )
        .bind(claims.org_id)
        .fetch_all(&pool)
        .await?
    } else {
        return Err(AppError::BadRequest(
            "Request must include either `aircraft_id` or `seed_all: true`".into()
        ));
    };

    if targets.is_empty() {
        return Err(AppError::NotFound("No matching aircraft found in this organization".into()));
    }

    let mut seeded = 0i64;
    let mut skipped_had = 0i64;
    let mut skipped_needs_manual_sn = 0i64;
    let mut skipped_no_model = 0i64;
    let mut skipped_no_templates = 0i64;
    let mut total_inserted = 0i64;

    for ac in &targets {
        // Each aircraft in its own transaction so a per-row failure
        // rolls back just that one — consistent with batch-import
        // semantics elsewhere. The function itself reports SkippedNoModel
        // / SkippedNoTemplates / SkippedHadSchedules through its return
        // value; we no longer pre-filter on model_id here.
        let mut tx = pool.begin().await?;
        let outcome = crate::services::maintenance_defaults::insert_defaults_for_aircraft(
            &mut tx,
            claims.org_id,
            ac.id,
            ac.model_id,
            &claims.user_code,
        )
        .await;
        match outcome {
            Ok(crate::services::maintenance_defaults::InsertDefaultsOutcome::Inserted(n)) => {
                tx.commit().await?;
                seeded += 1;
                total_inserted += n as i64;
            }
            Ok(crate::services::maintenance_defaults::InsertDefaultsOutcome::SkippedHadSchedules) => {
                tx.rollback().await?;
                skipped_had += 1;
            }
            Ok(crate::services::maintenance_defaults::InsertDefaultsOutcome::SkippedNeedsManualSn) => {
                tx.rollback().await?;
                skipped_needs_manual_sn += 1;
            }
            Ok(crate::services::maintenance_defaults::InsertDefaultsOutcome::SkippedNoModel) => {
                tx.rollback().await?;
                skipped_no_model += 1;
            }
            Ok(crate::services::maintenance_defaults::InsertDefaultsOutcome::SkippedNoTemplates) => {
                tx.rollback().await?;
                skipped_no_templates += 1;
            }
            Err(e) => {
                tx.rollback().await?;
                tracing::warn!("seed-defaults: failed for aircraft {}: {:?}", ac.id, e);
                return Err(e);
            }
        }
    }

    Ok(Json(SeedDefaultsResponse {
        aircraft_seeded: seeded,
        aircraft_skipped_had_schedules: skipped_had,
        aircraft_skipped_needs_manual_sn: skipped_needs_manual_sn,
        aircraft_skipped_no_model: skipped_no_model,
        aircraft_skipped_no_templates: skipped_no_templates,
        total_schedules_created: total_inserted,
    }))
}
