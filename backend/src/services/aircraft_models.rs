// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

//! Helpers for resolving aircraft model names against the per-org
//! `aircraft_models` catalog. Auto-creation (via `resolve_or_create`) is
//! permitted from the log-import path in `handlers::flights` and from the
//! fleet batch-import flow in `handlers::batch_import` (where the operator
//! has explicitly typed a model name + optional manufacturer); the
//! aircraft-CRUD endpoints use `resolve_by_name_ci` instead and reject
//! unknown names with 400.

use sqlx::{PgPool, Row};

use crate::errors::AppError;

/// Look up an aircraft_models id for `(org_id, name)` case-insensitively.
/// Returns `Ok(None)` if no match exists. `name` is trimmed.
pub async fn resolve_by_name_ci(
    pool: &PgPool,
    org_id: i32,
    name: &str,
) -> Result<Option<i32>, AppError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let row = sqlx::query(
        "SELECT id FROM aircraft_models \
         WHERE org_id = $1 AND LOWER(name) = LOWER($2) LIMIT 1"
    )
    .bind(org_id)
    .bind(trimmed)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(|r| r.get::<i32, _>("id")))
}

/// Same as `resolve_by_name_ci` but creates a new catalog row if none
/// exists. Used by the log-import auto-registration path and the fleet
/// batch-import flow. Returns the id.
///
/// `manufacturer` is honoured only on insert — when the model name already
/// resolves to an existing row, the existing manufacturer (which may be
/// blank or already populated) is preserved. Pass `Some("")` or `None` if
/// no manufacturer hint is available.
///
/// Uses `ON CONFLICT (org_id, LOWER(name)) DO UPDATE` to handle the race
/// where two concurrent imports of the same model both try to insert; the
/// `DO UPDATE SET name = EXCLUDED.name` branch returns the existing id.
pub async fn resolve_or_create(
    pool: &PgPool,
    org_id: i32,
    name: &str,
    manufacturer: Option<&str>,
) -> Result<i32, AppError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(AppError::BadRequest(
            "aircraft model name cannot be empty".into(),
        ));
    }
    if let Some(id) = resolve_by_name_ci(pool, org_id, trimmed).await? {
        return Ok(id);
    }
    let mfr = manufacturer
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let row = sqlx::query(
        "INSERT INTO aircraft_models (org_id, name, manufacturer) VALUES ($1, $2, $3) \
         ON CONFLICT (org_id, LOWER(name)) DO UPDATE SET name = aircraft_models.name \
         RETURNING id"
    )
    .bind(org_id)
    .bind(trimmed)
    .bind(mfr)
    .fetch_one(pool)
    .await?;
    Ok(row.get::<i32, _>("id"))
}
