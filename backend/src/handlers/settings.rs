// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

use axum::{
    extract::{Path, State},
    Json,
};
use serde::{Deserialize, Serialize};

use crate::auth::middleware::{require_admin, Claims};
use crate::db::DbPool;
use crate::errors::AppError;
use crate::services::audit;

/// Audit snapshot — mirrors the DB row shape so the scrub helper can apply the
/// settings-specific "secret keys scrub the value column" rule.
#[derive(Debug, Serialize)]
struct SettingAuditRow<'a> {
    key: &'a str,
    value: &'a str,
}

#[derive(Debug, Deserialize)]
pub struct UpsertSettingRequest {
    pub value: String,
}

/// GET /api/v1/settings — Returns all settings for the org as a JSON object.
pub async fn get_all_settings(
    claims: Claims,
    State(pool): State<DbPool>,
) -> Result<Json<serde_json::Value>, AppError> {
    // Read side is admin-only: settings may contain secrets (e.g. dji_api_key)
    // and we avoid exposing them to non-admin authenticated users.
    require_admin(&claims)?;
    let rows = sqlx::query_as::<_, SettingRow>(
        "SELECT key, value FROM settings WHERE org_id = $1"
    )
    .bind(claims.org_id)
    .fetch_all(&pool)
    .await?;

    let mut map = serde_json::Map::new();
    for row in rows {
        map.insert(row.key, serde_json::Value::String(row.value.unwrap_or_default()));
    }

    Ok(Json(serde_json::Value::Object(map)))
}

/// GET /api/v1/settings/:key — Returns a single setting value.
pub async fn get_setting(
    claims: Claims,
    State(pool): State<DbPool>,
    Path(key): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_admin(&claims)?;
    let row = sqlx::query_as::<_, SettingRow>(
        "SELECT key, value FROM settings WHERE org_id = $1 AND key = $2"
    )
    .bind(claims.org_id)
    .bind(&key)
    .fetch_optional(&pool)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Setting '{}' not found", key)))?;

    Ok(Json(serde_json::json!({
        "key": row.key,
        "value": row.value.unwrap_or_default()
    })))
}

/// PUT /api/v1/settings/:key — Upsert a key/value pair.
///
/// Audited as INSERT (new key) or UPDATE (existing key). For secret keys
/// (e.g. `dji_api_key`) the `value` column is scrubbed from the diff — only
/// the fact of change is preserved. See services::audit for the scrub list.
pub async fn upsert_setting(
    claims: Claims,
    State(pool): State<DbPool>,
    Path(key): Path<String>,
    Json(body): Json<UpsertSettingRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_admin(&claims)?;
    let mut tx = pool.begin().await?;
    let before_row: Option<SettingRow> = sqlx::query_as::<_, SettingRow>(
        "SELECT key, value FROM settings WHERE org_id = $1 AND key = $2"
    )
    .bind(claims.org_id)
    .bind(&key)
    .fetch_optional(&mut *tx)
    .await?;

    sqlx::query(
        "INSERT INTO settings (org_id, key, value) VALUES ($1,$2,$3) \
         ON CONFLICT (org_id, key) DO UPDATE SET value = EXCLUDED.value"
    )
    .bind(claims.org_id)
    .bind(&key)
    .bind(&body.value)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    let after_row = SettingAuditRow { key: &key, value: &body.value };

    // The settings table has a composite PK (org_id, key) — no scalar id.
    // Using 0 as the audit record_id sentinel and encoding the key in the
    // diff body (where the scrub rule applies).
    match before_row {
        Some(before) => {
            let before_audit = SettingAuditRow {
                key: &before.key,
                value: before.value.as_deref().unwrap_or(""),
            };
            audit::log_update_diff(
                &pool, claims.org_id, claims.user_id, &claims.user_code,
                "settings", 0, &before_audit, &after_row,
            ).await;
        }
        None => {
            audit::log_insert_full(
                &pool, claims.org_id, claims.user_id, &claims.user_code,
                "settings", 0, &after_row,
            ).await;
        }
    }

    Ok(Json(serde_json::json!({
        "key": key,
        "value": body.value
    })))
}

#[derive(Debug, sqlx::FromRow)]
struct SettingRow {
    pub key: String,
    pub value: Option<String>,
}
