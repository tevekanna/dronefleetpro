// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

use axum::{
    extract::State,
    Json,
};
use axum_extra::extract::Multipart;

use crate::auth::middleware::{require_role, Claims};
use crate::db::DbPool;
use crate::errors::AppError;
use crate::services::sn_matching::find_match;

pub async fn import_dji_log(
    claims: Claims,
    State(pool): State<DbPool>,
    mut multipart: Multipart,
) -> Result<Json<serde_json::Value>, AppError> {
    require_role(
        &claims,
        &["admin", "safety_officer", "technician", "pilot"],
    )?;

    let mut file_bytes: Option<Vec<u8>> = None;
    let mut filename = String::from("upload.csv");

    while let Some(field) = multipart.next_field().await
        .map_err(|e| AppError::BadRequest(format!("Multipart error: {}", e)))?
    {
        if field.name() == Some("file") {
            if let Some(fname) = field.file_name() {
                filename = fname.to_string();
            }
            let bytes = field.bytes().await
                .map_err(|e| AppError::BadRequest(format!("Failed to read file: {}", e)))?;
            file_bytes = Some(bytes.to_vec());
        }
    }

    let bytes = file_bytes.ok_or_else(|| AppError::BadRequest("No file field in request".into()))?;

    // Fetch DJI API key from org settings (needed for binary .txt v13+ logs)
    let api_key: Option<String> = sqlx::query(
        "SELECT value FROM settings WHERE org_id = $1 AND key = 'dji_api_key'"
    )
    .bind(claims.org_id)
    .fetch_optional(&pool)
    .await?
    .map(|row| {
        use sqlx::Row;
        row.get::<String, _>("value")
    })
    .filter(|v| !v.is_empty());

    // Parse the file — parser sets model_detection_status to "from_recover"
    // (v11+ logs), "from_details" (v9 fallback via parser.details), or
    // "unknown" based purely on what the log itself carried.
    let mut preview = crate::services::dji_parser::parse(&bytes, &filename, api_key.as_deref()).await?;

    use sqlx::Row;

    // Resolve equipment matches against the org's existing aircraft /
    // batteries / controllers. `find_match` applies the same exact-then-
    // prefix-tolerance lookup that `commit.rs` uses to decide auto-link
    // vs auto-register, so the preview UI cannot disagree with what
    // commit will actually do.
    if let Some(serial) = preview.aircraft_serial.as_deref().filter(|s| !s.is_empty()) {
        preview.aircraft_match =
            find_match(&pool, "aircraft", claims.org_id, serial).await?;
    }
    if let Some(serial) = preview.battery_serial.as_deref().filter(|s| !s.is_empty()) {
        preview.battery_match =
            find_match(&pool, "batteries", claims.org_id, serial).await?;
    }
    if let Some(serial) = preview.rc_serial.as_deref().filter(|s| !s.is_empty()) {
        preview.controller_match =
            find_match(&pool, "remote_controllers", claims.org_id, serial).await?;
    }

    // Model-resolution fallback chain, highest-trust source first:
    //   1. Aircraft registered in DB by serial → authoritative model name
    //   2. drone_type_mappings hit for (app_source, drone_type_code)
    //   3. recover.aircraft_name (already placed by the parser) → "from_recover"
    //   4. Nothing matched → "unknown"
    let mut resolved_status: Option<String> = None;

    // 1) aircraft from the resolved match — reuses find_match's
    //    prefix-tolerance, so model name resolves even when the log SN
    //    is truncated by 1 char (dji-log-parser bug).
    if let Some(ac_match) = preview.aircraft_match.as_ref() {
        let row = sqlx::query(
            "SELECT am.name AS model \
             FROM aircraft a LEFT JOIN aircraft_models am ON a.model_id = am.id \
             WHERE a.id = $1 AND a.org_id = $2"
        )
        .bind(ac_match.id)
        .bind(claims.org_id)
        .fetch_optional(&pool)
        .await?;

        if let Some(r) = row {
            let model: Option<String> = r.try_get("model").ok().flatten();
            if let Some(m) = model.filter(|s| !s.trim().is_empty()) {
                // Set both fields: aircraft_name is the display label the
                // UI shows, aircraft_model is the catalog lookup key the
                // commit handler uses for auto-register. They share the
                // same canonical value when the aircraft is already in
                // the catalog.
                preview.aircraft_name = Some(m.clone());
                preview.aircraft_model = Some(m);
                resolved_status = Some("from_aircraft_db".to_string());
            }
        }
    }

    // 2) drone_type_mappings — the mapping now FKs to aircraft_models, so
    //    we JOIN through to fetch the canonical name.
    if resolved_status.is_none() {
        if let (Some(type_code), Some(app_src)) = (&preview.drone_type_code, &preview.app_source) {
            let learned = sqlx::query(
                "SELECT am.name AS model_name \
                 FROM drone_type_mappings dtm \
                 JOIN aircraft_models am ON dtm.model_id = am.id \
                 WHERE dtm.org_id = $1 AND dtm.app_source = $2 AND dtm.drone_type_code = $3 \
                 LIMIT 1"
            )
            .bind(claims.org_id)
            .bind(app_src)
            .bind(type_code)
            .fetch_optional(&pool)
            .await?;

            if let Some(row) = learned {
                let name: String = row.get("model_name");
                if !name.trim().is_empty() {
                    preview.aircraft_name = Some(name.clone());
                    preview.aircraft_model = Some(name);
                    resolved_status = Some("from_mapping".to_string());
                }
            }
        }
    }

    // 3) Parser already placed an aircraft_name in the preview, sourced from
    //    either recover (v11+) or parser.details (v9 fallback). Preserve
    //    whichever the parser reported so logs surface the actual source.
    if resolved_status.is_none() && preview.aircraft_name.is_some() {
        resolved_status = preview.model_detection_status.clone()
            .filter(|s| s == "from_recover" || s == "from_details")
            .or_else(|| Some("from_recover".to_string()));
    }

    // 4) Nothing found
    preview.model_detection_status = Some(resolved_status.unwrap_or_else(|| "unknown".to_string()));

    tracing::info!(
        "Model detection for '{}': aircraft_serial={:?} → {}, aircraft_name={:?}",
        preview.source_filename,
        preview.aircraft_serial,
        preview.model_detection_status.as_deref().unwrap_or("?"),
        preview.aircraft_name
    );

    // Primary check: source_filename (exact match)
    let existing_by_filename = sqlx::query(
        "SELECT id FROM flights WHERE org_id = $1 AND source_filename = $2 AND deleted_at IS NULL LIMIT 1"
    )
    .bind(claims.org_id)
    .bind(&preview.source_filename)
    .fetch_optional(&pool)
    .await
    .map_err(|e| { tracing::error!("dedup-primary query failed: {e}"); AppError::Database(e) })?;

    if existing_by_filename.is_some() {
        return Err(AppError::BadRequest(format!(
            "Flight log '{}' has already been imported.", preview.source_filename
        )));
    }

    // Secondary check: same aircraft + same start time (catches renamed files)
    if let (Some(serial), Some(start)) = (&preview.aircraft_serial, &preview.flight_start) {
        if !start.starts_with("1970") {
            let existing_by_time = sqlx::query(
                "SELECT id FROM flights WHERE org_id = $1 AND aircraft_serial = $2 AND flight_start = $3::timestamptz AND deleted_at IS NULL LIMIT 1"
            )
            .bind(claims.org_id)
            .bind(serial)
            .bind(start)
            .fetch_optional(&pool)
            .await
            .map_err(|e| { tracing::error!("dedup-secondary query failed (start='{}'): {e}", start); AppError::Database(e) })?;

            if existing_by_time.is_some() {
                return Err(AppError::BadRequest(
                    "A flight with the same aircraft and start time has already been imported. \
                     This appears to be a duplicate even though the filename is different.".to_string()
                ));
            }
        }
    }

    Ok(Json(serde_json::json!({
        "preview": preview,
        "filename": filename,
        "message": "File parsed successfully. Review and confirm to save."
    })))
}

