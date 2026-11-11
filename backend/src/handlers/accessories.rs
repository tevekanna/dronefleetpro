// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

use axum::{
    extract::{Path, Query, State},
    Json,
};
use crate::auth::middleware::{require_admin_or_technician, Claims};
use crate::db::DbPool;
use crate::errors::AppError;
use crate::models::accessory::{Accessory, CreateAccessoryRequest, UpdateAccessoryRequest};
use crate::services::audit;

#[cfg(test)]
#[path = "accessories_db_tests.rs"]
mod db_tests;

#[derive(serde::Deserialize, Default)]
pub struct FleetQueryParams {
    pub include_retired: Option<bool>,
}

/// Measured per-unit mass must be strictly positive when present (NaN rejected
/// too). An omitted field keeps the existing value via COALESCE on update.
/// Mirrors the aircraft `empty_weight_kg` guard.
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

/// GET /api/v1/accessories — List all accessories for the authenticated organisation.
pub async fn list_accessories(
    claims: Claims,
    State(pool): State<DbPool>,
    Query(params): Query<FleetQueryParams>,
) -> Result<Json<Vec<Accessory>>, AppError> {
    let include_retired = params.include_retired.unwrap_or(false);
    let rows = if include_retired {
        sqlx::query_as::<_, Accessory>(
            "SELECT * FROM accessories WHERE org_id = $1 ORDER BY COALESCE(equipment_id, name)"
        )
        .bind(claims.org_id)
        .fetch_all(&pool)
        .await?
    } else {
        sqlx::query_as::<_, Accessory>(
            "SELECT * FROM accessories WHERE org_id = $1 AND status != 'retired' ORDER BY COALESCE(equipment_id, name)"
        )
        .bind(claims.org_id)
        .fetch_all(&pool)
        .await?
    };

    Ok(Json(rows))
}

/// GET /api/v1/accessories/:id — Get a single accessory by ID.
pub async fn get_accessory(
    claims: Claims,
    State(pool): State<DbPool>,
    Path(id): Path<i32>,
) -> Result<Json<Accessory>, AppError> {
    let row = sqlx::query_as::<_, Accessory>(
        "SELECT * FROM accessories WHERE id = $1 AND org_id = $2"
    )
    .bind(id)
    .bind(claims.org_id)
    .fetch_optional(&pool)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Accessory {} not found", id)))?;

    Ok(Json(row))
}

/// POST /api/v1/accessories — Create a new accessory.
pub async fn create_accessory(
    claims: Claims,
    State(pool): State<DbPool>,
    Json(body): Json<CreateAccessoryRequest>,
) -> Result<Json<Accessory>, AppError> {
    require_admin_or_technician(&claims)?;
    if body.name.trim().is_empty() {
        return Err(AppError::BadRequest("name is required".into()));
    }
    validate_weight(body.weight_kg)?;

    // serial_number is nullable; normalise an empty/whitespace SN to NULL so
    // the partial-unique index treats "no SN" consistently.
    let serial_number = body
        .serial_number
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    let row = sqlx::query_as::<_, Accessory>(
        "INSERT INTO accessories (org_id, aircraft_id, name, category, manufacturer, serial_number, equipment_id, weight_kg, status) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,'active') RETURNING *"
    )
    .bind(claims.org_id)
    .bind(&body.aircraft_id)
    .bind(body.name.trim())
    .bind(&body.category)
    .bind(&body.manufacturer)
    .bind(&serial_number)
    .bind(&body.equipment_id)
    .bind(&body.weight_kg)
    .fetch_one(&pool)
    .await
    .map_err(|e| {
        tracing::error!("create_accessory DB error: {:?}", e);
        AppError::Database(e)
    })?;

    audit::log_insert_full(&pool, claims.org_id, claims.user_id, &claims.user_code, "accessories", row.id, &row).await;

    Ok(Json(row))
}

/// PUT /api/v1/accessories/:id — Update an existing accessory.
pub async fn update_accessory(
    claims: Claims,
    State(pool): State<DbPool>,
    Path(id): Path<i32>,
    Json(body): Json<UpdateAccessoryRequest>,
) -> Result<Json<Accessory>, AppError> {
    require_admin_or_technician(&claims)?;
    validate_weight(body.weight_kg)?;

    let mut tx = pool.begin().await?;
    let before = sqlx::query_as::<_, Accessory>(
        "SELECT * FROM accessories WHERE id = $1 AND org_id = $2"
    )
    .bind(id)
    .bind(claims.org_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Accessory {} not found", id)))?;

    // aircraft_id is tri-state: omitted (None) keeps the current mount, an
    // explicit null (Some(None)) clears it, a value (Some(Some(id))) sets it.
    let (set_aircraft, aircraft_val): (bool, Option<i32>) = match body.aircraft_id {
        None => (false, None),
        Some(v) => (true, v),
    };

    let after = sqlx::query_as::<_, Accessory>(
        "UPDATE accessories SET name = COALESCE($1, name), aircraft_id = CASE WHEN $2 THEN $3 ELSE aircraft_id END, category = COALESCE($4, category), manufacturer = COALESCE($5, manufacturer), serial_number = COALESCE($6, serial_number), equipment_id = COALESCE($7, equipment_id), weight_kg = COALESCE($8, weight_kg), status = COALESCE($9, status), retirement_notes = COALESCE($10, retirement_notes) WHERE id = $11 AND org_id = $12 RETURNING *"
    )
    .bind(&body.name)
    .bind(set_aircraft)
    .bind(aircraft_val)
    .bind(&body.category)
    .bind(&body.manufacturer)
    .bind(&body.serial_number)
    .bind(&body.equipment_id)
    .bind(&body.weight_kg)
    .bind(&body.status)
    .bind(&body.retirement_notes)
    .bind(id)
    .bind(claims.org_id)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;

    audit::log_update_diff(&pool, claims.org_id, claims.user_id, &claims.user_code, "accessories", after.id, &before, &after).await;

    Ok(Json(after))
}

/// DELETE /api/v1/accessories/:id — Retire accessory by setting status to 'retired'.
/// NOTE: logged with action=UPDATE (soft status change, not a hard delete).
pub async fn retire_accessory(
    claims: Claims,
    State(pool): State<DbPool>,
    Path(id): Path<i32>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_admin_or_technician(&claims)?;
    let mut tx = pool.begin().await?;
    let before = sqlx::query_as::<_, Accessory>(
        "SELECT * FROM accessories WHERE id = $1 AND org_id = $2"
    )
    .bind(id)
    .bind(claims.org_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Accessory {} not found", id)))?;

    let after = sqlx::query_as::<_, Accessory>(
        "UPDATE accessories SET status = 'retired' WHERE id = $1 AND org_id = $2 RETURNING *"
    )
    .bind(id)
    .bind(claims.org_id)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;

    audit::log_update_diff(&pool, claims.org_id, claims.user_id, &claims.user_code, "accessories", id, &before, &after).await;

    Ok(Json(serde_json::json!({ "message": "Accessory retired", "id": id })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_weight_accepts_none_and_positive() {
        assert!(validate_weight(None).is_ok());
        assert!(validate_weight(Some(0.25)).is_ok());
        assert!(validate_weight(Some(1200.0)).is_ok());
    }

    #[test]
    fn validate_weight_rejects_zero_negative_and_nan() {
        assert!(validate_weight(Some(0.0)).is_err());
        assert!(validate_weight(Some(-1.0)).is_err());
        assert!(validate_weight(Some(f32::NAN)).is_err());
    }
}
