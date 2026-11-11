// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;

use crate::auth::middleware::{require_admin, require_admin_or_technician, require_role, Claims};
use crate::db::DbPool;
use crate::errors::AppError;
use crate::models::defect::{DefectRecord, CreateDefectRequest, UpdateDefectRequest};
use crate::services::audit;

#[cfg(test)]
#[path = "defects_tests.rs"]
mod tests;

#[derive(Debug, Deserialize)]
pub struct ListDefectsQuery {
    #[serde(default)]
    pub aircraft_id: Option<i32>,
}

/// GET /api/v1/defects — List defect records for the org.
///
/// Optional `?aircraft_id=N` filters the list to defects logged against that
/// specific airframe. When omitted, returns every defect in the org (the
/// historical behaviour).
///
/// Note: unlike `maintenance`, the `defect_log` table has no `controller_id`
/// column (defects are aircraft-only by schema). The query shape therefore
/// diverges from `maintenance::list_maintenance` by intent.
pub async fn list_defects(
    claims: Claims,
    State(pool): State<DbPool>,
    Query(query): Query<ListDefectsQuery>,
) -> Result<Json<Vec<DefectRecord>>, AppError> {
    let mut sql = String::from("SELECT * FROM defect_log WHERE org_id = $1");
    if query.aircraft_id.is_some() {
        sql.push_str(" AND aircraft_id = $2");
    }
    sql.push_str(" ORDER BY defect_date DESC, created_at DESC");

    let mut q = sqlx::query_as::<_, DefectRecord>(&sql).bind(claims.org_id);
    if let Some(id) = query.aircraft_id {
        q = q.bind(id);
    }

    let rows = q.fetch_all(&pool).await?;
    Ok(Json(rows))
}

/// POST /api/v1/defects — Report a new defect.
pub async fn create_defect(
    claims: Claims,
    State(pool): State<DbPool>,
    Json(body): Json<CreateDefectRequest>,
) -> Result<Json<DefectRecord>, AppError> {
    require_role(
        &claims,
        &["admin", "safety_officer", "technician", "pilot"],
    )?;
    if body.description.trim().is_empty() {
        return Err(AppError::BadRequest("description is required".into()));
    }
    if body.reported_by.len() != 3 || !body.reported_by.chars().all(|c| c.is_ascii_uppercase()) {
        return Err(AppError::BadRequest("reported_by must be exactly 3 uppercase letters".into()));
    }

    let row = sqlx::query_as::<_, DefectRecord>(
        "INSERT INTO defect_log (org_id, aircraft_id, flight_id, defect_date, reported_by, description, status) VALUES ($1,$2,$3,$4::date,$5,$6,'open') RETURNING *"
    )
    .bind(claims.org_id)
    .bind(body.aircraft_id)
    .bind(body.flight_id)
    .bind(&body.defect_date)
    .bind(&body.reported_by)
    .bind(&body.description)
    .fetch_one(&pool)
    .await
    .map_err(|e| {
        tracing::error!("create_defect DB error: {:?}", e);
        AppError::Database(e)
    })?;

    audit::log_insert_full(&pool, claims.org_id, claims.user_id, &claims.user_code, "defect_log", row.id, &row).await;

    Ok(Json(row))
}

/// PUT /api/v1/defects/:id — Update defect status (close a defect).
pub async fn update_defect_status(
    claims: Claims,
    State(pool): State<DbPool>,
    Path(id): Path<i32>,
    Json(body): Json<UpdateDefectRequest>,
) -> Result<Json<DefectRecord>, AppError> {
    require_admin_or_technician(&claims)?;
    let mut tx = pool.begin().await?;
    let before = sqlx::query_as::<_, DefectRecord>(
        "SELECT * FROM defect_log WHERE id = $1 AND org_id = $2"
    )
    .bind(id)
    .bind(claims.org_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Defect {} not found", id)))?;

    let after = sqlx::query_as::<_, DefectRecord>(
        "UPDATE defect_log SET status = $1, action_taken = COALESCE($2, action_taken), closed_by = COALESCE($3, closed_by), closed_date = CASE WHEN $1 = 'closed' THEN CURRENT_DATE ELSE closed_date END WHERE id = $4 AND org_id = $5 RETURNING *"
    )
    .bind(&body.status)
    .bind(&body.action_taken)
    .bind(&body.closed_by)
    .bind(id)
    .bind(claims.org_id)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;

    audit::log_update_diff(&pool, claims.org_id, claims.user_id, &claims.user_code, "defect_log", after.id, &before, &after).await;

    Ok(Json(after))
}

/// DELETE /api/v1/defects/:id — Delete a defect record.
pub async fn delete_defect(
    claims: Claims,
    State(pool): State<DbPool>,
    Path(id): Path<i32>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_admin(&claims)?;
    let mut tx = pool.begin().await?;
    let before = sqlx::query_as::<_, DefectRecord>(
        "SELECT * FROM defect_log WHERE id = $1 AND org_id = $2"
    )
    .bind(id)
    .bind(claims.org_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Defect {} not found", id)))?;

    sqlx::query("DELETE FROM defect_log WHERE id = $1 AND org_id = $2")
        .bind(id)
        .bind(claims.org_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;

    audit::log_delete_full(&pool, claims.org_id, claims.user_id, &claims.user_code, "defect_log", id, &before).await;

    Ok(Json(serde_json::json!({ "message": "Defect deleted", "id": id })))
}