/// ArduPilot `.tlog` import preview. Unlike the DJI path (one file → one
/// flight), a single `.tlog` is a GCS connection session that yields 0..N
/// arm/disarm segments, each a separate flight sharing ONE airframe (the
/// autopilot board UID). We resolve that UID against the fleet once and stamp
/// the result onto every segment, then flag any segment whose flight_guid /
/// source_filename already exists so the review UI can disable it.
///
/// `.bin` (DataFlash) import is a separate, richer path (PR C); it is rejected
/// here with a pointer to `.tlog`.
pub async fn import_ardupilot_log(
    claims: Claims,
    State(pool): State<DbPool>,
    mut multipart: Multipart,
) -> Result<Json<serde_json::Value>, AppError> {
    require_role(
        &claims,
        &["admin", "safety_officer", "technician", "pilot"],
    )?;

    let mut file_bytes: Option<Vec<u8>> = None;
    let mut filename = String::from("upload.tlog");

    while let Some(field) = multipart.next_field().await
        .map_err(|e| AppError::BadRequest(format!("Multipart error: {}", e)))?
    {
        if field.name() == Some("file") {
            if let Some(fname) = field.file_name() {
                filename = fname.to_string();
            }
            let bytes = field.bytes().await
                .map_err(|e| AppError::BadRequest(format!("Failed to read file: {}", e)))?;
            file_bytes = Some(bytes.to_vec());
        }
    }

    let bytes = file_bytes.ok_or_else(|| AppError::BadRequest("No file field in request".into()))?;

    // Pick the parser by extension. Both produce the shared
    // ArduPilotSegmentPreview contract, so the shared post-parse logic below
    // (UID match, per-segment dedup, response shape) runs identically for both.
    let lower = filename.to_lowercase();
    let mut segments = if lower.ends_with(".tlog") {
        crate::services::ardupilot_tlog::parse_tlog(&bytes, &filename)?
    } else if lower.ends_with(".bin") {
        // .bin (DataFlash) parsing is CPU-heavy on large logs (multi-million
        // entries), so run it on the blocking pool to avoid stalling the async
        // runtime. spawn_blocking needs owned data.
        let bytes_owned = bytes.clone();
        let fname = filename.clone();
        tokio::task::spawn_blocking(move || {
            crate::services::ardupilot_bin::parse_bin(&bytes_owned, &fname)
        })
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("bin parse task failed: {e}")))??
    } else {
        return Err(AppError::BadRequest(format!(
            "Unsupported ArduPilot log format: {filename}. Use a .tlog or .bin file."
        )));
    };

    use sqlx::Row;

    // Resolve the shared airframe ONCE from the board UID — every segment in a
    // .tlog comes from the same autopilot, so they share aircraft_serial.
    let shared_uid: Option<String> = segments
        .iter()
        .find_map(|s| s.preview.aircraft_serial.as_deref().filter(|v| !v.is_empty()))
        .map(|s| s.to_string());

    let mut shared_match = None;
    if let Some(uid) = shared_uid.as_deref() {
        if let Some(ac_match) = find_match(&pool, "aircraft", claims.org_id, uid).await? {
            // Resolve the catalog model name for the matched aircraft and stamp
            // the "from_aircraft_db" provenance onto every segment, mirroring
            // import_dji_log's highest-trust branch.
            let model_row = sqlx::query(
                "SELECT am.name AS model \
                 FROM aircraft a LEFT JOIN aircraft_models am ON a.model_id = am.id \
                 WHERE a.id = $1 AND a.org_id = $2",
            )
            .bind(ac_match.id)
            .bind(claims.org_id)
            .fetch_optional(&pool)
            .await?;
            let model_name: Option<String> = model_row
                .and_then(|r| r.try_get::<Option<String>, _>("model").ok().flatten())
                .filter(|s| !s.trim().is_empty());

            for seg in segments.iter_mut() {
                seg.preview.aircraft_match = Some(ac_match.clone());
                if let Some(ref m) = model_name {
                    seg.preview.aircraft_name = Some(m.clone());
                    seg.preview.aircraft_model = Some(m.clone());
                }
                seg.preview.model_detection_status = Some("from_aircraft_db".to_string());
            }
            shared_match = Some(ac_match);
        }
    }

    // Per-segment dedup: flag (never reject) segments whose flight_guid or
    // source_filename already exists as a live flight. The actual UNIQUE guard
    // is enforced at commit time.
    for seg in segments.iter_mut() {
        let guid = seg.preview.flight_guid.clone().unwrap_or_default();
        let dup = sqlx::query(
            "SELECT id FROM flights \
             WHERE org_id = $1 AND deleted_at IS NULL AND (flight_guid = $2 OR source_filename = $3) \
             LIMIT 1",
        )
        .bind(claims.org_id)
        .bind(&guid)
        .bind(&seg.preview.source_filename)
        .fetch_optional(&pool)
        .await?;
        seg.already_imported = dup.is_some();
    }

    let count = segments.len();
    let new_count = segments.iter().filter(|s| !s.already_imported).count();

    Ok(Json(serde_json::json!({
        "filename": filename,
        "aircraft_match": shared_match,
        "segments": segments,
        "message": format!(
            "Parsed {count} flight segment(s) from telemetry log; {new_count} new, {} already imported.",
            count - new_count
        ),
    })))
}
