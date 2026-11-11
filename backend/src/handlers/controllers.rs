// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

use axum::{
    extract::{Path, Query, State},
    Json,
};
use crate::auth::middleware::{require_admin_or_technician, Claims};
use crate::db::DbPool;
use crate::errors::AppError;
use crate::models::controller::{Controller, CreateControllerRequest, UpdateControllerRequest};
use crate::services::audit;

#[cfg(test)]
#[path = "controllers_tests.rs"]
mod tests;

#[derive(serde::Deserialize, Default)]
pub struct FleetQueryParams {
    pub include_retired: Option<bool>,
}

/// GET /api/v1/controllers — List all remote controllers for the authenticated organisation.
pub async fn list_controllers(
    claims: Claims,
    State(pool): State<DbPool>,
    Query(params): Query<FleetQueryParams>,
) -> Result<Json<Vec<Controller>>, AppError> {
    let include_retired = params.include_retired.unwrap_or(false);
    let rows = if include_retired {
        sqlx::query_as::<_, Controller>(
            "SELECT * FROM remote_controllers WHERE org_id = $1 ORDER BY COALESCE(nickname, serial_number)"
        )
        .bind(claims.org_id)
        .fetch_all(&pool)
        .await?
    } else {
        sqlx::query_as::<_, Controller>(
            "SELECT * FROM remote_controllers WHERE org_id = $1 AND status != 'retired' ORDER BY COALESCE(nickname, serial_number)"
        )
        .bind(claims.org_id)
        .fetch_all(&pool)
        .await?
    };

    Ok(Json(rows))
}

/// GET /api/v1/controllers/:id — Get a single controller by ID.
pub async fn get_controller(
    claims: Claims,
    State(pool): State<DbPool>,
    Path(id): Path<i32>,
) -> Result<Json<Controller>, AppError> {
    let row = sqlx::query_as::<_, Controller>(
        "SELECT * FROM remote_controllers WHERE id = $1 AND org_id = $2"
    )
    .bind(id)
    .bind(claims.org_id)
    .fetch_optional(&pool)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Controller {} not found", id)))?;

    Ok(Json(row))
}

/// POST /api/v1/controllers — Create a new remote controller.
pub async fn create_controller(
    claims: Claims,
    State(pool): State<DbPool>,
    Json(body): Json<CreateControllerRequest>,
) -> Result<Json<Controller>, AppError> {
    require_admin_or_technician(&claims)?;
    if body.serial_number.trim().is_empty() {
        return Err(AppError::BadRequest("serial_number is required".into()));
    }

    let row = sqlx::query_as::<_, Controller>(
        "INSERT INTO remote_controllers (org_id, serial_number, equipment_id, model, nickname, aircraft_id, notes, compatible_models, status) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,'active') RETURNING *"
    )
    .bind(claims.org_id)
    .bind(body.serial_number.trim())
    .bind(&body.equipment_id)
    .bind(&body.model)
    .bind(&body.nickname)
    .bind(&body.aircraft_id)
    .bind(&body.notes)
    .bind(&body.compatible_models)
    .fetch_one(&pool)
    .await?;

    audit::log_insert_full(&pool, claims.org_id, claims.user_id, &claims.user_code, "remote_controllers", row.id, &row).await;

    Ok(Json(row))
}

/// PUT /api/v1/controllers/:id — Update an existing controller.
pub async fn update_controller(
    claims: Claims,
    State(pool): State<DbPool>,
    Path(id): Path<i32>,
    Json(body): Json<UpdateControllerRequest>,
) -> Result<Json<Controller>, AppError> {
    require_admin_or_technician(&claims)?;
    let mut tx = pool.begin().await?;
    let before = sqlx::query_as::<_, Controller>(
        "SELECT * FROM remote_controllers WHERE id = $1 AND org_id = $2"
    )
    .bind(id)
    .bind(claims.org_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Controller {} not found", id)))?;

    // aircraft_id is tri-state: omitted (None) keeps the current mount, an
    // explicit null (Some(None)) clears it, a value (Some(Some(id))) sets it.
    let (set_aircraft, aircraft_val): (bool, Option<i32>) = match body.aircraft_id {
        None => (false, None),
        Some(v) => (true, v),
    };

    let after = sqlx::query_as::<_, Controller>(
        "UPDATE remote_controllers SET equipment_id = COALESCE($1, equipment_id), model = COALESCE($2, model), nickname = COALESCE($3, nickname), aircraft_id = CASE WHEN $4 THEN $5 ELSE aircraft_id END, status = COALESCE($6, status), notes = COALESCE($7, notes), compatible_models = COALESCE($8, compatible_models), retirement_notes = COALESCE($9, retirement_notes) WHERE id = $10 AND org_id = $11 RETURNING *"
    )
    .bind(&body.equipment_id)
    .bind(&body.model)
    .bind(&body.nickname)
    .bind(set_aircraft)
    .bind(aircraft_val)
    .bind(&body.status)
    .bind(&body.notes)
    .bind(&body.compatible_models)
    .bind(&body.retirement_notes)
    .bind(id)
    .bind(claims.org_id)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;

    audit::log_update_diff(&pool, claims.org_id, claims.user_id, &claims.user_code, "remote_controllers", after.id, &before, &after).await;

    Ok(Json(after))
}

/// DELETE /api/v1/controllers/:id — Retire controller by setting status to 'retired'.
/// NOTE: logged with action=UPDATE (soft status change, not a hard delete).
/// Older audit rows for this handler may still say DELETE.
pub async fn retire_controller(
    claims: Claims,
    State(pool): State<DbPool>,
    Path(id): Path<i32>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_admin_or_technician(&claims)?;
    let mut tx = pool.begin().await?;
    let before = sqlx::query_as::<_, Controller>(
        "SELECT * FROM remote_controllers WHERE id = $1 AND org_id = $2"
    )
    .bind(id)
    .bind(claims.org_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Controller {} not found", id)))?;

    let after = sqlx::query_as::<_, Controller>(
        "UPDATE remote_controllers SET status = 'retired' WHERE id = $1 AND org_id = $2 RETURNING *"
    )
    .bind(id)
    .bind(claims.org_id)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;

    audit::log_update_diff(&pool, claims.org_id, claims.user_id, &claims.user_code, "remote_controllers", id, &before, &after).await;

    Ok(Json(serde_json::json!({ "message": "Controller retired", "id": id })))
}
