// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

//! Per-org aircraft_models catalog. View endpoints (list with aggregated
//! stats + per-model stats detail) are open to any authenticated user;
//! CRUD endpoints (create / update / merge / retire / permanent_delete)
//! and the bare `GET /:id` (used by ModelFormModal for editing) remain
//! admin-only.

use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;
use sqlx::Row;

use crate::auth::middleware::{require_admin, Claims};
use crate::db::DbPool;
use crate::errors::AppError;
use crate::models::aircraft_model::{
    AircraftModel, AircraftModelIndividual, AircraftModelStatsDetail, AircraftModelWithStats,
    CreateAircraftModelRequest, MergeAircraftModelRequest, UpdateAircraftModelRequest,
};
use crate::services::audit;

#[cfg(test)]
#[path = "aircraft_models_tests.rs"]
mod tests;

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    pub is_active: Option<bool>,
}

/// Shared column + JOIN definition for both `list` and `get_stats`. Aggregates
/// over the denormalized counters on `aircraft` (total_flights, total_seconds,
/// last_flight) — no JOIN to `flights` is required.
const MODEL_STATS_SELECT: &str = "\
SELECT \
    am.id, am.org_id, am.name, am.manufacturer, am.category, am.is_active, \
    am.notes, am.default_max_flight_time_minutes, am.default_weight_kg, \
    am.mtom_kg, am.empty_weight_kg, am.max_payload_kg, am.class_marking, \
    am.max_dimension_cm, am.design_approval_required, \
    am.created_at, am.updated_at, \
    COUNT(a.id)::bigint                                              AS aircraft_count, \
    COUNT(a.id) FILTER (WHERE a.status = 'active')::bigint           AS active_count, \
    COUNT(a.id) FILTER (WHERE a.status = 'maintenance')::bigint      AS maintenance_count, \
    COUNT(a.id) FILTER (WHERE a.status = 'retired')::bigint          AS retired_count, \
    COALESCE(SUM(a.total_flights), 0)::bigint                        AS total_flights, \
    COALESCE(SUM(a.total_seconds), 0)::bigint                        AS total_flight_seconds, \
    MAX(a.last_flight)                                               AS last_flight \
FROM aircraft_models am \
LEFT JOIN aircraft a ON a.model_id = am.id";

