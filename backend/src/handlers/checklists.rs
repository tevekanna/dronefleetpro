// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

//! Per-model operating checklists (Normal Ops + QRH) with nested sections
//! and items. One checklist per (model_id, type); `type` is part of the
//! checklist's identity — to change it, delete and recreate.
//!
//! Permission split (matches the model_documents convention):
//!   - `list` and `get`        — any authenticated role.
//!   - `create`, `update`,
//!     `delete`                — admin only.
//!
//! Update semantics: replace-style. PUT submits the full nested payload;
//! the server runs DELETE-sections-CASCADE + INSERT-fresh inside one tx,
//! so child IDs are not stable across updates. The frontend editor holds
//! state locally and submits the entire intended shape on save.
//!
//! Audit: every mutating action writes a `log_custom_action` row with the
//! full nested ChecklistFull snapshot in `details` (UPDATE additionally
//! carries before+after under `before`/`after` keys). The legacy audit
//! `INSERT`/`UPDATE`/`DELETE` shapes don't fit because the entity spans
//! three tables — operators want one audit row per logical change.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use sqlx::{Postgres, Row, Transaction};

use crate::auth::middleware::{require_admin, Claims};
use crate::db::DbPool;
use crate::errors::AppError;
use crate::models::checklist::{
    Checklist, ChecklistFull, ChecklistInputBody, ChecklistItem, ChecklistItemInput,
    ChecklistSection, ChecklistSectionFull, ChecklistSectionInput, ChecklistSummary,
};
use crate::services::audit;

#[cfg(test)]
#[path = "checklists_tests.rs"]
mod tests;

const VALID_TYPES: &[&str] = &["normal_ops", "qrh"];
const VALID_RESPONSIBILITIES: &[&str] = &["PF", "PM", "PF/PM", "N/A"];

/// Confirms the aircraft_models row belongs to the caller's org; returns
/// 404 (not 403) so cross-org IDs don't leak existence. Matches the
/// pattern in `model_documents::ensure_model_in_org`.
async fn ensure_model_in_org(pool: &DbPool, model_id: i32, org_id: i32) -> Result<(), AppError> {
    let row = sqlx::query("SELECT 1 FROM aircraft_models WHERE id = $1 AND org_id = $2")
        .bind(model_id)
        .bind(org_id)
        .fetch_optional(pool)
        .await?;
    if row.is_none() {
        return Err(AppError::NotFound(format!(
            "Aircraft model {} not found",
            model_id
        )));
    }
    Ok(())
}

/// Server-side input validation. The DB CHECK constraints catch the same
/// violations, but pre-validating in Rust gives the caller a clean 400
/// with a specific message instead of a generic Database error.
fn validate_input(body: &ChecklistInputBody) -> Result<(), AppError> {
    if !VALID_TYPES.contains(&body.r#type.as_str()) {
        return Err(AppError::BadRequest(format!(
            "type must be one of: {:?}",
            VALID_TYPES
        )));
    }
    if body.title.trim().is_empty() {
        return Err(AppError::BadRequest("title must not be empty".into()));
    }
    for (si, section) in body.sections.iter().enumerate() {
        if section.title.trim().is_empty() {
            return Err(AppError::BadRequest(format!(
                "sections[{}].title must not be empty",
                si
            )));
        }
        for (ii, item) in section.items.iter().enumerate() {
            if item.label.trim().is_empty() {
                return Err(AppError::BadRequest(format!(
                    "sections[{}].items[{}].label must not be empty",
                    si, ii
                )));
            }
            if !VALID_RESPONSIBILITIES.contains(&item.responsibility.as_str()) {
                return Err(AppError::BadRequest(format!(
                    "sections[{}].items[{}].responsibility must be one of: {:?}",
                    si, ii, VALID_RESPONSIBILITIES
                )));
            }
        }
    }
    Ok(())
}

