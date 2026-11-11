// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;
use serde_json::json;

use crate::auth::middleware::{require_admin, Claims};
use crate::db::DbPool;
use crate::errors::AppError;
use crate::models::maintenance_schedule::MaintenanceSchedule;
use crate::models::maintenance_schedule_template::{
    CreateScheduleTemplateRequest, MaintenanceScheduleTemplate,
    MaintenanceScheduleTemplateWithCount, SyncTemplateRequest, SyncTemplateResponse,
    UpdateScheduleTemplateRequest,
};
use crate::services::audit;
use crate::services::schedule_calc::{
    compute_next_due, compute_secondary_next_due, validate_primary_type, validate_secondary,
    DEFAULT_WARNING_THRESHOLD,
};

const NAME_MAX_LEN: usize = 100;

#[derive(Debug, Deserialize)]
pub struct ListQueryParams {
    pub model_id: Option<i32>,
    pub include_inactive: Option<bool>,
}

fn validate_name(name: &str) -> Result<(), AppError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(AppError::BadRequest("name is required".into()));
    }
    if trimmed.chars().count() > NAME_MAX_LEN {
        return Err(AppError::BadRequest(format!(
            "name must be {NAME_MAX_LEN} characters or fewer"
        )));
    }
    Ok(())
}

fn validate_warning_threshold(t: f32) -> Result<(), AppError> {
    if !(0.0 < t && t <= 1.0) {
        return Err(AppError::BadRequest(
            "warning_threshold must be > 0 and <= 1".into(),
        ));
    }
    Ok(())
}

async fn model_belongs_to_org(
    pool: &DbPool,
    model_id: i32,
    org_id: i32,
) -> Result<bool, AppError> {
    let exists: Option<(i32,)> =
        sqlx::query_as("SELECT id FROM aircraft_models WHERE id = $1 AND org_id = $2")
            .bind(model_id)
            .bind(org_id)
            .fetch_optional(pool)
            .await?;
    Ok(exists.is_some())
}

/// GET /api/v1/maintenance-schedule-templates
///
/// Each row includes `reference_count`: the number of `maintenance_schedules`
/// rows whose `name` matches the template's name AND whose aircraft has the
/// template's `model_id`. This mirrors the scope `sync_template` operates on,
/// so it is the right "is this template in use?" count for the
/// permanent-delete guard on the UI.
pub async fn list_templates(
    claims: Claims,
    State(pool): State<DbPool>,
    Query(params): Query<ListQueryParams>,
) -> Result<Json<Vec<MaintenanceScheduleTemplateWithCount>>, AppError> {
    let include_inactive = params.include_inactive.unwrap_or(false);
    let rows = sqlx::query_as::<_, MaintenanceScheduleTemplateWithCount>(
        "SELECT t.*, \
            COALESCE(( \
              SELECT COUNT(*) FROM maintenance_schedules s \
              JOIN aircraft a ON s.aircraft_id = a.id \
              WHERE s.org_id = t.org_id \
                AND s.name = t.name \
                AND a.model_id = t.model_id \
            ), 0)::bigint AS reference_count \
         FROM maintenance_schedule_templates t \
         WHERE t.org_id = $1 \
           AND ($2::int IS NULL OR t.model_id = $2) \
           AND ($3::bool OR t.is_active = TRUE) \
         ORDER BY t.model_id, t.name",
    )
    .bind(claims.org_id)
    .bind(params.model_id)
    .bind(include_inactive)
    .fetch_all(&pool)
    .await?;
    Ok(Json(rows))
}

/// GET /api/v1/maintenance-schedule-templates/:id
pub async fn get_template(
    claims: Claims,
    State(pool): State<DbPool>,
    Path(id): Path<i32>,
) -> Result<Json<MaintenanceScheduleTemplate>, AppError> {
    let row = sqlx::query_as::<_, MaintenanceScheduleTemplate>(
        "SELECT * FROM maintenance_schedule_templates WHERE id = $1 AND org_id = $2",
    )
    .bind(id)
    .bind(claims.org_id)
    .fetch_optional(&pool)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Schedule template {id} not found")))?;
    Ok(Json(row))
}

