// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

use std::collections::HashMap;
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

use crate::auth::middleware::{
    require_admin, require_role, require_self_or_role, Claims,
};
use crate::db::DbPool;
use crate::errors::AppError;
use crate::models::incident::{
    AddNoteRequest, CreateIncidentRequest, Incident, IncidentAttachment,
    IncidentAttachmentResponse, IncidentDetailResponse, IncidentListItem, IncidentNote,
    UpdateIncidentRequest,
};
use crate::services::audit;
use crate::services::occurrence_reporting::{self, ReportingStatus};
use crate::services::occurrence_taxonomy;
use chrono::Utc;

#[cfg(test)]
#[path = "incidents_db_tests.rs"]
mod db_tests;

const VALID_SEVERITIES: &[&str] = &["low", "medium", "high", "critical"];

/// Validate an Annex VI occurrence code against the compile-time taxonomy.
/// Mirrors the severity validation style: 400 naming the invalid value.
fn validate_occurrence_code(code: &str) -> Result<(), AppError> {
    if occurrence_taxonomy::find(code).is_none() {
        return Err(AppError::BadRequest(format!(
            "occurrence_code '{}' is not a valid Annex VI occurrence code",
            code
        )));
    }
    Ok(())
}

/// Resolve `aircraft_models.design_approval_required` for a set of aircraft
/// ids in one query (resolve-on-read, mirroring the aircraft/battery lookups
/// in `list_flights`). The map value is None when the aircraft has no model
/// or the model has not been assessed — both collapse to "undetermined".
async fn design_approval_by_aircraft(
    pool: &DbPool,
    org_id: i32,
    aircraft_ids: &[i32],
) -> Result<HashMap<i32, Option<String>>, AppError> {
    if aircraft_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let rows = sqlx::query(
        "SELECT a.id, am.design_approval_required \
         FROM aircraft a \
         LEFT JOIN aircraft_models am ON am.id = a.model_id \
         WHERE a.org_id = $1 AND a.id = ANY($2)",
    )
    .bind(org_id)
    .bind(aircraft_ids)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| {
            (
                r.get::<i32, _>("id"),
                r.get::<Option<String>, _>("design_approval_required"),
            )
        })
        .collect())
}

/// Compute the ReportingStatus for one incident given the resolved
/// design-approval map. An incident with no aircraft, or whose aircraft is
/// absent from the map, is undetermined by construction.
fn reporting_for(
    incident: &Incident,
    dar_map: &HashMap<i32, Option<String>>,
    now: chrono::DateTime<Utc>,
) -> ReportingStatus {
    let dar = incident
        .aircraft_id
        .and_then(|id| dar_map.get(&id))
        .and_then(|v| v.as_deref());
    occurrence_reporting::compute_status(
        dar,
        incident.occurrence_code.as_deref(),
        incident.became_aware_at,
        incident.reported_at,
        now,
    )
}

/// Maximum upload size for an incident attachment (256 MiB). Matches the
/// `DefaultBodyLimit` layer applied to the upload route in `main.rs`.
const MAX_ATTACHMENT_BYTES: usize = 256 * 1024 * 1024;

// ---------------------------------------------------------------------------
// Attachment file-lifecycle helpers (mirrors `model_documents.rs`; that
// module's versions are private, so we replicate small equivalents here).
// ---------------------------------------------------------------------------

fn upload_dir() -> String {
    std::env::var("UPLOAD_DIR").unwrap_or_else(|_| "./uploads".to_string())
}

/// Resolve a stored `file_path` (relative to UPLOAD_DIR) to an absolute disk
/// path. The DB never stores absolute paths — the column stays portable
/// across deploys with different UPLOAD_DIR values.
fn absolute_path(file_path: &str) -> PathBuf {
    StdPath::new(&upload_dir()).join(file_path)
}

/// Replace characters that would break a Content-Disposition header. Quotes
/// terminate the value; CR/LF inject headers; backslash escapes. Anything
/// outside printable ASCII is also coerced to underscore.
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
        "attachment".into()
    } else {
        out
    }
}