/// Re-fetch the full nested payload for a checklist that lives in the
/// caller's org. Shared by GET, POST-response, and PUT-response paths.
async fn fetch_full(
    pool: &DbPool,
    checklist_id: i32,
    org_id: i32,
) -> Result<ChecklistFull, AppError> {
    let checklist = sqlx::query_as::<_, Checklist>(
        "SELECT * FROM checklists WHERE id = $1 AND org_id = $2",
    )
    .bind(checklist_id)
    .bind(org_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Checklist {} not found", checklist_id)))?;

    let sections = sqlx::query_as::<_, ChecklistSection>(
        "SELECT id, checklist_id, title, sort_order, start_on_new_page \
         FROM checklist_sections WHERE checklist_id = $1 ORDER BY sort_order, id",
    )
    .bind(checklist.id)
    .fetch_all(pool)
    .await?;

    let section_ids: Vec<i32> = sections.iter().map(|s| s.id).collect();
    let items = sqlx::query_as::<_, ChecklistItem>(
        "SELECT id, section_id, label, value, responsibility, sort_order \
         FROM checklist_items WHERE section_id = ANY($1) ORDER BY sort_order, id",
    )
    .bind(&section_ids)
    .fetch_all(pool)
    .await?;

    let mut sections_full: Vec<ChecklistSectionFull> = sections
        .into_iter()
        .map(|s| ChecklistSectionFull {
            section: s,
            items: Vec::new(),
        })
        .collect();
    for item in items {
        if let Some(s) = sections_full.iter_mut().find(|s| s.section.id == item.section_id) {
            s.items.push(item);
        }
    }

    Ok(ChecklistFull {
        checklist,
        sections: sections_full,
    })
}

/// Insert the sections + items for a checklist inside an existing tx.
/// Used by both `create` (fresh row) and `update` (after wiping prior
/// children via CASCADE).
async fn insert_sections_and_items(
    tx: &mut Transaction<'_, Postgres>,
    checklist_id: i32,
    sections: &[ChecklistSectionInput],
) -> Result<(), AppError> {
    for section in sections {
        let row = sqlx::query(
            "INSERT INTO checklist_sections (checklist_id, title, sort_order, start_on_new_page) \
             VALUES ($1, $2, $3, $4) RETURNING id",
        )
        .bind(checklist_id)
        .bind(section.title.trim())
        .bind(section.sort_order)
        .bind(section.start_on_new_page)
        .fetch_one(&mut **tx)
        .await?;
        let section_id: i32 = row.get("id");

        for item in &section.items {
            insert_item(tx, section_id, item).await?;
        }
    }
    Ok(())
}

async fn insert_item(
    tx: &mut Transaction<'_, Postgres>,
    section_id: i32,
    item: &ChecklistItemInput,
) -> Result<(), AppError> {
    sqlx::query(
        "INSERT INTO checklist_items (section_id, label, value, responsibility, sort_order) \
         VALUES ($1, $2, $3, $4, $5)",
    )
    .bind(section_id)
    .bind(item.label.trim())
    .bind(&item.value)
    .bind(&item.responsibility)
    .bind(item.sort_order)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

// ---------------------------------------------------------------------------
// GET /api/v1/aircraft-models/:id/checklists
// ---------------------------------------------------------------------------

pub async fn list(
    claims: Claims,
    State(pool): State<DbPool>,
    Path(model_id): Path<i32>,
) -> Result<Json<Vec<ChecklistSummary>>, AppError> {
    ensure_model_in_org(&pool, model_id, claims.org_id).await?;

    let rows = sqlx::query(
        "SELECT c.id, c.model_id, c.type, c.title, c.revision, c.revision_date, c.updated_at, \
                COUNT(DISTINCT s.id) AS section_count, \
                COUNT(i.id)          AS item_count \
         FROM checklists c \
         LEFT JOIN checklist_sections s ON s.checklist_id = c.id \
         LEFT JOIN checklist_items i    ON i.section_id   = s.id \
         WHERE c.model_id = $1 AND c.org_id = $2 \
         GROUP BY c.id \
         ORDER BY c.type, c.title",
    )
    .bind(model_id)
    .bind(claims.org_id)
    .fetch_all(&pool)
    .await?;

    let summaries: Vec<ChecklistSummary> = rows
        .into_iter()
        .map(|r| ChecklistSummary {
            id: r.get("id"),
            model_id: r.get("model_id"),
            r#type: r.get("type"),
            title: r.get("title"),
            revision: r.get("revision"),
            revision_date: r.get("revision_date"),
            updated_at: r.get("updated_at"),
            section_count: r.get("section_count"),
            item_count: r.get("item_count"),
        })
        .collect();

    Ok(Json(summaries))
}

// ---------------------------------------------------------------------------
// GET /api/v1/checklists/:id
// ---------------------------------------------------------------------------

pub async fn get(
    claims: Claims,
    State(pool): State<DbPool>,
    Path(checklist_id): Path<i32>,
) -> Result<Json<ChecklistFull>, AppError> {
    let full = fetch_full(&pool, checklist_id, claims.org_id).await?;
    Ok(Json(full))
}

// ---------------------------------------------------------------------------
// POST /api/v1/aircraft-models/:id/checklists
// ---------------------------------------------------------------------------

pub async fn create(
    claims: Claims,
    State(pool): State<DbPool>,
    Path(model_id): Path<i32>,
    Json(body): Json<ChecklistInputBody>,
) -> Result<Json<ChecklistFull>, AppError> {
    require_admin(&claims)?;
    validate_input(&body)?;
    ensure_model_in_org(&pool, model_id, claims.org_id).await?;

    let mut tx = pool.begin().await?;

    // Pre-flight UNIQUE(model_id, type) check inside the tx so a conflict
    // surfaces as a clean 409. Mirrors aircraft_models::create.
    let clash = sqlx::query(
        "SELECT 1 FROM checklists WHERE model_id = $1 AND type = $2",
    )
    .bind(model_id)
    .bind(&body.r#type)
    .fetch_optional(&mut *tx)
    .await?;
    if clash.is_some() {
        return Err(AppError::Conflict(format!(
            "A {} checklist already exists for this model.",
            body.r#type
        )));
    }

    let revision = body.revision.clone().unwrap_or_else(|| "01".into());
    let row = sqlx::query(
        "INSERT INTO checklists \
           (org_id, model_id, type, title, revision, revision_date, created_by, updated_by) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $7) RETURNING id",
    )
    .bind(claims.org_id)
    .bind(model_id)
    .bind(&body.r#type)
    .bind(body.title.trim())
    .bind(&revision)
    .bind(body.revision_date)
    .bind(claims.user_id)
    .fetch_one(&mut *tx)
    .await?;
    let new_id: i32 = row.get("id");

    insert_sections_and_items(&mut tx, new_id, &body.sections).await?;

    tx.commit().await?;

    let full = fetch_full(&pool, new_id, claims.org_id).await?;
    audit::log_custom_action(
        &pool,
        claims.org_id,
        claims.user_id,
        &claims.user_code,
        "CREATE_CHECKLIST",
        "checklists",
        new_id,
        serde_json::json!({
            "model_id": model_id,
            "snapshot": &full,
        }),
    )
    .await?;

    Ok(Json(full))
}

// ---------------------------------------------------------------------------
// PUT /api/v1/checklists/:id
// ---------------------------------------------------------------------------

pub async fn update(
    claims: Claims,
    State(pool): State<DbPool>,
    Path(checklist_id): Path<i32>,
    Json(body): Json<ChecklistInputBody>,
) -> Result<Json<ChecklistFull>, AppError> {
    require_admin(&claims)?;
    validate_input(&body)?;

    // Snapshot the existing state for the audit `before`. fetch_full also
    // 404s if the row is missing or out-of-org.
    let before_snapshot = fetch_full(&pool, checklist_id, claims.org_id).await?;

    if before_snapshot.checklist.r#type != body.r#type {
        return Err(AppError::BadRequest(format!(
            "type is part of the checklist's identity and cannot be changed via update \
             (existing: '{}', requested: '{}'). Delete and recreate instead.",
            before_snapshot.checklist.r#type, body.r#type
        )));
    }

    let mut tx = pool.begin().await?;

    // Take a row lock to prevent a concurrent update from racing with the
    // DELETE-children + INSERT-children steps below.
    sqlx::query("SELECT id FROM checklists WHERE id = $1 AND org_id = $2 FOR UPDATE")
        .bind(checklist_id)
        .bind(claims.org_id)
        .fetch_one(&mut *tx)
        .await?;

    let revision = body
        .revision
        .clone()
        .unwrap_or_else(|| before_snapshot.checklist.revision.clone());

    sqlx::query(
        "UPDATE checklists \
            SET title = $1, revision = $2, revision_date = $3, \
                updated_by = $4, updated_at = now() \
          WHERE id = $5",
    )
    .bind(body.title.trim())
    .bind(&revision)
    .bind(body.revision_date)
    .bind(claims.user_id)
    .bind(checklist_id)
    .execute(&mut *tx)
    .await?;

    sqlx::query("DELETE FROM checklist_sections WHERE checklist_id = $1")
        .bind(checklist_id)
        .execute(&mut *tx)
        .await?;

    insert_sections_and_items(&mut tx, checklist_id, &body.sections).await?;

    tx.commit().await?;

    let after_snapshot = fetch_full(&pool, checklist_id, claims.org_id).await?;
    audit::log_custom_action(
        &pool,
        claims.org_id,
        claims.user_id,
        &claims.user_code,
        "UPDATE_CHECKLIST",
        "checklists",
        checklist_id,
        serde_json::json!({
            "before": &before_snapshot,
            "after": &after_snapshot,
        }),
    )
    .await?;

    Ok(Json(after_snapshot))
}

// ---------------------------------------------------------------------------
// DELETE /api/v1/checklists/:id
// ---------------------------------------------------------------------------

pub async fn delete(
    claims: Claims,
    State(pool): State<DbPool>,
    Path(checklist_id): Path<i32>,
) -> Result<Response, AppError> {
    require_admin(&claims)?;

    // Snapshot first; 404 if it doesn't exist in this org.
    let snapshot = fetch_full(&pool, checklist_id, claims.org_id).await?;

    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM checklists WHERE id = $1 AND org_id = $2")
        .bind(checklist_id)
        .bind(claims.org_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;

    audit::log_custom_action(
        &pool,
        claims.org_id,
        claims.user_id,
        &claims.user_code,
        "DELETE_CHECKLIST",
        "checklists",
        checklist_id,
        serde_json::json!({
            "model_id": snapshot.checklist.model_id,
            "snapshot": &snapshot,
        }),
    )
    .await?;

    Ok((StatusCode::NO_CONTENT, ()).into_response())
}