/// POST /api/v1/maintenance-schedule-templates
pub async fn create_template(
    claims: Claims,
    State(pool): State<DbPool>,
    Json(body): Json<CreateScheduleTemplateRequest>,
) -> Result<Json<MaintenanceScheduleTemplate>, AppError> {
    require_admin(&claims)?;

    validate_name(&body.name)?;
    validate_primary_type(&body.interval_type)?;
    if body.interval_value <= 0.0 {
        return Err(AppError::BadRequest(
            "interval_value must be greater than 0".into(),
        ));
    }
    let warning_threshold = body
        .warning_threshold
        .unwrap_or(DEFAULT_WARNING_THRESHOLD);
    validate_warning_threshold(warning_threshold)?;
    validate_secondary(
        body.secondary_interval_type.as_deref(),
        body.secondary_interval_value,
    )?;

    if !model_belongs_to_org(&pool, body.model_id, claims.org_id).await? {
        return Err(AppError::BadRequest(format!(
            "model_id {} not found in this organisation",
            body.model_id
        )));
    }

    let row = sqlx::query_as::<_, MaintenanceScheduleTemplate>(
        "INSERT INTO maintenance_schedule_templates (\
            org_id, model_id, name, description, interval_type, interval_value, \
            warning_threshold, is_critical, secondary_interval_type, \
            secondary_interval_value, is_active, created_by\
         ) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,TRUE,$11) \
         RETURNING *",
    )
    .bind(claims.org_id)
    .bind(body.model_id)
    .bind(body.name.trim())
    .bind(&body.description)
    .bind(&body.interval_type)
    .bind(body.interval_value)
    .bind(warning_threshold)
    .bind(body.is_critical.unwrap_or(false))
    .bind(&body.secondary_interval_type)
    .bind(body.secondary_interval_value)
    .bind(&claims.user_code)
    .fetch_one(&pool)
    .await?;

    audit::log_insert_full(
        &pool,
        claims.org_id,
        claims.user_id,
        &claims.user_code,
        "maintenance_schedule_templates",
        row.id,
        &row,
    )
    .await;

    Ok(Json(row))
}

/// PUT /api/v1/maintenance-schedule-templates/:id
pub async fn update_template(
    claims: Claims,
    State(pool): State<DbPool>,
    Path(id): Path<i32>,
    Json(body): Json<UpdateScheduleTemplateRequest>,
) -> Result<Json<MaintenanceScheduleTemplate>, AppError> {
    require_admin(&claims)?;

    let existing = sqlx::query_as::<_, MaintenanceScheduleTemplate>(
        "SELECT * FROM maintenance_schedule_templates WHERE id = $1 AND org_id = $2",
    )
    .bind(id)
    .bind(claims.org_id)
    .fetch_optional(&pool)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Schedule template {id} not found")))?;

    if let Some(ref n) = body.name {
        validate_name(n)?;
    }
    if let Some(ref t) = body.interval_type {
        validate_primary_type(t)?;
    }
    if let Some(v) = body.interval_value {
        if v <= 0.0 {
            return Err(AppError::BadRequest(
                "interval_value must be greater than 0".into(),
            ));
        }
    }
    if let Some(t) = body.warning_threshold {
        validate_warning_threshold(t)?;
    }
    let new_secondary_type = body
        .secondary_interval_type
        .clone()
        .or_else(|| existing.secondary_interval_type.clone());
    let new_secondary_value = body
        .secondary_interval_value
        .or(existing.secondary_interval_value);
    validate_secondary(new_secondary_type.as_deref(), new_secondary_value)?;

    let new_name = body
        .name
        .as_deref()
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| existing.name.clone());
    let new_description = body
        .description
        .clone()
        .or_else(|| existing.description.clone());
    let new_interval_type = body
        .interval_type
        .clone()
        .unwrap_or_else(|| existing.interval_type.clone());
    let new_interval_value = body.interval_value.unwrap_or(existing.interval_value);
    let new_warning_threshold = body
        .warning_threshold
        .unwrap_or(existing.warning_threshold);
    let new_is_critical = body.is_critical.unwrap_or(existing.is_critical);
    let new_is_active = body.is_active.unwrap_or(existing.is_active);

    let row = sqlx::query_as::<_, MaintenanceScheduleTemplate>(
        "UPDATE maintenance_schedule_templates SET \
            name = $1, description = $2, interval_type = $3, interval_value = $4, \
            warning_threshold = $5, is_critical = $6, \
            secondary_interval_type = $7, secondary_interval_value = $8, \
            is_active = $9 \
         WHERE id = $10 AND org_id = $11 RETURNING *",
    )
    .bind(&new_name)
    .bind(&new_description)
    .bind(&new_interval_type)
    .bind(new_interval_value)
    .bind(new_warning_threshold)
    .bind(new_is_critical)
    .bind(&new_secondary_type)
    .bind(new_secondary_value)
    .bind(new_is_active)
    .bind(id)
    .bind(claims.org_id)
    .fetch_one(&pool)
    .await?;

    audit::log_update_diff(
        &pool,
        claims.org_id,
        claims.user_id,
        &claims.user_code,
        "maintenance_schedule_templates",
        row.id,
        &existing,
        &row,
    )
    .await;

    Ok(Json(row))
}