/// Delete an orphaned attachment row after a post-commit filesystem write
/// failed. Best-effort — logs on failure.
async fn rollback_orphan_row(pool: &DbPool, att_id: i32) {
    if let Err(e) = sqlx::query("DELETE FROM incident_attachments WHERE id = $1")
        .bind(att_id)
        .execute(pool)
        .await
    {
        tracing::error!(
            "Failed to roll back orphan incident_attachments row {}: {:?}",
            att_id,
            e
        );
    }
}

/// Allowlisted upload extensions. Given a lowercased extension, returns
/// `(kind, mime)` for the attachment, or `None` if the extension is not
/// accepted.
fn attachment_kind_and_mime(ext: &str) -> Option<(&'static str, &'static str)> {
    match ext {
        "pdf" => Some(("document", "application/pdf")),
        "txt" => Some(("document", "text/plain")),
        "log" => Some(("log", "text/plain")),
        "csv" => Some(("document", "text/csv")),
        "json" => Some(("document", "application/json")),
        "kml" => Some(("document", "application/vnd.google-earth.kml+xml")),
        "gpx" => Some(("document", "application/gpx+xml")),
        "bin" => Some(("ardupilot_bin", "application/octet-stream")),
        "tlog" => Some(("ardupilot_tlog", "application/octet-stream")),
        "ulg" => Some(("log", "application/octet-stream")),
        "png" => Some(("image", "image/png")),
        "jpg" | "jpeg" => Some(("image", "image/jpeg")),
        "gif" => Some(("image", "image/gif")),
        "webp" => Some(("image", "image/webp")),
        _ => None,
    }
}

/// The list of accepted extensions, for error messages.
const ALLOWED_EXTENSIONS: &[&str] = &[
    "pdf", "txt", "log", "csv", "json", "kml", "gpx", "bin", "tlog", "ulg", "png", "jpg",
    "jpeg", "gif", "webp",
];

/// Derive the lowercased extension from a filename (text after the final
/// `.`). Returns `None` when there is no extension.
fn extension_of(filename: &str) -> Option<String> {
    filename
        .rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_lowercase())
        .filter(|e| !e.is_empty())
}

