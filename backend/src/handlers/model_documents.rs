// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

//! Document upload/download/delete for the per-org aircraft_models catalog.
//!
//! Storage layout: `${UPLOAD_DIR}/aircraft_models/{model_id}/{document_id}.pdf`.
//! Files are keyed by document id (PK), not by category name, so deletes and
//! replaces leave a clean 1:1 correspondence between rows and on-disk files
//! and the audit log captures each row's original filename + size at the
//! moment of upload.
//!
//! Permission split:
//!   - `upload` and `delete` are admin-only.
//!   - `list` and `download` are open to any authenticated role (manuals are
//!     reference material; gating reads adds friction without compliance
//!     value, matching the PR #2 view-everywhere stance for /aircraft-models).
//!
//! File-write ordering: we INSERT inside the tx, commit, then write the file
//! AFTER commit. Failure to write the file leaves an orphan DB row — the
//! handler attempts to DELETE the orphan row before returning 500. The
//! alternative (write-temp-then-rename) was rejected for code-weight reasons;
//! the failure mode (full disk / readonly volume) is rare and recoverable.

use std::path::{Path as StdPath, PathBuf};

use axum::{
    body::Body,
    extract::{Path, State},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use axum_extra::extract::Multipart;
use sqlx::Row;

use crate::auth::middleware::{require_admin, Claims};
use crate::db::DbPool;
use crate::errors::AppError;
use crate::models::model_document::{ModelDocument, ModelDocumentResponse};
use crate::services::audit;

#[cfg(test)]
#[path = "model_documents_tests.rs"]
mod tests;

const ALLOWED_CATEGORIES: &[&str] = &["instruction_manual", "maintenance_manual", "other"];
const MAX_UPLOAD_BYTES: usize = 100 * 1024 * 1024;
const PDF_MAGIC: &[u8] = b"%PDF-";
const PDF_MIME: &str = "application/pdf";
const MAX_TITLE_LEN: usize = 200;

fn upload_dir() -> String {
    std::env::var("UPLOAD_DIR").unwrap_or_else(|_| "./uploads".to_string())
}

/// Resolve a stored `file_path` (relative to UPLOAD_DIR) to an absolute disk
/// path. The DB never stores absolute paths — the column is portable across
/// deploys with different UPLOAD_DIR values.
fn absolute_path(file_path: &str) -> PathBuf {
    StdPath::new(&upload_dir()).join(file_path)
}

/// Replace characters that would break a Content-Disposition header. Quotes
/// terminate the value; CR/LF inject headers; backslash escapes. Anything
/// outside printable ASCII is also coerced to underscore for safety — for
/// non-ASCII display we'd want RFC 5987 filename* params, but the simpler
/// safe-ASCII coercion is sufficient for the common case (operators name
/// files like "DJI_Mavic_3_User_Manual.pdf").
fn sanitize_filename(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    for c in name.chars() {
        if matches!(c, '"' | '\\' | '\r' | '\n') || (c as u32) < 0x20 || (c as u32) > 0x7e {
            out.push('_');
        } else {
            out.push(c);
        }
    }
    if out.is_empty() {
        "document.pdf".into()
    } else {
        out
    }
}

/// Confirms a model exists in the caller's org and returns 404 otherwise.
/// Used as the prefix check for every endpoint in this module so cross-org
/// IDs surface as 404 (not 403) — matches the convention from
/// `aircraft_models::get_stats`.
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

/// POST /api/v1/aircraft-models/:id/documents
///
/// multipart fields:
///   - `category` (text): one of ALLOWED_CATEGORIES.
///   - `file` (binary): the PDF.
///   - `title` (text, optional): required when category='other', forbidden
///     otherwise. 1-200 chars after trimming.
///   - `replace_doc_id` (text, optional): only valid when category='other'.
///     Targets a specific 'other' document for replacement (otherwise
///     uploading 'other' always creates a new row).
///
/// Replace semantics:
///   - instruction_manual / maintenance_manual: an existing row for the
///     same (model_id, category) is unlinked and DB-deleted before insert.
///     Audited as REPLACE_DOCUMENT.
///   - other (no replace_doc_id): always INSERT a new row. Audited as
///     UPLOAD_DOCUMENT.
///   - other (with replace_doc_id): the targeted row is unlinked and
///     deleted; new row inserted with new title. Audited as REPLACE_DOCUMENT.
pub async fn upload(
    claims: Claims,
    State(pool): State<DbPool>,
    Path(model_id): Path<i32>,
    mut multipart: Multipart,
) -> Result<Json<ModelDocumentResponse>, AppError> {
    require_admin(&claims)?;
    ensure_model_in_org(&pool, model_id, claims.org_id).await?;

    let mut category: Option<String> = None;
    let mut file_bytes: Option<Vec<u8>> = None;
    let mut original_filename = String::from("document.pdf");
    let mut title: Option<String> = None;
    let mut replace_doc_id: Option<i32> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(format!("Multipart error: {}", e)))?
    {
        match field.name() {
            Some("category") => {
                let v = field
                    .text()
                    .await
                    .map_err(|e| AppError::BadRequest(format!("Reading category: {}", e)))?;
                category = Some(v.trim().to_string());
            }
            Some("title") => {
                let v = field
                    .text()
                    .await
                    .map_err(|e| AppError::BadRequest(format!("Reading title: {}", e)))?;
                title = Some(v);
            }
            Some("replace_doc_id") => {
                let v = field
                    .text()
                    .await
                    .map_err(|e| AppError::BadRequest(format!("Reading replace_doc_id: {}", e)))?;
                let parsed: i32 = v.trim().parse().map_err(|_| {
                    AppError::BadRequest(format!(
                        "replace_doc_id must be a positive integer, got {:?}",
                        v
                    ))
                })?;
                replace_doc_id = Some(parsed);
            }
            Some("file") => {
                if let Some(fname) = field.file_name() {
                    original_filename = fname.to_string();
                }
                let bytes = field
                    .bytes()
                    .await
                    .map_err(|e| AppError::BadRequest(format!("Reading file: {}", e)))?;
                file_bytes = Some(bytes.to_vec());
            }
            _ => {}
        }
    }

    let category = category.ok_or_else(|| {
        AppError::BadRequest("category field is required (instruction_manual / maintenance_manual / other)".into())
    })?;
    if !ALLOWED_CATEGORIES.contains(&category.as_str()) {
        return Err(AppError::BadRequest(format!(
            "category must be one of: {:?}",
            ALLOWED_CATEGORIES
        )));
    }

    // Per-category validation of title + replace_doc_id semantics. The DB
    // CHECK constraint catches the title rule for 'other' as a backstop, but
    // returning a clean 400 from the handler beats letting the constraint
    // produce a generic Conflict.
    let trimmed_title: Option<String> = if category == "other" {
        if replace_doc_id.is_some() && title.is_none() {
            // Allow replace path to omit title only if explicitly OK — but
            // for clarity, REQUIRE title on every 'other' upload, including
            // replacements. Operator may want to change the label.
        }
        let t = title
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| AppError::BadRequest(
                "title is required when category='other' (1-200 chars)".into(),
            ))?;
        if t.chars().count() > MAX_TITLE_LEN {
            return Err(AppError::BadRequest(format!(
                "title must be at most {} characters",
                MAX_TITLE_LEN
            )));
        }
        Some(t.to_string())
    } else {
        if title.is_some() {
            return Err(AppError::BadRequest(format!(
                "title is not allowed for category={}; only 'other' uses titles",
                category
            )));
        }
        if replace_doc_id.is_some() {
            return Err(AppError::BadRequest(format!(
                "replace_doc_id is not allowed for category={}; manuals auto-replace by category",
                category
            )));
        }
        None
    };

    let bytes = file_bytes.ok_or_else(|| AppError::BadRequest("file field is required".into()))?;
    if bytes.is_empty() {
        return Err(AppError::BadRequest("file is empty".into()));
    }
    if bytes.len() > MAX_UPLOAD_BYTES {
        return Err(AppError::BadRequest(format!(
            "file exceeds {} byte limit",
            MAX_UPLOAD_BYTES
        )));
    }
    if bytes.len() < PDF_MAGIC.len() || &bytes[..PDF_MAGIC.len()] != PDF_MAGIC {
        return Err(AppError::BadRequest(
            "Only PDF files are accepted (magic byte check failed)".into(),
        ));
    }

    let size_bytes = bytes.len() as i64;

    let mut tx = pool.begin().await?;

    // Determine which (if any) existing row gets replaced by this upload:
    //   - 'other' + replace_doc_id: target that specific doc (must match
    //     model + category).
    //   - 'other' without replace_doc_id: no replacement; always new row.
    //   - manual categories: any existing row for same (model, category).
    let previous: Option<ModelDocument> = if category == "other" {
        if let Some(target_id) = replace_doc_id {
            let row = sqlx::query_as::<_, ModelDocument>(
                "SELECT * FROM model_documents \
                 WHERE id = $1 AND model_id = $2 AND org_id = $3",
            )
            .bind(target_id)
            .bind(model_id)
            .bind(claims.org_id)
            .fetch_optional(&mut *tx)
            .await?;
            let row = row.ok_or_else(|| AppError::BadRequest(format!(
                "replace_doc_id {} not found for model {}",
                target_id, model_id
            )))?;
            if row.category != "other" {
                return Err(AppError::BadRequest(format!(
                    "replace_doc_id {} has category={}, cannot be replaced via category='other'",
                    target_id, row.category
                )));
            }
            Some(row)
        } else {
            None
        }
    } else {
        sqlx::query_as::<_, ModelDocument>(
            "SELECT * FROM model_documents \
             WHERE model_id = $1 AND category = $2 AND org_id = $3",
        )
        .bind(model_id)
        .bind(&category)
        .bind(claims.org_id)
        .fetch_optional(&mut *tx)
        .await?
    };

    if let Some(prev) = previous.as_ref() {
        sqlx::query("DELETE FROM model_documents WHERE id = $1")
            .bind(prev.id)
            .execute(&mut *tx)
            .await?;
    }

    let row = sqlx::query(
        "INSERT INTO model_documents \
           (model_id, org_id, category, title, original_filename, file_path, size_bytes, mime_type, uploaded_by) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) RETURNING id",
    )
    .bind(model_id)
    .bind(claims.org_id)
    .bind(&category)
    .bind(trimmed_title.as_deref())
    .bind(&original_filename)
    .bind("") // placeholder — updated below once we know the id
    .bind(size_bytes)
    .bind(PDF_MIME)
    .bind(claims.user_id)
    .fetch_one(&mut *tx)
    .await?;

    let new_id: i32 = row.get("id");
    let file_path = format!("aircraft_models/{}/{}.pdf", model_id, new_id);

    sqlx::query("UPDATE model_documents SET file_path = $1 WHERE id = $2")
        .bind(&file_path)
        .bind(new_id)
        .execute(&mut *tx)
        .await?;

    let inserted = sqlx::query_as::<_, ModelDocument>(
        "SELECT * FROM model_documents WHERE id = $1",
    )
    .bind(new_id)
    .fetch_one(&mut *tx)
    .await?;

    // Audit inside the tx so the row only lands if commit succeeds. Two
    // shapes: REPLACE_DOCUMENT carries previous-metadata for both the
    // manual-category replace path and the explicit 'other' replace_doc_id
    // path; UPLOAD_DOCUMENT covers new manual uploads and every
    // non-replacing 'other' upload.
    let details = match previous.as_ref() {
        Some(prev) => serde_json::json!({
            "model_id": model_id,
            "category": category,
            "new_id": new_id,
            "new_title": trimmed_title,
            "new_filename": original_filename,
            "new_size_bytes": size_bytes,
            "previous_id": prev.id,
            "previous_title": prev.title,
            "previous_filename": prev.original_filename,
            "previous_size_bytes": prev.size_bytes,
        }),
        None => serde_json::json!({
            "model_id": model_id,
            "category": category,
            "new_id": new_id,
            "new_title": trimmed_title,
            "new_filename": original_filename,
            "new_size_bytes": size_bytes,
        }),
    };
    let action = if previous.is_some() {
        "REPLACE_DOCUMENT"
    } else {
        "UPLOAD_DOCUMENT"
    };
    audit::log_custom_action(
        &pool,
        claims.org_id,
        claims.user_id,
        &claims.user_code,
        action,
        "model_documents",
        new_id,
        details,
    )
    .await?;

    tx.commit().await?;

    // After commit: write the file. On failure, attempt to roll back the DB
    // row so we don't leave an orphan. The previous file is left in place
    // until we know the new one is on disk.
    let target = absolute_path(&file_path);
    let dir = target.parent().ok_or_else(|| {
        AppError::Internal(anyhow::anyhow!("Could not determine parent dir for {}", file_path))
    })?;
    if let Err(e) = std::fs::create_dir_all(dir) {
        rollback_orphan_row(&pool, new_id).await;
        tracing::error!("create_dir_all failed for {:?}: {:?}", dir, e);
        return Err(AppError::Internal(anyhow::anyhow!(e)));
    }
    if let Err(e) = std::fs::write(&target, &bytes) {
        rollback_orphan_row(&pool, new_id).await;
        tracing::error!("fs::write failed for {:?}: {:?}", target, e);
        return Err(AppError::Internal(anyhow::anyhow!(e)));
    }

    // Best-effort unlink the previous file. Tolerate ENOENT; warn for
    // anything else and keep going — the audit row preserves the previous
    // filename so an operator can reconcile if it matters.
    if let Some(prev) = previous {
        let prev_path = absolute_path(&prev.file_path);
        match std::fs::remove_file(&prev_path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                tracing::warn!(
                    "Failed to unlink replaced document file {:?} (doc id {}): {:?}",
                    prev_path,
                    prev.id,
                    e
                );
            }
        }
    }

    Ok(Json(ModelDocumentResponse::from(inserted)))
}