/// DELETE /api/v1/maintenance-schedule-templates/:id — soft delete.
/// Sets `is_active = FALSE`. Existing aircraft schedules spawned from
/// this template are NOT touched; admin can re-activate the template
/// or run sync after editing.
pub async fn delete_template(
    claims: Claims,
    State(pool): State<DbPool>,
    Path(id): Path<i32>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_admin(&claims)?;

    let existing = sqlx::query_as::<_, MaintenanceScheduleTemplate>(
        "SELECT * FROM maintenance_schedule_templates \
         WHERE id = $1 AND org_id = $2 AND is_active = TRUE",
    )
    .bind(id)
    .bind(claims.org_id)
    .fetch_optional(&pool)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Active schedule template {id} not found")))?;

    let updated = sqlx::query_as::<_, MaintenanceScheduleTemplate>(
        "UPDATE maintenance_schedule_templates SET is_active = FALSE \
         WHERE id = $1 AND org_id = $2 RETURNING *",
    )
    .bind(id)
    .bind(claims.org_id)
    .fetch_one(&pool)
    .await?;

    audit::log_update_diff(
        &pool,
        claims.org_id,
        claims.user_id,
        &claims.user_code,
        "maintenance_schedule_templates",
        updated.id,
        &existing,
        &updated,
    )
    .await;

    Ok(Json(json!({ "deleted": true, "id": id })))
}

/// POST /api/v1/maintenance-schedule-templates/:id/sync
///
/// Updates each affected aircraft's `maintenance_schedules` row whose
/// `name` matches the template, replacing interval / warning threshold
/// / secondary axis / criticality with the template's current values.
/// `next_due_*` is recomputed from the (unchanged) baselines.
///
/// Aircraft without a schedule of the same name are silently skipped —
/// counted in `skipped_no_schedule`. The action is admin-only and
/// audited per-row via the standard update-diff helper.
pub async fn sync_template(
    claims: Claims,
    State(pool): State<DbPool>,
    Path(id): Path<i32>,
    Json(body): Json<SyncTemplateRequest>,
) -> Result<Json<SyncTemplateResponse>, AppError> {
    require_admin(&claims)?;

    let template = sqlx::query_as::<_, MaintenanceScheduleTemplate>(
        "SELECT * FROM maintenance_schedule_templates WHERE id = $1 AND org_id = $2",
    )
    .bind(id)
    .bind(claims.org_id)
    .fetch_optional(&pool)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Schedule template {id} not found")))?;

    // Resolve the candidate aircraft set:
    //   - With aircraft_ids: keep only ids that belong to org AND have
    //     model_id matching the template's model. This stops sync from
    //     bleeding into unrelated aircraft when caller passes an ID
    //     from another model.
    //   - Without aircraft_ids: every aircraft in the org with the
    //     template's model_id.
    let candidate_aircraft: Vec<i32> =
        match body.aircraft_ids.as_ref().map(|v| v.as_slice()) {
            Some(ids) if !ids.is_empty() => sqlx::query_as::<_, (i32,)>(
                "SELECT id FROM aircraft \
                 WHERE org_id = $1 AND model_id = $2 AND id = ANY($3)",
            )
            .bind(claims.org_id)
            .bind(template.model_id)
            .bind(ids)
            .fetch_all(&pool)
            .await?
            .into_iter()
            .map(|(i,)| i)
            .collect(),
            _ => sqlx::query_as::<_, (i32,)>(
                "SELECT id FROM aircraft WHERE org_id = $1 AND model_id = $2",
            )
            .bind(claims.org_id)
            .bind(template.model_id)
            .fetch_all(&pool)
            .await?
            .into_iter()
            .map(|(i,)| i)
            .collect(),
        };

    let mut synced = 0i64;
    let mut skipped_no_schedule = 0i64;

    for aircraft_id in candidate_aircraft {
        // Find the schedule for this aircraft + template name.
        let before = sqlx::query_as::<_, MaintenanceSchedule>(
            "SELECT * FROM maintenance_schedules \
             WHERE org_id = $1 AND aircraft_id = $2 AND name = $3",
        )
        .bind(claims.org_id)
        .bind(aircraft_id)
        .bind(&template.name)
        .fetch_optional(&pool)
        .await?;

        let Some(before) = before else {
            skipped_no_schedule += 1;
            continue;
        };

        // Recompute next_due from the unchanged baselines using the
        // template's new interval values.
        let last_h = if before.last_performed_flight_hours > 0.0 {
            Some(before.last_performed_flight_hours)
        } else {
            None
        };
        let last_c = if before.last_performed_cycles > 0 {
            Some(before.last_performed_cycles)
        } else {
            None
        };
        let (next_due_at, next_due_flight_hours, next_due_cycles) = compute_next_due(
            &template.interval_type,
            template.interval_value,
            before.last_performed_at,
            last_h,
            last_c,
        );
        let (secondary_next_due_at, secondary_next_due_hours) = compute_secondary_next_due(
            template.secondary_interval_type.as_deref(),
            template.secondary_interval_value,
            before.last_performed_at,
            last_h,
        );

        let updated = sqlx::query_as::<_, MaintenanceSchedule>(
            "UPDATE maintenance_schedules SET \
                description = $1, \
                interval_type = $2, interval_value = $3, \
                warning_threshold = $4, is_critical = $5, \
                secondary_interval_type = $6, secondary_interval_value = $7, \
                next_due_at = $8, next_due_flight_hours = $9, next_due_cycles = $10, \
                secondary_next_due_at = $11, secondary_next_due_hours = $12 \
             WHERE id = $13 AND org_id = $14 \
             RETURNING *",
        )
        .bind(&template.description)
        .bind(&template.interval_type)
        .bind(template.interval_value)
        .bind(template.warning_threshold)
        .bind(template.is_critical)
        .bind(&template.secondary_interval_type)
        .bind(template.secondary_interval_value)
        .bind(next_due_at)
        .bind(next_due_flight_hours)
        .bind(next_due_cycles)
        .bind(secondary_next_due_at)
        .bind(secondary_next_due_hours)
        .bind(before.id)
        .bind(claims.org_id)
        .fetch_one(&pool)
        .await?;

        audit::log_update_diff(
            &pool,
            claims.org_id,
            claims.user_id,
            &claims.user_code,
            "maintenance_schedules",
            updated.id,
            &before,
            &updated,
        )
        .await;

        synced += 1;
    }

    Ok(Json(SyncTemplateResponse {
        synced,
        skipped_no_schedule,
        schedules_updated: synced,
    }))
}