/// Confirm an incident exists in the caller's org and return it. 404
/// otherwise. Used as the ownership/existence check for the sub-resource
/// endpoints.
async fn ensure_incident_in_org(
    pool: &DbPool,
    id: i32,
    org_id: i32,
) -> Result<Incident, AppError> {
    sqlx::query_as::<_, Incident>("SELECT * FROM incidents WHERE id = $1 AND org_id = $2")
        .bind(id)
        .bind(org_id)
        .fetch_optional(pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Incident {} not found", id)))
}

/// RBAC for modifying an incident (adding attachments or notes). Mirrors the
/// edit gate in `update_incident`: admin/safety_officer can modify any
/// incident; pilot/technician only their own; every other role is forbidden.
fn require_incident_modify(claims: &Claims, incident: &Incident) -> Result<(), AppError> {
    if claims.has_any(&["admin", "safety_officer"]) {
        return Ok(());
    }
    if claims.has_any(&["pilot", "technician"]) {
        return require_self_or_role(claims, incident.created_by_user_id, &[]);
    }
    Err(AppError::Forbidden(
        "Your role cannot modify incidents".into(),
    ))
}

/// Statuses that "close" an incident — transitioning to one of these requires
/// admin or safety_officer per the RBAC matrix.
const CLOSE_STATUSES: &[&str] = &["closed", "resolved"];

/// GET /api/v1/incidents — List all incidents for the org, each row carrying
/// its computed occurrence-reporting status.
pub async fn list_incidents(
    claims: Claims,
    State(pool): State<DbPool>,
) -> Result<Json<Vec<IncidentListItem>>, AppError> {
    let rows = sqlx::query_as::<_, Incident>(
        "SELECT * FROM incidents WHERE org_id = $1 ORDER BY occurred_at DESC"
    )
    .bind(claims.org_id)
    .fetch_all(&pool)
    .await?;

    let mut aircraft_ids: Vec<i32> = rows.iter().filter_map(|i| i.aircraft_id).collect();
    aircraft_ids.sort_unstable();
    aircraft_ids.dedup();
    let dar_map = design_approval_by_aircraft(&pool, claims.org_id, &aircraft_ids).await?;

    let now = Utc::now();
    let items = rows
        .into_iter()
        .map(|incident| {
            let reporting = reporting_for(&incident, &dar_map, now);
            IncidentListItem { incident, reporting }
        })
        .collect();

    Ok(Json(items))
}

/// POST /api/v1/incidents — Report a new incident.
pub async fn create_incident(
    claims: Claims,
    State(pool): State<DbPool>,
    Json(body): Json<CreateIncidentRequest>,
) -> Result<Json<Incident>, AppError> {
    require_role(
        &claims,
        &["admin", "safety_officer", "technician", "pilot"],
    )?;

    if body.description.trim().is_empty() {
        return Err(AppError::BadRequest("description is required".into()));
    }
    if !VALID_SEVERITIES.contains(&body.severity.as_str()) {
        return Err(AppError::BadRequest(format!(
            "severity must be one of: {:?}", VALID_SEVERITIES
        )));
    }
    if let Some(ref code) = body.occurrence_code {
        validate_occurrence_code(code)?;
    }

    let row = sqlx::query_as::<_, Incident>(
        "INSERT INTO incidents (org_id, flight_id, pilot_id, aircraft_id, occurred_at, severity, category, description, location_description, immediate_action, reported_to_authority, status, occurrence_code, became_aware_at, reported_at, reporting_delay_reason, created_by_user_id) VALUES ($1,$2,$3,$4,$5::timestamptz,$6,$7,$8,$9,$10,COALESCE($11,false),'open',$12,$13,$14,$15,$16) RETURNING *"
    )
    .bind(claims.org_id)
    .bind(&body.flight_id)
    .bind(&body.pilot_id)
    .bind(&body.aircraft_id)
    .bind(&body.occurred_at)
    .bind(&body.severity)
    .bind(&body.category)
    .bind(&body.description)
    .bind(&body.location_description)
    .bind(&body.immediate_action)
    .bind(&body.reported_to_authority)
    .bind(&body.occurrence_code)
    .bind(&body.became_aware_at)
    .bind(&body.reported_at)
    .bind(&body.reporting_delay_reason)
    .bind(claims.user_id)
    .fetch_one(&pool)
    .await
    .map_err(|e| {
        tracing::error!("create_incident DB error: {:?}", e);
        AppError::Database(e)
    })?;

    audit::log_insert_full(&pool, claims.org_id, claims.user_id, &claims.user_code, "incidents", row.id, &row).await;

    Ok(Json(row))
}

/// PUT /api/v1/incidents/:id — Update incident status, root cause, corrective action.
pub async fn update_incident(
    claims: Claims,
    State(pool): State<DbPool>,
    Path(id): Path<i32>,
    Json(body): Json<UpdateIncidentRequest>,
) -> Result<Json<Incident>, AppError> {
    let mut tx = pool.begin().await?;
    let before = sqlx::query_as::<_, Incident>(
        "SELECT * FROM incidents WHERE id = $1 AND org_id = $2"
    )
    .bind(id)
    .bind(claims.org_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Incident {} not found", id)))?;

    // RBAC: admin/safety_officer can edit any incident; pilot/technician can
    // edit only their own (creator). Viewer cannot edit at all.
    if claims.has_any(&["admin", "safety_officer"]) {
        // allowed for any incident
    } else if claims.has_any(&["pilot", "technician"]) {
        require_self_or_role(&claims, before.created_by_user_id, &[])?;
    } else {
        return Err(AppError::Forbidden(
            "Your role cannot edit incidents".into(),
        ));
    }

    // Status transition guard: closing/resolving requires admin or
    // safety_officer regardless of edit-own ownership.
    if let Some(ref new_status) = body.status {
        let normalised = new_status.trim().to_ascii_lowercase();
        if CLOSE_STATUSES.contains(&normalised.as_str()) {
            require_role(&claims, &["admin", "safety_officer"]).map_err(|_| {
                AppError::Forbidden(
                    "Closing or resolving an incident requires admin or safety_officer role"
                        .to_string(),
                )
            })?;
        }
    }

    // Validate an incoming occurrence code (explicit null = clear, which
    // needs no validation).
    if let Some(Some(ref code)) = body.occurrence_code {
        validate_occurrence_code(code)?;
    }

    // Tri-state unpacking (double_option): outer None = keep existing,
    // Some(None) = clear to NULL, Some(Some(v)) = set. Paired with the
    // CASE WHEN pattern below because COALESCE can never clear to NULL.
    let (set_code, code_val) = match body.occurrence_code {
        None => (false, None),
        Some(v) => (true, v),
    };
    let (set_aware, aware_val) = match body.became_aware_at {
        None => (false, None),
        Some(v) => (true, v),
    };
    let (set_reported, reported_val) = match body.reported_at {
        None => (false, None),
        Some(v) => (true, v),
    };
    let (set_delay, delay_val) = match body.reporting_delay_reason {
        None => (false, None),
        Some(v) => (true, v),
    };

    let after = sqlx::query_as::<_, Incident>(
        "UPDATE incidents SET status = COALESCE($1, status), root_cause = COALESCE($2, root_cause), corrective_action = COALESCE($3, corrective_action), authority_ref = COALESCE($4, authority_ref), occurrence_code = CASE WHEN $5 THEN $6 ELSE occurrence_code END, became_aware_at = CASE WHEN $7 THEN $8 ELSE became_aware_at END, reported_at = CASE WHEN $9 THEN $10 ELSE reported_at END, reporting_delay_reason = CASE WHEN $11 THEN $12 ELSE reporting_delay_reason END, updated_at = NOW() WHERE id = $13 AND org_id = $14 RETURNING *"
    )
    .bind(&body.status)
    .bind(&body.root_cause)
    .bind(&body.corrective_action)
    .bind(&body.authority_ref)
    .bind(set_code)
    .bind(code_val)
    .bind(set_aware)
    .bind(aware_val)
    .bind(set_reported)
    .bind(reported_val)
    .bind(set_delay)
    .bind(delay_val)
    .bind(id)
    .bind(claims.org_id)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;

    audit::log_update_diff(&pool, claims.org_id, claims.user_id, &claims.user_code, "incidents", after.id, &before, &after).await;

    Ok(Json(after))
}

/// DELETE /api/v1/incidents/:id — Delete an incident record (admin only).
pub async fn delete_incident(
    claims: Claims,
    State(pool): State<DbPool>,
    Path(id): Path<i32>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_admin(&claims)?;

    let mut tx = pool.begin().await?;
    let before = sqlx::query_as::<_, Incident>(
        "SELECT * FROM incidents WHERE id = $1 AND org_id = $2"
    )
    .bind(id)
    .bind(claims.org_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Incident {} not found", id)))?;

    // Read attachment paths and unlink the files BEFORE the DB DELETE. Once
    // the incident row is gone the FK CASCADE removes the attachment rows, so
    // we must capture the paths first.
    unlink_files_for_incident(&pool, id, claims.org_id).await;

    sqlx::query("DELETE FROM incidents WHERE id = $1 AND org_id = $2")
        .bind(id)
        .bind(claims.org_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;

    audit::log_delete_full(&pool, claims.org_id, claims.user_id, &claims.user_code, "incidents", id, &before).await;

    Ok(Json(serde_json::json!({ "message": "Incident deleted", "id": id })))
}

/// GET /api/v1/incidents/:id — Incident detail with attachments and notes.
/// Open to any authenticated role.
pub async fn get_incident(
    claims: Claims,
    State(pool): State<DbPool>,
    Path(id): Path<i32>,
) -> Result<Json<IncidentDetailResponse>, AppError> {
    let incident = ensure_incident_in_org(&pool, id, claims.org_id).await?;

    let attachments = sqlx::query_as::<_, IncidentAttachment>(
        "SELECT * FROM incident_attachments WHERE incident_id = $1 AND org_id = $2 \
         ORDER BY uploaded_at DESC",
    )
    .bind(id)
    .bind(claims.org_id)
    .fetch_all(&pool)
    .await?;

    let notes = sqlx::query_as::<_, IncidentNote>(
        "SELECT * FROM incident_notes WHERE incident_id = $1 AND org_id = $2 \
         ORDER BY created_at ASC",
    )
    .bind(id)
    .bind(claims.org_id)
    .fetch_all(&pool)
    .await?;

    let aircraft_ids: Vec<i32> = incident.aircraft_id.into_iter().collect();
    let dar_map = design_approval_by_aircraft(&pool, claims.org_id, &aircraft_ids).await?;
    let reporting = reporting_for(&incident, &dar_map, Utc::now());

    Ok(Json(IncidentDetailResponse {
        incident,
        reporting,
        attachments: attachments.into_iter().map(Into::into).collect(),
        notes,
    }))
}

/// POST /api/v1/incidents/:id/attachments — Upload an evidence file.
///
/// Reads the `file` multipart field only. RBAC: admin/safety_officer or the
/// incident's own creator (pilot/technician). Mirrors the model_documents
/// file-write ordering: INSERT + audit inside the tx, commit, then write the
/// file after commit; a write failure rolls back the orphan row.
pub async fn upload_attachment(
    claims: Claims,
    State(pool): State<DbPool>,
    Path(id): Path<i32>,
    mut multipart: Multipart,
) -> Result<Json<IncidentAttachmentResponse>, AppError> {
    let incident = ensure_incident_in_org(&pool, id, claims.org_id).await?;
    require_incident_modify(&claims, &incident)?;

    let mut file_bytes: Option<Vec<u8>> = None;
    let mut original_filename = String::from("attachment");
    let mut field_content_type: Option<String> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(format!("Multipart error: {}", e)))?
    {
        if field.name() == Some("file") {
            if let Some(fname) = field.file_name() {
                original_filename = fname.to_string();
            }
            if let Some(ct) = field.content_type() {
                let ct = ct.trim();
                if !ct.is_empty() {
                    field_content_type = Some(ct.to_string());
                }
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
    if bytes.len() > MAX_ATTACHMENT_BYTES {
        return Err(AppError::BadRequest(format!(
            "file exceeds {} byte limit",
            MAX_ATTACHMENT_BYTES
        )));
    }

    let ext = extension_of(&original_filename).ok_or_else(|| {
        AppError::BadRequest(format!(
            "file must have one of these extensions: {}",
            ALLOWED_EXTENSIONS.join(", ")
        ))
    })?;
    let (kind, mapped_mime) = attachment_kind_and_mime(&ext).ok_or_else(|| {
        AppError::BadRequest(format!(
            "file type .{} is not accepted; allowed extensions: {}",
            ext,
            ALLOWED_EXTENSIONS.join(", ")
        ))
    })?;

    // Prefer the client-supplied content type when present; fall back to the
    // extension mapping's canonical mime.
    let mime_type = field_content_type.unwrap_or_else(|| mapped_mime.to_string());
    let size_bytes = bytes.len() as i64;

    let mut tx = pool.begin().await?;

    let row = sqlx::query(
        "INSERT INTO incident_attachments \
           (incident_id, org_id, kind, original_filename, file_path, size_bytes, mime_type, uploaded_by) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8) RETURNING id",
    )
    .bind(id)
    .bind(claims.org_id)
    .bind(kind)
    .bind(&original_filename)
    .bind("") // placeholder — updated below once we know the id
    .bind(size_bytes)
    .bind(&mime_type)
    .bind(claims.user_id)
    .fetch_one(&mut *tx)
    .await?;

    let att_id: i32 = row.get("id");
    let file_path = format!("incidents/{}/{}.{}", id, att_id, ext);

    sqlx::query("UPDATE incident_attachments SET file_path = $1 WHERE id = $2")
        .bind(&file_path)
        .bind(att_id)
        .execute(&mut *tx)
        .await?;

    let inserted = sqlx::query_as::<_, IncidentAttachment>(
        "SELECT * FROM incident_attachments WHERE id = $1",
    )
    .bind(att_id)
    .fetch_one(&mut *tx)
    .await?;

    audit::log_custom_action(
        &pool,
        claims.org_id,
        claims.user_id,
        &claims.user_code,
        "UPLOAD_INCIDENT_ATTACHMENT",
        "incident_attachments",
        att_id,
        serde_json::json!({
            "incident_id": id,
            "filename": original_filename,
            "size_bytes": size_bytes,
            "kind": kind,
        }),
    )
    .await?;

    tx.commit().await?;

    // After commit: write the file. On failure, roll back the DB row so we
    // don't leave an orphan.
    let target = absolute_path(&file_path);
    let dir = target.parent().ok_or_else(|| {
        AppError::Internal(anyhow::anyhow!("Could not determine parent dir for {}", file_path))
    })?;
    if let Err(e) = std::fs::create_dir_all(dir) {
        rollback_orphan_row(&pool, att_id).await;
        tracing::error!("create_dir_all failed for {:?}: {:?}", dir, e);
        return Err(AppError::Internal(anyhow::anyhow!(e)));
    }
    if let Err(e) = std::fs::write(&target, &bytes) {
        rollback_orphan_row(&pool, att_id).await;
        tracing::error!("fs::write failed for {:?}: {:?}", target, e);
        return Err(AppError::Internal(anyhow::anyhow!(e)));
    }

    Ok(Json(IncidentAttachmentResponse::from(inserted)))
}

/// GET /api/v1/incidents/:id/attachments/:att_id/download — Stream an
/// attachment. Open to any authenticated role.
pub async fn download_attachment(
    claims: Claims,
    State(pool): State<DbPool>,
    Path((id, att_id)): Path<(i32, i32)>,
) -> Result<Response, AppError> {
    let att = sqlx::query_as::<_, IncidentAttachment>(
        "SELECT * FROM incident_attachments \
         WHERE id = $1 AND incident_id = $2 AND org_id = $3",
    )
    .bind(att_id)
    .bind(id)
    .bind(claims.org_id)
    .fetch_optional(&pool)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Attachment {} not found", att_id)))?;

    let path = absolute_path(&att.file_path);
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) => {
            tracing::error!(
                "Attachment file missing or unreadable: att_id={}, path={:?}, err={:?}",
                att.id,
                path,
                e
            );
            return Err(AppError::Internal(anyhow::anyhow!(
                "Attachment file missing on disk; contact admin"
            )));
        }
    };

    let safe_name = sanitize_filename(&att.original_filename);
    let disposition = format!("attachment; filename=\"{}\"", safe_name);

    let mut resp = Response::new(Body::from(bytes));
    *resp.status_mut() = StatusCode::OK;
    let headers = resp.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(&att.mime_type).map_err(|e| {
            tracing::error!("Building Content-Type from stored mime: {:?}", e);
            AppError::Internal(anyhow::anyhow!(e))
        })?,
    );
    headers.insert(
        header::CONTENT_LENGTH,
        HeaderValue::from(att.size_bytes as u64),
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

/// DELETE /api/v1/incidents/:id/attachments/:att_id — Remove an attachment.
/// admin/safety_officer only.
pub async fn delete_attachment(
    claims: Claims,
    State(pool): State<DbPool>,
    Path((id, att_id)): Path<(i32, i32)>,
) -> Result<Response, AppError> {
    require_role(&claims, &["admin", "safety_officer"])?;

    let mut tx = pool.begin().await?;
    let att = sqlx::query_as::<_, IncidentAttachment>(
        "SELECT * FROM incident_attachments \
         WHERE id = $1 AND incident_id = $2 AND org_id = $3",
    )
    .bind(att_id)
    .bind(id)
    .bind(claims.org_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Attachment {} not found", att_id)))?;

    sqlx::query("DELETE FROM incident_attachments WHERE id = $1")
        .bind(att.id)
        .execute(&mut *tx)
        .await?;

    audit::log_custom_action(
        &pool,
        claims.org_id,
        claims.user_id,
        &claims.user_code,
        "DELETE_INCIDENT_ATTACHMENT",
        "incident_attachments",
        att.id,
        serde_json::json!({
            "incident_id": id,
            "filename": att.original_filename,
            "size_bytes": att.size_bytes,
            "kind": att.kind,
        }),
    )
    .await?;

    tx.commit().await?;

    let path = absolute_path(&att.file_path);
    match std::fs::remove_file(&path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => {
            tracing::warn!(
                "Failed to unlink deleted attachment file {:?} (att id {}): {:?}",
                path,
                att.id,
                e
            );
        }
    }

    Ok((StatusCode::NO_CONTENT, ()).into_response())
}

/// POST /api/v1/incidents/:id/notes — Append a note. RBAC: admin/safety_officer
/// or the incident's own creator. Notes are append-only (no update/delete).
pub async fn add_note(
    claims: Claims,
    State(pool): State<DbPool>,
    Path(id): Path<i32>,
    Json(body): Json<AddNoteRequest>,
) -> Result<Json<IncidentNote>, AppError> {
    let incident = ensure_incident_in_org(&pool, id, claims.org_id).await?;
    require_incident_modify(&claims, &incident)?;

    let note = body.note.trim();
    if note.is_empty() {
        return Err(AppError::BadRequest("note is required".into()));
    }

    let row = sqlx::query_as::<_, IncidentNote>(
        "INSERT INTO incident_notes (incident_id, org_id, note, created_by) \
         VALUES ($1, $2, $3, $4) RETURNING *",
    )
    .bind(id)
    .bind(claims.org_id)
    .bind(note)
    .bind(claims.user_id)
    .fetch_one(&pool)
    .await?;

    audit::log_custom_action(
        &pool,
        claims.org_id,
        claims.user_id,
        &claims.user_code,
        "ADD_INCIDENT_NOTE",
        "incident_notes",
        row.id,
        serde_json::json!({ "incident_id": id }),
    )
    .await?;

    Ok(Json(row))
}

/// Best-effort filesystem cleanup for every attachment file under a given
/// incident. Called by `delete_incident` (before the DB DELETE) and by
/// `purge::purge_incidents` (before the bulk DELETE), so the CASCADE doesn't
/// drop the rows before we've read their file_paths.
pub async fn unlink_files_for_incident(pool: &DbPool, incident_id: i32, org_id: i32) {
    let rows = match sqlx::query(
        "SELECT file_path FROM incident_attachments WHERE incident_id = $1 AND org_id = $2",
    )
    .bind(incident_id)
    .bind(org_id)
    .fetch_all(pool)
    .await
    {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(
                "unlink_files_for_incident: failed to list attachments for incident {}: {:?}",
                incident_id,
                e
            );
            return;
        }
    };

    for row in rows {
        let file_path: String = match row.try_get("file_path") {
            Ok(p) => p,
            Err(_) => continue,
        };
        let path = absolute_path(&file_path);
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                tracing::warn!(
                    "Failed to unlink attachment file {:?} (incident {}): {:?}",
                    path,
                    incident_id,
                    e
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{attachment_kind_and_mime, extension_of};

    #[test]
    fn ardupilot_bin_maps_to_octet_stream() {
        assert_eq!(
            attachment_kind_and_mime("bin"),
            Some(("ardupilot_bin", "application/octet-stream"))
        );
    }

    #[test]
    fn tlog_maps_to_ardupilot_tlog() {
        assert_eq!(
            attachment_kind_and_mime("tlog"),
            Some(("ardupilot_tlog", "application/octet-stream"))
        );
    }

    #[test]
    fn pdf_maps_to_document() {
        assert_eq!(
            attachment_kind_and_mime("pdf"),
            Some(("document", "application/pdf"))
        );
    }

    #[test]
    fn png_maps_to_image() {
        assert_eq!(
            attachment_kind_and_mime("png"),
            Some(("image", "image/png"))
        );
    }

    #[test]
    fn unknown_extension_is_rejected() {
        assert_eq!(attachment_kind_and_mime("exe"), None);
    }

    #[test]
    fn extension_is_lowercased_and_taken_after_last_dot() {
        assert_eq!(extension_of("Flight.Log.BIN").as_deref(), Some("bin"));
        assert_eq!(extension_of("noext"), None);
    }
}
