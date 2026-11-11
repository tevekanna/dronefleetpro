// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

//! Per-organisation branding (sidebar logo upload).
//!
//! Two logo variants per org keyed by `variant` ('light' | 'dark') — the image
//! shown when the UI is in that colour theme. Stored as BYTEA in
//! `organisation_branding` (one row per org+variant) rather than the `settings`
//! KV store, because `GET /settings` is admin-only and the sidebar logo must be
//! readable by every authenticated role.
//!
//! The read endpoint returns base64 `data:` URLs so the frontend can bind them
//! directly to <img src> (a plain <img> cannot carry the Bearer auth header).
//! Upload and delete are admin-only and audited.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use axum_extra::extract::Multipart;
use base64::Engine as _;
use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::auth::middleware::{require_admin, Claims};
use crate::db::DbPool;
use crate::errors::AppError;
use crate::services::audit;

const MAX_LOGO_BYTES: usize = 2 * 1024 * 1024; // 2 MB
const PNG_MAGIC: &[u8] = &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
const JPEG_MAGIC: &[u8] = &[0xFF, 0xD8, 0xFF];

fn valid_variant(v: &str) -> bool {
    v == "light" || v == "dark"
}

/// Detect image type from magic bytes. Returns the canonical MIME or None.
fn detect_mime(bytes: &[u8]) -> Option<&'static str> {
    if bytes.len() >= PNG_MAGIC.len() && &bytes[..PNG_MAGIC.len()] == PNG_MAGIC {
        Some("image/png")
    } else if bytes.len() >= JPEG_MAGIC.len() && &bytes[..JPEG_MAGIC.len()] == JPEG_MAGIC {
        Some("image/jpeg")
    } else {
        None
    }
}

#[derive(Debug, sqlx::FromRow)]
struct BrandingRow {
    variant: String,
    logo: Vec<u8>,
    mime_type: String,
    size_bytes: i32,
    original_filename: Option<String>,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct LogoMeta {
    data_url: String,
    mime_type: String,
    size_bytes: i32,
    original_filename: Option<String>,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
pub struct BrandingResponse {
    light: Option<LogoMeta>,
    dark: Option<LogoMeta>,
}

/// GET /api/v1/organisation/branding — any authenticated role.
pub async fn get_branding(
    claims: Claims,
    State(pool): State<DbPool>,
) -> Result<Json<BrandingResponse>, AppError> {
    let rows = sqlx::query_as::<_, BrandingRow>(
        "SELECT variant, logo, mime_type, size_bytes, original_filename, updated_at \
         FROM organisation_branding WHERE org_id = $1",
    )
    .bind(claims.org_id)
    .fetch_all(&pool)
    .await?;

    let mut resp = BrandingResponse { light: None, dark: None };
    for row in rows {
        let data_url = format!(
            "data:{};base64,{}",
            row.mime_type,
            base64::engine::general_purpose::STANDARD.encode(&row.logo)
        );
        let meta = LogoMeta {
            data_url,
            mime_type: row.mime_type.clone(),
            size_bytes: row.size_bytes,
            original_filename: row.original_filename.clone(),
            updated_at: row.updated_at,
        };
        match row.variant.as_str() {
            "light" => resp.light = Some(meta),
            "dark" => resp.dark = Some(meta),
            _ => {}
        }
    }
    Ok(Json(resp))
}

/// POST /api/v1/organisation/logo/:variant — admin-only, multipart `file`.
pub async fn upload_logo(
    claims: Claims,
    State(pool): State<DbPool>,
    Path(variant): Path<String>,
    mut multipart: Multipart,
) -> Result<Json<serde_json::Value>, AppError> {
    require_admin(&claims)?;
    if !valid_variant(&variant) {
        return Err(AppError::BadRequest("variant must be 'light' or 'dark'".into()));
    }

    let mut file_bytes: Option<Vec<u8>> = None;
    let mut original_filename: Option<String> = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(format!("Multipart error: {}", e)))?
    {
        if field.name() == Some("file") {
            if let Some(fname) = field.file_name() {
                original_filename = Some(fname.to_string());
            }
            let bytes = field
                .bytes()
                .await
                .map_err(|e| AppError::BadRequest(format!("Reading file: {}", e)))?;
            file_bytes = Some(bytes.to_vec());
        }
    }

    let bytes = file_bytes.ok_or_else(|| AppError::BadRequest("file field is required".into()))?;
    if bytes.is_empty() {
        return Err(AppError::BadRequest("file is empty".into()));
    }
    if bytes.len() > MAX_LOGO_BYTES {
        return Err(AppError::BadRequest("logo exceeds the 2 MB limit".into()));
    }
    let mime = detect_mime(&bytes).ok_or_else(|| {
        AppError::BadRequest("Only PNG and JPEG images are accepted (magic byte check failed)".into())
    })?;
    let size_bytes = bytes.len() as i32;

    sqlx::query(
        "INSERT INTO organisation_branding \
            (org_id, variant, logo, mime_type, size_bytes, original_filename, updated_by, updated_at) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, now()) \
         ON CONFLICT (org_id, variant) DO UPDATE SET \
            logo = EXCLUDED.logo, mime_type = EXCLUDED.mime_type, \
            size_bytes = EXCLUDED.size_bytes, original_filename = EXCLUDED.original_filename, \
            updated_by = EXCLUDED.updated_by, updated_at = now()",
    )
    .bind(claims.org_id)
    .bind(&variant)
    .bind(&bytes)
    .bind(mime)
    .bind(size_bytes)
    .bind(original_filename.as_deref())
    .bind(claims.user_id)
    .execute(&pool)
    .await?;

    audit::log_custom_action(
        &pool,
        claims.org_id,
        claims.user_id,
        &claims.user_code,
        "UPLOAD_ORG_LOGO",
        "organisation_branding",
        0,
        serde_json::json!({
            "variant": variant,
            "mime_type": mime,
            "size_bytes": size_bytes,
            "original_filename": original_filename,
        }),
    )
    .await?;

    Ok(Json(serde_json::json!({
        "variant": variant,
        "mime_type": mime,
        "size_bytes": size_bytes,
        "original_filename": original_filename,
    })))
}

/// DELETE /api/v1/organisation/logo/:variant — admin-only.
pub async fn delete_logo(
    claims: Claims,
    State(pool): State<DbPool>,
    Path(variant): Path<String>,
) -> Result<Response, AppError> {
    require_admin(&claims)?;
    if !valid_variant(&variant) {
        return Err(AppError::BadRequest("variant must be 'light' or 'dark'".into()));
    }

    let result = sqlx::query("DELETE FROM organisation_branding WHERE org_id = $1 AND variant = $2")
        .bind(claims.org_id)
        .bind(&variant)
        .execute(&pool)
        .await?;

    if result.rows_affected() == 0 {
        return Err(AppError::NotFound(format!("No {} logo to delete", variant)));
    }

    audit::log_custom_action(
        &pool,
        claims.org_id,
        claims.user_id,
        &claims.user_code,
        "DELETE_ORG_LOGO",
        "organisation_branding",
        0,
        serde_json::json!({ "variant": variant }),
    )
    .await?;

    Ok((StatusCode::NO_CONTENT, ()).into_response())
}
