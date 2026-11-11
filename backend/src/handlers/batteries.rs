// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

use axum::{
    extract::{Path, Query, State},
    Json,
};
use crate::auth::middleware::{require_admin_or_technician, Claims};
use crate::db::DbPool;
use crate::errors::AppError;
use crate::models::battery::{Battery, CreateBatteryRequest, UpdateBatteryRequest};
use crate::services::audit;

#[cfg(test)]
#[path = "batteries_tests.rs"]
mod tests;

#[derive(serde::Deserialize, Default)]
pub struct FleetQueryParams {
    pub include_retired: Option<bool>,
}

/// Measured per-unit battery mass must be strictly positive when present (NaN
/// rejected too). An omitted field keeps the existing value via COALESCE on
/// update. Mirrors the aircraft `empty_weight_kg` guard.
fn validate_weight(weight_kg: Option<f32>) -> Result<(), AppError> {
    if let Some(w) = weight_kg {
        if !(w > 0.0) {
            return Err(AppError::BadRequest(
                "weight_kg must be greater than 0 when provided".into(),
            ));
        }
    }
    Ok(())
}

/// GET /api/v1/batteries — List all batteries for the authenticated organisation.
pub async fn list_batteries(
    claims: Claims,
    State(pool): State<DbPool>,
    Query(params): Query<FleetQueryParams>,
) -> Result<Json<Vec<Battery>>, AppError> {
    let include_retired = params.include_retired.unwrap_or(false);
    let rows = if include_retired {
        sqlx::query_as::<_, Battery>(
            "SELECT * FROM batteries WHERE org_id = $1 ORDER BY COALESCE(equipment_id, serial_number)"
        )
        .bind(claims.org_id)
        .fetch_all(&pool)
        .await?
    } else {
        sqlx::query_as::<_, Battery>(
            "SELECT * FROM batteries WHERE org_id = $1 AND status != 'retired' ORDER BY COALESCE(equipment_id, serial_number)"
        )
        .bind(claims.org_id)
        .fetch_all(&pool)
        .await?
    };

    Ok(Json(rows))
}

/// GET /api/v1/batteries/:id — Get a single battery by ID.
pub async fn get_battery(
    claims: Claims,
    State(pool): State<DbPool>,
    Path(id): Path<i32>,
) -> Result<Json<Battery>, AppError> {
    let row = sqlx::query_as::<_, Battery>(
        "SELECT * FROM batteries WHERE id = $1 AND org_id = $2"
    )
    .bind(id)
    .bind(claims.org_id)
    .fetch_optional(&pool)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Battery {} not found", id)))?;

    Ok(Json(row))
}

/// POST /api/v1/batteries — Create a new battery.
pub async fn create_battery(
    claims: Claims,
    State(pool): State<DbPool>,
    Json(body): Json<CreateBatteryRequest>,
) -> Result<Json<Battery>, AppError> {
    require_admin_or_technician(&claims)?;
    if body.serial_number.trim().is_empty() {
        return Err(AppError::BadRequest("serial_number is required".into()));
    }
    validate_weight(body.weight_kg)?;

    let row = sqlx::query_as::<_, Battery>(
        "INSERT INTO batteries (org_id, serial_number, equipment_id, aircraft_id, capacity_wh, weight_kg, max_cycles, compatible_model, compatible_model_id, status) VALUES ($1,$2,$3,$4,$5,$6,COALESCE($7,300),$8,$9,'active') RETURNING *"
    )
    .bind(claims.org_id)
    .bind(body.serial_number.trim())
    .bind(&body.equipment_id)
    .bind(&body.aircraft_id)
    .bind(&body.capacity_wh)
    .bind(&body.weight_kg)
    .bind(&body.max_cycles)
    .bind(&body.compatible_model)
    .bind(&body.compatible_model_id)
    .fetch_one(&pool)
    .await
    .map_err(|e| {
        tracing::error!("create_battery DB error: {:?}", e);
        AppError::Database(e)
    })?;

    audit::log_insert_full(&pool, claims.org_id, claims.user_id, &claims.user_code, "batteries", row.id, &row).await;

    Ok(Json(row))
}