/// POST /api/v1/maintenance-schedule-templates/:id/permanent-delete
///
/// Hard-deletes a template row only if no `maintenance_schedules` were
/// spawned from it (matched by name + model). Returns 409 Conflict with
/// a structured message if any schedules reference it; the caller is
/// expected to remove or rename those schedules first.
///
/// Distinct from the DELETE verb on this resource, which performs a
/// soft retire (is_active = FALSE).
pub async fn permanent_delete(
    claims: Claims,
    State(pool): State<DbPool>,
    Path(id): Path<i32>,
) -> Result<Json<MaintenanceScheduleTemplate>, AppError> {
    require_admin(&claims)?;

    let mut tx = pool.begin().await?;

    let template = sqlx::query_as::<_, MaintenanceScheduleTemplate>(
        "SELECT * FROM maintenance_schedule_templates WHERE id = $1 AND org_id = $2",
    )
    .bind(id)
    .bind(claims.org_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Schedule template {id} not found")))?;

    let schedule_count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*)::bigint FROM maintenance_schedules s \
         JOIN aircraft a ON s.aircraft_id = a.id \
         WHERE s.org_id = $1 AND s.name = $2 AND a.model_id = $3",
    )
    .bind(claims.org_id)
    .bind(&template.name)
    .bind(template.model_id)
    .fetch_one(&mut *tx)
    .await?;

    if schedule_count.0 > 0 {
        return Err(AppError::Conflict(format!(
            "Cannot delete template '{}': {} schedules reference it. Remove or reassign these first.",
            template.name, schedule_count.0
        )));
    }

    sqlx::query("DELETE FROM maintenance_schedule_templates WHERE id = $1 AND org_id = $2")
        .bind(id)
        .bind(claims.org_id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    audit::log_delete_full(
        &pool,
        claims.org_id,
        claims.user_id,
        &claims.user_code,
        "maintenance_schedule_templates",
        id,
        &template,
    )
    .await;

    Ok(Json(template))
}