async fn rollback_orphan_row(pool: &DbPool, doc_id: i32) {
    if let Err(e) = sqlx::query("DELETE FROM model_documents WHERE id = $1")
        .bind(doc_id)
        .execute(pool)
        .await
    {
        tracing::error!(
            "Failed to roll back orphan model_documents row {}: {:?}",
            doc_id,
            e
        );
    }
}

/// GET /api/v1/aircraft-models/:id/documents
pub async fn list(
    claims: Claims,
    State(pool): State<DbPool>,
    Path(model_id): Path<i32>,
) -> Result<Json<Vec<ModelDocumentResponse>>, AppError> {
    ensure_model_in_org(&pool, model_id, claims.org_id).await?;

    let rows = sqlx::query_as::<_, ModelDocument>(
        "SELECT * FROM model_documents \
         WHERE model_id = $1 AND org_id = $2 \
         ORDER BY category, uploaded_at DESC",
    )
    .bind(model_id)
    .bind(claims.org_id)
    .fetch_all(&pool)
    .await?;

    Ok(Json(rows.into_iter().map(Into::into).collect()))
}

/// GET /api/v1/aircraft-models/:id/documents/:doc_id/download
pub async fn download(
    claims: Claims,
    State(pool): State<DbPool>,
    Path((model_id, doc_id)): Path<(i32, i32)>,
) -> Result<Response, AppError> {
    let doc = sqlx::query_as::<_, ModelDocument>(
        "SELECT * FROM model_documents \
         WHERE id = $1 AND model_id = $2 AND org_id = $3",
    )
    .bind(doc_id)
    .bind(model_id)
    .bind(claims.org_id)
    .fetch_optional(&pool)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Document {} not found", doc_id)))?;

    let path = absolute_path(&doc.file_path);
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) => {
            tracing::error!(
                "Document file missing or unreadable: doc_id={}, path={:?}, err={:?}",
                doc.id,
                path,
                e
            );
            return Err(AppError::Internal(anyhow::anyhow!(
                "Document file missing on disk; contact admin"
            )));
        }
    };

    let safe_name = sanitize_filename(&doc.original_filename);
    let disposition = format!("attachment; filename=\"{}\"", safe_name);

    let mut resp = Response::new(Body::from(bytes));
    *resp.status_mut() = StatusCode::OK;
    let headers = resp.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(PDF_MIME));
    headers.insert(
        header::CONTENT_LENGTH,
        HeaderValue::from(doc.size_bytes as u64),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        HeaderValue::from_str(&disposition).map_err(|e| {
            tracing::error!("Building Content-Disposition: {:?}", e);
            AppError::Internal(anyhow::anyhow!(e))
        })?,
    );
    Ok(resp)
}

