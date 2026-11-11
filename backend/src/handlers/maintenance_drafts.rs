// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

use axum::{
    extract::{Path, State},
    Json,
};
use serde_json::json;

use crate::auth::middleware::Claims;
use crate::auth::middleware::require_admin_or_technician;
use crate::db::DbPool;
use crate::errors::AppError;
use crate::models::maintenance::{MaintenanceDraft, UpsertDraftRequest};

/// GET /api/v1/maintenance/drafts/:schedule_id
pub async fn get_draft(
    claims: Claims,
    State(pool): State<DbPool>,
    Path(schedule_id): Path<i32>,
) -> Result<Json<MaintenanceDraft>, AppError> {
    let row = sqlx::query_as::<_, MaintenanceDraft>(
        "SELECT * FROM maintenance_drafts WHERE org_id = $1 AND schedule_id = $2",
    )
    .bind(claims.org_id)
    .bind(schedule_id)
    .fetch_optional(&pool)
    .await?
    .ok_or_else(|| AppError::NotFound("No draft for this schedule".into()))?;

    Ok(Json(row))
}

/// PUT /api/v1/maintenance/drafts/:schedule_id — upsert.
pub async fn upsert_draft(
    claims: Claims,
    State(pool): State<DbPool>,
    Path(schedule_id): Path<i32>,
    Json(body): Json<UpsertDraftRequest>,
) -> Result<Json<MaintenanceDraft>, AppError> {
    require_admin_or_technician(&claims)?;

    let saved_by = body.saved_by.trim();
    if saved_by.is_empty() || saved_by.len() > 10 {
        return Err(AppError::BadRequest(
            "saved_by must be 1–10 characters".into(),
        ));
    }

    let row = sqlx::query_as::<_, MaintenanceDraft>(
        "INSERT INTO maintenance_drafts \
           (org_id, schedule_id, saved_by, form_data, checklist_status, checklist_notes, updated_at) \
         VALUES ($1,$2,$3,$4,$5,$6,NOW()) \
         ON CONFLICT (org_id, schedule_id) \
         DO UPDATE SET \
           saved_by = EXCLUDED.saved_by, \
           form_data = EXCLUDED.form_data, \
           checklist_status = EXCLUDED.checklist_status, \
           checklist_notes = EXCLUDED.checklist_notes, \
           updated_at = NOW() \
         RETURNING *",
    )
    .bind(claims.org_id)
    .bind(schedule_id)
    .bind(saved_by)
    .bind(&body.form_data)
    .bind(&body.checklist_status)
    .bind(&body.checklist_notes)
    .fetch_one(&pool)
    .await?;

    Ok(Json(row))
}

/// DELETE /api/v1/maintenance/drafts/:schedule_id — idempotent.
pub async fn delete_draft(
    claims: Claims,
    State(pool): State<DbPool>,
    Path(schedule_id): Path<i32>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_admin_or_technician(&claims)?;

    sqlx::query("DELETE FROM maintenance_drafts WHERE org_id = $1 AND schedule_id = $2")
        .bind(claims.org_id)
        .bind(schedule_id)
        .execute(&pool)
        .await?;

    Ok(Json(json!({ "deleted": true })))
}