/// GET /api/v1/aircraft-models
///
/// Returns every model in the caller's org with aggregated flight stats.
/// Open to any authenticated user.
///
/// Optional query parameter:
/// - `is_active=true` — only active models
/// - `is_active=false` — only retired models
/// - omitted — both
pub async fn list(
    claims: Claims,
    State(pool): State<DbPool>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<AircraftModelWithStats>>, AppError> {
    let sql = format!(
        "{select} \
         WHERE am.org_id = $1 \
           AND ($2::boolean IS NULL OR am.is_active = $2) \
         GROUP BY am.id \
         ORDER BY am.is_active DESC, LOWER(am.name)",
        select = MODEL_STATS_SELECT,
    );

    let rows = sqlx::query_as::<_, AircraftModelWithStats>(&sql)
        .bind(claims.org_id)
        .bind(q.is_active)
        .fetch_all(&pool)
        .await?;

    Ok(Json(rows))
}

/// GET /api/v1/aircraft-models/:id/stats
///
/// Returns the same aggregated row as `list` for a single model, plus the
/// full list of aircraft individuals belonging to that model. Open to any
/// authenticated user. Returns 404 (not 403) when the model is in a
/// different org so callers can't probe cross-org existence.
pub async fn get_stats(
    claims: Claims,
    State(pool): State<DbPool>,
    Path(id): Path<i32>,
) -> Result<Json<AircraftModelStatsDetail>, AppError> {
    let model_sql = format!(
        "{select} \
         WHERE am.id = $1 AND am.org_id = $2 \
         GROUP BY am.id",
        select = MODEL_STATS_SELECT,
    );

    let model = sqlx::query_as::<_, AircraftModelWithStats>(&model_sql)
        .bind(id)
        .bind(claims.org_id)
        .fetch_optional(&pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Aircraft model {} not found", id)))?;

    let individuals = sqlx::query_as::<_, AircraftModelIndividual>(
        "SELECT id, serial_number, equipment_id, nickname, status, \
                total_flights, total_seconds, last_flight, needs_manual_sn \
         FROM aircraft \
         WHERE model_id = $1 AND org_id = $2 \
         ORDER BY status, equipment_id NULLS LAST, serial_number",
    )
    .bind(id)
    .bind(claims.org_id)
    .fetch_all(&pool)
    .await?;

    Ok(Json(AircraftModelStatsDetail { model, individuals }))
}

/// GET /api/v1/aircraft-models/:id
pub async fn get(
    claims: Claims,
    State(pool): State<DbPool>,
    Path(id): Path<i32>,
) -> Result<Json<AircraftModel>, AppError> {
    require_admin(&claims)?;

    let row = sqlx::query_as::<_, AircraftModel>(
        "SELECT * FROM aircraft_models WHERE id = $1 AND org_id = $2"
    )
    .bind(id)
    .bind(claims.org_id)
    .fetch_optional(&pool)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Aircraft model {} not found", id)))?;

    Ok(Json(row))
}

/// Validate the regulatory weight / dimension / class-marking fields
/// (migration 049). All numeric fields must be strictly positive when
/// present (NaN is rejected too); `class_marking` must be a known EASA
/// marking; and if BOTH `mtom_kg` and `empty_weight_kg` are present,
/// `mtom_kg` must be >= `empty_weight_kg` — an airframe cannot have a
/// maximum take-off mass below its own empty weight.
///
/// On update the caller passes the *effective* values (request value if
/// provided, else the existing row value) so a partial update that lowers
/// MTOM below the stored empty weight is still rejected.
fn validate_weights(
    mtom_kg: Option<f32>,
    empty_weight_kg: Option<f32>,
    max_payload_kg: Option<f32>,
    max_dimension_cm: Option<f32>,
    class_marking: Option<&str>,
) -> Result<(), AppError> {
    for (label, value) in [
        ("mtom_kg", mtom_kg),
        ("empty_weight_kg", empty_weight_kg),
        ("max_payload_kg", max_payload_kg),
        ("max_dimension_cm", max_dimension_cm),
    ] {
        if let Some(v) = value {
            if !(v > 0.0) {
                return Err(AppError::BadRequest(format!(
                    "{label} must be greater than 0 when provided"
                )));
            }
        }
    }

    if let Some(cm) = class_marking {
        if !matches!(cm, "C0" | "C1" | "C2" | "C3" | "C4" | "C5" | "C6") {
            return Err(AppError::BadRequest(
                "class_marking must be one of C0, C1, C2, C3, C4, C5, C6".into(),
            ));
        }
    }

    if let (Some(mtom), Some(empty)) = (mtom_kg, empty_weight_kg) {
        if mtom < empty {
            return Err(AppError::BadRequest(format!(
                "mtom_kg ({mtom}) must be greater than or equal to empty_weight_kg ({empty})"
            )));
        }
    }

    Ok(())
}

/// POST /api/v1/aircraft-models
pub async fn create(
    claims: Claims,
    State(pool): State<DbPool>,
    Json(body): Json<CreateAircraftModelRequest>,
) -> Result<Json<AircraftModel>, AppError> {
    require_admin(&claims)?;

    let name = body.name.trim();
    if name.is_empty() {
        return Err(AppError::BadRequest("name is required".into()));
    }

    // Check for a case-insensitive name collision first so we can return a
    // friendly 409 before the INSERT fails on the unique index.
    let existing = sqlx::query(
        "SELECT id FROM aircraft_models WHERE org_id = $1 AND LOWER(name) = LOWER($2)"
    )
    .bind(claims.org_id)
    .bind(name)
    .fetch_optional(&pool)
    .await?;
    if existing.is_some() {
        return Err(AppError::Conflict(format!(
            "A model named '{}' already exists in this organization.", name
        )));
    }

    let class_marking = body
        .class_marking
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    validate_weights(
        body.mtom_kg,
        body.empty_weight_kg,
        body.max_payload_kg,
        body.max_dimension_cm,
        class_marking,
    )?;

    if let Some(ref v) = body.design_approval_required {
        if !matches!(v.as_str(), "required" | "not_required") {
            return Err(AppError::BadRequest(
                "design_approval_required must be 'required' or 'not_required' (or null for not determined)"
                    .into(),
            ));
        }
    }

    let row = sqlx::query_as::<_, AircraftModel>(
        "INSERT INTO aircraft_models \
           (org_id, name, manufacturer, category, notes, default_max_flight_time_minutes, default_weight_kg, \
            mtom_kg, empty_weight_kg, max_payload_kg, class_marking, max_dimension_cm, design_approval_required) \
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13) RETURNING *"
    )
    .bind(claims.org_id)
    .bind(name)
    .bind(body.manufacturer.as_deref().map(str::trim).filter(|s| !s.is_empty()))
    .bind(body.category.as_deref().map(str::trim).filter(|s| !s.is_empty()))
    .bind(body.notes.as_deref().map(str::trim).filter(|s| !s.is_empty()))
    .bind(body.default_max_flight_time_minutes)
    .bind(body.default_weight_kg)
    .bind(body.mtom_kg)
    .bind(body.empty_weight_kg)
    .bind(body.max_payload_kg)
    .bind(class_marking)
    .bind(body.max_dimension_cm)
    .bind(&body.design_approval_required)
    .fetch_one(&pool)
    .await?;

    audit::log_insert_full(
        &pool, claims.org_id, claims.user_id, &claims.user_code,
        "aircraft_models", row.id, &row
    ).await;

    Ok(Json(row))
}

/// PUT /api/v1/aircraft-models/:id
pub async fn update(
    claims: Claims,
    State(pool): State<DbPool>,
    Path(id): Path<i32>,
    Json(body): Json<UpdateAircraftModelRequest>,
) -> Result<Json<AircraftModel>, AppError> {
    require_admin(&claims)?;

    let mut tx = pool.begin().await?;
    let before = sqlx::query_as::<_, AircraftModel>(
        "SELECT * FROM aircraft_models WHERE id = $1 AND org_id = $2"
    )
    .bind(id)
    .bind(claims.org_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Aircraft model {} not found", id)))?;

    // Case-insensitive collision check if the caller is renaming.
    if let Some(ref new_name) = body.name {
        let trimmed = new_name.trim();
        if trimmed.is_empty() {
            return Err(AppError::BadRequest("name cannot be empty".into()));
        }
        if !trimmed.eq_ignore_ascii_case(&before.name) {
            let clash = sqlx::query(
                "SELECT id FROM aircraft_models \
                 WHERE org_id = $1 AND LOWER(name) = LOWER($2) AND id != $3"
            )
            .bind(claims.org_id)
            .bind(trimmed)
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?;
            if clash.is_some() {
                return Err(AppError::Conflict(format!(
                    "A model named '{}' already exists in this organization.", trimmed
                )));
            }
        }
    }

    // Validate the *effective* post-update state (request value if provided,
    // else the existing row value) so a partial update that lowers MTOM below
    // the stored empty weight — or vice versa — is still rejected. Fields are
    // COALESCE'd below, so an omitted field keeps its existing value.
    let class_marking = body
        .class_marking
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    validate_weights(
        body.mtom_kg.or(before.mtom_kg),
        body.empty_weight_kg.or(before.empty_weight_kg),
        body.max_payload_kg.or(before.max_payload_kg),
        body.max_dimension_cm.or(before.max_dimension_cm),
        class_marking.or(before.class_marking.as_deref()),
    )?;

    // design_approval_required is tri-state (double_option): omitted keeps
    // the stored value, explicit null clears back to NOT DETERMINED, a value
    // must be one of the two CHECK-constrained states.
    let (set_dar, dar_val) = match body.design_approval_required {
        None => (false, None),
        Some(v) => (true, v),
    };
    if let Some(ref v) = dar_val {
        if !matches!(v.as_str(), "required" | "not_required") {
            return Err(AppError::BadRequest(
                "design_approval_required must be 'required' or 'not_required' (or null to clear)"
                    .into(),
            ));
        }
    }

    let after = sqlx::query_as::<_, AircraftModel>(
        "UPDATE aircraft_models SET \
           name = COALESCE($1, name), \
           manufacturer = COALESCE($2, manufacturer), \
           category = COALESCE($3, category), \
           is_active = COALESCE($4, is_active), \
           notes = COALESCE($5, notes), \
           default_max_flight_time_minutes = COALESCE($6, default_max_flight_time_minutes), \
           default_weight_kg = COALESCE($7, default_weight_kg), \
           mtom_kg = COALESCE($8, mtom_kg), \
           empty_weight_kg = COALESCE($9, empty_weight_kg), \
           max_payload_kg = COALESCE($10, max_payload_kg), \
           class_marking = COALESCE($11, class_marking), \
           max_dimension_cm = COALESCE($12, max_dimension_cm), \
           design_approval_required = CASE WHEN $13 THEN $14 ELSE design_approval_required END, \
           updated_at = NOW() \
         WHERE id = $15 AND org_id = $16 \
         RETURNING *"
    )
    .bind(body.name.as_deref().map(str::trim))
    .bind(body.manufacturer.as_deref().map(str::trim))
    .bind(body.category.as_deref().map(str::trim))
    .bind(body.is_active)
    .bind(body.notes.as_deref().map(str::trim))
    .bind(body.default_max_flight_time_minutes)
    .bind(body.default_weight_kg)
    .bind(body.mtom_kg)
    .bind(body.empty_weight_kg)
    .bind(body.max_payload_kg)
    .bind(class_marking)
    .bind(body.max_dimension_cm)
    .bind(set_dar)
    .bind(dar_val)
    .bind(id)
    .bind(claims.org_id)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;

    audit::log_update_diff(
        &pool, claims.org_id, claims.user_id, &claims.user_code,
        "aircraft_models", after.id, &before, &after,
    ).await;

    Ok(Json(after))
}

/// DELETE /api/v1/aircraft-models/:id — soft retire (is_active = FALSE).
///
/// Hard deletion is not available here: models with dependent aircraft or
/// drone_type_mappings stay as history. If the caller wants to collapse
/// duplicates, the merge endpoint is the way.
pub async fn retire(
    claims: Claims,
    State(pool): State<DbPool>,
    Path(id): Path<i32>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_admin(&claims)?;

    let mut tx = pool.begin().await?;
    let before = sqlx::query_as::<_, AircraftModel>(
        "SELECT * FROM aircraft_models WHERE id = $1 AND org_id = $2"
    )
    .bind(id)
    .bind(claims.org_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Aircraft model {} not found", id)))?;

    let after = sqlx::query_as::<_, AircraftModel>(
        "UPDATE aircraft_models SET is_active = FALSE, updated_at = NOW() \
         WHERE id = $1 AND org_id = $2 RETURNING *"
    )
    .bind(id)
    .bind(claims.org_id)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;

    audit::log_update_diff(
        &pool, claims.org_id, claims.user_id, &claims.user_code,
        "aircraft_models", id, &before, &after,
    ).await;

    Ok(Json(serde_json::json!({
        "message": "Aircraft model retired", "id": id
    })))
}

/// POST /api/v1/aircraft-models/:id/merge
///
/// Moves all aircraft and drone_type_mappings from the source model to the
/// target, then hard-deletes the source row. Done inside a single
/// transaction; `drone_type_mappings` rows that would collide with an
/// existing (org, app_source, drone_type_code) on the target are deleted
/// from the source side rather than moved.
pub async fn merge(
    claims: Claims,
    State(pool): State<DbPool>,
    Path(source_id): Path<i32>,
    Json(body): Json<MergeAircraftModelRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_admin(&claims)?;

    if source_id == body.target_id {
        return Err(AppError::BadRequest("Cannot merge a model into itself.".into()));
    }

    let mut tx = pool.begin().await?;

    let source = sqlx::query_as::<_, AircraftModel>(
        "SELECT * FROM aircraft_models WHERE id = $1 AND org_id = $2"
    )
    .bind(source_id)
    .bind(claims.org_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Source model {} not found", source_id)))?;

    let target = sqlx::query_as::<_, AircraftModel>(
        "SELECT * FROM aircraft_models WHERE id = $1 AND org_id = $2"
    )
    .bind(body.target_id)
    .bind(claims.org_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Target model {} not found", body.target_id)))?;

    if !target.is_active {
        return Err(AppError::BadRequest(
            "Target model is retired; cannot merge into a retired model.".into(),
        ));
    }

    // Move aircraft.
    let aircraft_moved: i64 = sqlx::query(
        "WITH moved AS ( \
           UPDATE aircraft SET model_id = $1 WHERE model_id = $2 AND org_id = $3 RETURNING id \
         ) SELECT COUNT(*)::bigint AS n FROM moved"
    )
    .bind(body.target_id)
    .bind(source_id)
    .bind(claims.org_id)
    .fetch_one(&mut *tx)
    .await?
    .get::<i64, _>("n");

    // drone_type_mappings: delete rows that would collide with an existing
    // target-side row on (org, app_source, drone_type_code), then move the rest.
    let mappings_deleted: i64 = sqlx::query(
        "WITH deleted AS ( \
           DELETE FROM drone_type_mappings src \
           WHERE src.model_id = $1 AND src.org_id = $2 \
             AND EXISTS ( \
               SELECT 1 FROM drone_type_mappings t \
               WHERE t.org_id = src.org_id \
                 AND t.app_source = src.app_source \
                 AND t.drone_type_code = src.drone_type_code \
                 AND t.id <> src.id \
                 AND t.model_id = $3 \
             ) RETURNING id \
         ) SELECT COUNT(*)::bigint AS n FROM deleted"
    )
    .bind(source_id)
    .bind(claims.org_id)
    .bind(body.target_id)
    .fetch_one(&mut *tx)
    .await?
    .get::<i64, _>("n");

    let mappings_moved: i64 = sqlx::query(
        "WITH moved AS ( \
           UPDATE drone_type_mappings SET model_id = $1 \
           WHERE model_id = $2 AND org_id = $3 RETURNING id \
         ) SELECT COUNT(*)::bigint AS n FROM moved"
    )
    .bind(body.target_id)
    .bind(source_id)
    .bind(claims.org_id)
    .fetch_one(&mut *tx)
    .await?
    .get::<i64, _>("n");

    // Hard-delete the source row.
    sqlx::query("DELETE FROM aircraft_models WHERE id = $1 AND org_id = $2")
        .bind(source_id)
        .bind(claims.org_id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    audit::log_delete_full(
        &pool, claims.org_id, claims.user_id, &claims.user_code,
        "aircraft_models", source_id, &source,
    ).await;

    Ok(Json(serde_json::json!({
        "message": "Models merged",
        "source_id": source_id,
        "target_id": body.target_id,
        "source_name": source.name,
        "target_name": target.name,
        "aircraft_moved": aircraft_moved,
        "mappings_moved": mappings_moved,
        "mappings_deleted_as_duplicates": mappings_deleted,
    })))
}

/// POST /api/v1/aircraft-models/:id/permanent-delete
///
/// Hard-deletes a model row only if it has zero downstream references
/// (no aircraft, no drone_type_mappings, no maintenance_schedule_templates).
/// Returns 409 Conflict listing the dependency counts if any exist; the
/// caller is expected to remove or reassign them before retrying.
///
/// Distinct from the DELETE verb on this resource, which performs a
/// soft retire (is_active = FALSE).
pub async fn permanent_delete(
    claims: Claims,
    State(pool): State<DbPool>,
    Path(id): Path<i32>,
) -> Result<Json<AircraftModel>, AppError> {
    require_admin(&claims)?;

    let mut tx = pool.begin().await?;

    let model = sqlx::query_as::<_, AircraftModel>(
        "SELECT * FROM aircraft_models WHERE id = $1 AND org_id = $2"
    )
    .bind(id)
    .bind(claims.org_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Aircraft model {} not found", id)))?;

    let counts = sqlx::query(
        "SELECT \
           (SELECT COUNT(*) FROM aircraft \
              WHERE model_id = $1 AND org_id = $2)::bigint AS aircraft_count, \
           (SELECT COUNT(*) FROM drone_type_mappings \
              WHERE model_id = $1 AND org_id = $2)::bigint AS mapping_count, \
           (SELECT COUNT(*) FROM maintenance_schedule_templates \
              WHERE model_id = $1 AND org_id = $2)::bigint AS template_count"
    )
    .bind(id)
    .bind(claims.org_id)
    .fetch_one(&mut *tx)
    .await?;

    let aircraft_count: i64 = counts.get("aircraft_count");
    let mapping_count: i64 = counts.get("mapping_count");
    let template_count: i64 = counts.get("template_count");

    if aircraft_count > 0 || mapping_count > 0 || template_count > 0 {
        return Err(AppError::Conflict(format!(
            "Cannot delete model '{}': {} aircraft, {} mappings, {} schedule templates reference it. \
             Remove or reassign these first.",
            model.name, aircraft_count, mapping_count, template_count
        )));
    }

    // Unlink document files from disk BEFORE the DB DELETE triggers
    // ON DELETE CASCADE on model_documents. Best-effort — failures are
    // logged but don't block the model delete.
    crate::handlers::model_documents::unlink_files_for_model(&pool, id, claims.org_id).await;

    sqlx::query("DELETE FROM aircraft_models WHERE id = $1 AND org_id = $2")
        .bind(id)
        .bind(claims.org_id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    audit::log_delete_full(
        &pool, claims.org_id, claims.user_id, &claims.user_code,
        "aircraft_models", id, &model,
    ).await;

    Ok(Json(model))
}