/// DELETE /api/v1/aircraft-models/:id/documents/:doc_id
pub async fn delete(
    claims: Claims,
    State(pool): State<DbPool>,
    Path((model_id, doc_id)): Path<(i32, i32)>,
) -> Result<Response, AppError> {
    require_admin(&claims)?;

    let mut tx = pool.begin().await?;
    let doc = sqlx::query_as::<_, ModelDocument>(
        "SELECT * FROM model_documents \
         WHERE id = $1 AND model_id = $2 AND org_id = $3",
    )
    .bind(doc_id)
    .bind(model_id)
    .bind(claims.org_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Document {} not found", doc_id)))?;

    sqlx::query("DELETE FROM model_documents WHERE id = $1")
        .bind(doc.id)
        .execute(&mut *tx)
        .await?;

    audit::log_custom_action(
        &pool,
        claims.org_id,
        claims.user_id,
        &claims.user_code,
        "DELETE_DOCUMENT",
        "model_documents",
        doc.id,
        serde_json::json!({
            "model_id": doc.model_id,
            "doc_id": doc.id,
            "category": doc.category,
            "filename": doc.original_filename,
            "size_bytes": doc.size_bytes,
        }),
    )
    .await?;

    tx.commit().await?;

    let path = absolute_path(&doc.file_path);
    match std::fs::remove_file(&path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            tracing::warn!(
                "Failed to unlink deleted document file {:?} (doc id {}): {:?}",
                path,
                doc.id,
                e
            );
        }
    }

    Ok((StatusCode::NO_CONTENT, ()).into_response())
}

/// Best-effort filesystem cleanup for every document file under a given
/// model. Called by `aircraft_models::permanent_delete` BEFORE the DB
/// DELETE so the CASCADE doesn't drop the rows before we've read their
/// file_paths.
pub async fn unlink_files_for_model(pool: &DbPool, model_id: i32, org_id: i32) {
    let docs = match sqlx::query_as::<_, ModelDocument>(
        "SELECT * FROM model_documents WHERE model_id = $1 AND org_id = $2",
    )
    .bind(model_id)
    .bind(org_id)
    .fetch_all(pool)
    .await
    {
        Ok(d) => d,
        Err(e) => {
            tracing::warn!(
                "unlink_files_for_model: failed to list documents for model {}: {:?}",
                model_id,
                e
            );
            return;
        }
    };

    for doc in docs {
        let path = absolute_path(&doc.file_path);
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                tracing::warn!(
                    "Failed to unlink document file {:?} (doc id {}, model {}): {:?}",
                    path,
                    doc.id,
                    model_id,
                    e
                );
            }
        }
    }
}