/// PUT /api/v1/batteries/:id — Update an existing battery.
pub async fn update_battery(
    claims: Claims,
    State(pool): State<DbPool>,
    Path(id): Path<i32>,
    Json(body): Json<UpdateBatteryRequest>,
) -> Result<Json<Battery>, AppError> {
    require_admin_or_technician(&claims)?;
    validate_weight(body.weight_kg)?;
    let mut tx = pool.begin().await?;
    let before = sqlx::query_as::<_, Battery>(
        "SELECT * FROM batteries WHERE id = $1 AND org_id = $2"
    )
    .bind(id)
    .bind(claims.org_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Battery {} not found", id)))?;

    // aircraft_id is tri-state: omitted (None) keeps the current mount, an
    // explicit null (Some(None)) clears it, a value (Some(Some(id))) sets it.
    let (set_aircraft, aircraft_val): (bool, Option<i32>) = match body.aircraft_id {
        None => (false, None),
        Some(v) => (true, v),
    };

    // compatible_model_id is tri-state: omitted (None) keeps the current
    // value, an explicit null (Some(None)) clears it, a value sets it.
    let (set_compat, compat_val): (bool, Option<i32>) = match body.compatible_model_id {
        None => (false, None),
        Some(v) => (true, v),
    };

    let after = sqlx::query_as::<_, Battery>(
        "UPDATE batteries SET equipment_id = COALESCE($1, equipment_id), aircraft_id = CASE WHEN $2 THEN $3 ELSE aircraft_id END, capacity_wh = COALESCE($4, capacity_wh), weight_kg = COALESCE($5, weight_kg), max_cycles = COALESCE($6, max_cycles), compatible_model = COALESCE($7, compatible_model), compatible_model_id = CASE WHEN $8 THEN $9 ELSE compatible_model_id END, status = COALESCE($10, status), retirement_notes = COALESCE($11, retirement_notes) WHERE id = $12 AND org_id = $13 RETURNING *"
    )
    .bind(&body.equipment_id)
    .bind(set_aircraft)
    .bind(aircraft_val)
    .bind(&body.capacity_wh)
    .bind(&body.weight_kg)
    .bind(&body.max_cycles)
    .bind(&body.compatible_model)
    .bind(set_compat)
    .bind(compat_val)
    .bind(&body.status)
    .bind(&body.retirement_notes)
    .bind(id)
    .bind(claims.org_id)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;

    audit::log_update_diff(&pool, claims.org_id, claims.user_id, &claims.user_code, "batteries", after.id, &before, &after).await;

    Ok(Json(after))
}

/// DELETE /api/v1/batteries/:id — Retire battery by setting status to 'retired'.
/// NOTE: logged with action=UPDATE (soft status change, not a hard delete).
/// Older audit rows for this handler may still say DELETE.
pub async fn retire_battery(
    claims: Claims,
    State(pool): State<DbPool>,
    Path(id): Path<i32>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_admin_or_technician(&claims)?;
    let mut tx = pool.begin().await?;
    let before = sqlx::query_as::<_, Battery>(
        "SELECT * FROM batteries WHERE id = $1 AND org_id = $2"
    )
    .bind(id)
    .bind(claims.org_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Battery {} not found", id)))?;

    let after = sqlx::query_as::<_, Battery>(
        "UPDATE batteries SET status = 'retired' WHERE id = $1 AND org_id = $2 RETURNING *"
    )
    .bind(id)
    .bind(claims.org_id)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;

    audit::log_update_diff(&pool, claims.org_id, claims.user_id, &claims.user_code, "batteries", id, &before, &after).await;

    Ok(Json(serde_json::json!({ "message": "Battery retired", "id": id })))
}
