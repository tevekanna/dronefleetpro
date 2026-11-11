// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

use axum::{
    extract::State,
    Json,
};
use axum_extra::extract::Multipart;
use serde::{Deserialize, Serialize};
use crate::auth::middleware::{require_admin_or_technician, Claims};
use crate::db::DbPool;
use crate::errors::AppError;
use crate::services::audit;
use crate::services::equipment_ids::next_equipment_id;
use crate::services::qr_csv_parser::{
    format_display_name, parse_qr_csv, CsvFormat, ParsedQrRow,
};

// --- Response types ---

#[derive(Debug, Serialize)]
pub struct BatchImportPreview {
    pub entity_type: String,
    /// Total non-header rows the parser scanned (includes silently
    /// skipped, deduped, and rejected rows). Lets the preview UI show
    /// "scanned N rows → M valid SNs" without having to subtract.
    pub total_rows: usize,
    /// Valid SNs the parser kept — equal to `rows.len()`.
    pub total_parsed: usize,
    /// Within-file duplicates dropped (preserving first occurrence).
    pub duplicates_removed: usize,
    pub rows: Vec<BatchImportRow>,
    /// Per-row warnings (e.g. Tello rows that need manual SN correction).
    /// Shown in the preview UI so the operator sees them before committing.
    pub warnings: Vec<String>,
    /// Per-row errors (malformed CSV rows that were skipped). Not in `rows`.
    pub errors: Vec<String>,
    /// Machine-readable source-format slug (e.g. "binary_eye_v1").
    pub source_format: Option<String>,
    /// Human-readable label for the same format ("Binary Eye export").
    pub source_format_label: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct BatchImportRow {
    pub serial_number: String,
    pub status: String, // "new" | "duplicate" | "invalid"
    pub suggested_compatible_model: Option<String>,
    pub existing_id: Option<i32>,
    /// Extra QR data for aircraft (SSID, password, raw_qr, source_format).
    /// Echoed back to the commit endpoint so the row is persisted intact.
    pub device_metadata: Option<serde_json::Value>,
    /// TRUE for aircraft (e.g. Tello) where the QR didn't yield a real SN.
    /// The UI shows a warning and the column is persisted on commit.
    pub needs_manual_sn: bool,
    /// Manufacturer parsed from a Standard CSV column. Pre-fills the
    /// preview input; ignored for QR-scanner formats.
    pub manufacturer: Option<String>,
    /// Model parsed from a Standard CSV column. Pre-fills the preview
    /// input; ignored for QR-scanner formats.
    pub model: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct BatchCommitRequest {
    pub entity_type: String,
    pub rows: Vec<BatchCommitRow>,
}

#[derive(Debug, Deserialize)]
pub struct BatchCommitRow {
    pub serial_number: String,
    pub compatible_model: Option<String>,
    pub model: Option<String>,
    /// Used for aircraft only: when the supplied `model` is new to the
    /// per-org catalog, the auto-created `aircraft_models` row's
    /// `manufacturer` column is populated from this value. Ignored if
    /// the model already exists (we don't clobber existing manufacturer).
    pub manufacturer: Option<String>,
    pub nickname: Option<String>,
    pub max_cycles: Option<i32>,
    /// Round-tripped from the preview response. Persisted as-is for
    /// aircraft; ignored for batteries / controllers.
    #[serde(default)]
    pub device_metadata: Option<serde_json::Value>,
    /// Round-tripped from the preview response. Persisted as-is for
    /// aircraft; ignored for batteries / controllers.
    #[serde(default)]
    pub needs_manual_sn: bool,
}

#[derive(Debug, Serialize)]
pub struct BatchCommitResult {
    pub created: usize,
    pub skipped_duplicates: usize,
    pub entity_type: String,
}

// --- DJI battery serial prefix mapping ---

fn guess_compatible_model(serial: &str) -> Option<String> {
    let prefixes: &[(&str, &str)] = &[
        ("M61", "Matrice 600"),
        ("M3E", "Matrice 3E"),
        ("M30", "Matrice 30"),
        ("M3T", "Matrice 3T"),
        ("M3M", "Matrice 3M"),
        ("M2E", "Mavic 2 Enterprise"),
        ("M2P", "Mavic 2 Pro"),
        ("M3C", "Mavic 3 Classic"),
        ("M3P", "Mavic 3 Pro"),
        ("P4M", "Phantom 4 Multispectral"),
        ("P4P", "Phantom 4 Pro"),
        ("M4E", "Matrice 4E"),
        ("M4T", "Matrice 4T"),
    ];
    let upper = serial.to_uppercase();
    for (prefix, model) in prefixes {
        if upper.starts_with(prefix) {
            return Some(model.to_string());
        }
    }
    None
}

fn format_label(format: CsvFormat) -> &'static str {
    match format {
        CsvFormat::BinaryEye => "binary_eye_v1",
        CsvFormat::DjiScanner => "dji_qr_scanner_v1",
        CsvFormat::TelloScanner => "tello_qr_scanner_v1",
        CsvFormat::StandardCsv => "standard_csv_v1",
        CsvFormat::Generic => "generic",
    }
}

// --- POST /api/v1/fleet/batch-import (preview) ---

pub async fn preview(
    claims: Claims,
    State(pool): State<DbPool>,
    mut multipart: Multipart,
) -> Result<Json<BatchImportPreview>, AppError> {
    require_admin_or_technician(&claims)?;
    let mut file_bytes: Option<Vec<u8>> = None;
    let mut entity_type: Option<String> = None;

    while let Some(field) = multipart.next_field().await
        .map_err(|e| AppError::BadRequest(format!("Multipart error: {}", e)))? {

        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "file" => {
                file_bytes = Some(
                    field.bytes().await
                        .map_err(|e| AppError::BadRequest(format!("Failed to read file: {}", e)))?
                        .to_vec()
                );
            }
            "entity_type" => {
                entity_type = Some(
                    field.text().await
                        .map_err(|e| AppError::BadRequest(format!("Failed to read entity_type: {}", e)))?
                );
            }
            _ => {}
        }
    }

    let file_bytes = file_bytes
        .ok_or_else(|| AppError::BadRequest("Missing 'file' field".into()))?;
    let entity_type = entity_type
        .ok_or_else(|| AppError::BadRequest("Missing 'entity_type' field".into()))?;

    if !["aircraft", "battery", "controller"].contains(&entity_type.as_str()) {
        return Err(AppError::BadRequest(format!("Invalid entity_type: {}", entity_type)));
    }

    let parsed = parse_qr_csv(&file_bytes)?;
    let mut rows: Vec<BatchImportRow> = Vec::with_capacity(parsed.rows.len());

    for ParsedQrRow {
        serial_number,
        device_metadata,
        needs_manual_sn,
        manufacturer,
        model,
    } in &parsed.rows
    {
        if serial_number.is_empty() {
            rows.push(BatchImportRow {
                serial_number: serial_number.clone(),
                status: "invalid".to_string(),
                suggested_compatible_model: None,
                existing_id: None,
                device_metadata: device_metadata.clone(),
                needs_manual_sn: *needs_manual_sn,
                manufacturer: manufacturer.clone(),
                model: model.clone(),
            });
            continue;
        }

        let (exists, existing_id) = match entity_type.as_str() {
            "aircraft" => {
                let row: Option<(i32,)> = sqlx::query_as(
                    "SELECT id FROM aircraft WHERE org_id = $1 AND serial_number = $2"
                )
                .bind(claims.org_id)
                .bind(serial_number)
                .fetch_optional(&pool)
                .await?;
                (row.is_some(), row.map(|r| r.0))
            }
            "battery" => {
                let row: Option<(i32,)> = sqlx::query_as(
                    "SELECT id FROM batteries WHERE org_id = $1 AND serial_number = $2"
                )
                .bind(claims.org_id)
                .bind(serial_number)
                .fetch_optional(&pool)
                .await?;
                (row.is_some(), row.map(|r| r.0))
            }
            "controller" => {
                let row: Option<(i32,)> = sqlx::query_as(
                    "SELECT id FROM remote_controllers WHERE org_id = $1 AND serial_number = $2"
                )
                .bind(claims.org_id)
                .bind(serial_number)
                .fetch_optional(&pool)
                .await?;
                (row.is_some(), row.map(|r| r.0))
            }
            _ => (false, None),
        };

        let suggested = if entity_type == "battery" {
            guess_compatible_model(serial_number)
        } else {
            None
        };

        rows.push(BatchImportRow {
            serial_number: serial_number.clone(),
            status: if exists { "duplicate".to_string() } else { "new".to_string() },
            suggested_compatible_model: suggested,
            existing_id,
            device_metadata: device_metadata.clone(),
            needs_manual_sn: *needs_manual_sn,
            manufacturer: manufacturer.clone(),
            model: model.clone(),
        });
    }

    Ok(Json(BatchImportPreview {
        entity_type,
        total_rows: parsed.total_rows,
        total_parsed: rows.len(),
        duplicates_removed: parsed.duplicates_removed,
        rows,
        warnings: parsed.warnings,
        errors: parsed.errors,
        source_format: Some(format_label(parsed.format).to_string()),
        source_format_label: Some(format_display_name(parsed.format).to_string()),
    }))
}

// --- POST /api/v1/fleet/batch-commit ---

pub async fn commit(
    claims: Claims,
    State(pool): State<DbPool>,
    Json(body): Json<BatchCommitRequest>,
) -> Result<Json<BatchCommitResult>, AppError> {
    require_admin_or_technician(&claims)?;
    if !["aircraft", "battery", "controller"].contains(&body.entity_type.as_str()) {
        return Err(AppError::BadRequest(format!("Invalid entity_type: {}", body.entity_type)));
    }

    let valid_rows: Vec<&BatchCommitRow> = body.rows.iter()
        .filter(|r| !r.serial_number.trim().is_empty())
        .collect();

    let (table, prefix) = match body.entity_type.as_str() {
        "aircraft" => ("aircraft", "UAV-"),
        "battery" => ("batteries", "BAT-"),
        "controller" => ("remote_controllers", "CTL-"),
        _ => unreachable!(),
    };

    // Get starting equipment ID number
    let first_eid = next_equipment_id(&pool, claims.org_id, table, prefix).await?;
    let mut next_num: u32 = first_eid.trim_start_matches(prefix).parse().unwrap_or(1);

    let mut created: usize = 0;
    let mut skipped: usize = 0;

    for row in &valid_rows {
        let serial = row.serial_number.trim();
        let eid = format!("{}{:03}", prefix, next_num);

        let inserted: Option<(i32, serde_json::Value)> = match body.entity_type.as_str() {
            "aircraft" => {
                // Resolve the CSV-supplied model name to an existing
                // aircraft_models row, or auto-create one when the
                // operator typed a new name. Manufacturer is honoured
                // only on creation. If the model field is blank, leave
                // the aircraft's model_id NULL so the admin can assign
                // it from the edit form afterwards.
                let model_id = match row.model.as_deref() {
                    Some(s) if !s.trim().is_empty() => {
                        Some(
                            crate::services::aircraft_models::resolve_or_create(
                                &pool,
                                claims.org_id,
                                s.trim(),
                                row.manufacturer.as_deref(),
                            )
                            .await?,
                        )
                    }
                    _ => None,
                };
                // Per-row transaction so the aircraft INSERT and any
                // model-driven default schedules either both land or
                // both roll back. ON CONFLICT DO NOTHING returns no row
                // on a duplicate serial — in that case we don't seed
                // schedules and the tx commits empty.
                let mut tx = pool.begin().await?;
                let r = sqlx::query(
                    "INSERT INTO aircraft (org_id, serial_number, equipment_id, model_id, nickname, status, device_metadata, needs_manual_sn) \
                     VALUES ($1, $2, $3, $4, $5, 'active', $6, $7) \
                     ON CONFLICT (org_id, serial_number) DO NOTHING \
                     RETURNING id, to_jsonb(aircraft.*) AS row_json"
                )
                .bind(claims.org_id)
                .bind(serial)
                .bind(&eid)
                .bind(model_id)
                .bind(&row.nickname)
                .bind(&row.device_metadata)
                .bind(row.needs_manual_sn)
                .fetch_optional(&mut *tx)
                .await?;

                let result = if let Some(rec) = r {
                    use sqlx::Row;
                    let id: i32 = rec.get("id");
                    let rj: serde_json::Value = rec.get("row_json");

                    // Seed default maintenance schedules from this
                    // model's templates. Internal idempotency gate +
                    // SkippedNoModel/SkippedNoTemplates outcomes mean
                    // the call is safe even when model_id is None or
                    // the model has no templates yet.
                    crate::services::maintenance_defaults::insert_defaults_for_aircraft(
                        &mut tx,
                        claims.org_id,
                        id,
                        model_id,
                        &claims.user_code,
                    )
                    .await?;

                    Some((id, rj))
                } else {
                    None
                };
                tx.commit().await?;
                result
            }
            "battery" => {
                let max_cycles = row.max_cycles.unwrap_or(300);
                let r = sqlx::query(
                    "INSERT INTO batteries (org_id, serial_number, equipment_id, compatible_model, max_cycles, status) \
                     VALUES ($1, $2, $3, $4, $5, 'active') \
                     ON CONFLICT (org_id, serial_number) DO NOTHING \
                     RETURNING id, to_jsonb(batteries.*) AS row_json"
                )
                .bind(claims.org_id)
                .bind(serial)
                .bind(&eid)
                .bind(&row.compatible_model)
                .bind(max_cycles)
                .fetch_optional(&pool)
                .await?;
                r.and_then(|rec| {
                    use sqlx::Row;
                    let id: i32 = rec.try_get("id").ok()?;
                    let rj: serde_json::Value = rec.try_get("row_json").ok()?;
                    Some((id, rj))
                })
            }
            "controller" => {
                let r = sqlx::query(
                    "INSERT INTO remote_controllers (org_id, serial_number, equipment_id, model, nickname, status) \
                     VALUES ($1, $2, $3, $4, $5, 'active') \
                     ON CONFLICT (org_id, serial_number) DO NOTHING \
                     RETURNING id, to_jsonb(remote_controllers.*) AS row_json"
                )
                .bind(claims.org_id)
                .bind(serial)
                .bind(&eid)
                .bind(&row.model)
                .bind(&row.nickname)
                .fetch_optional(&pool)
                .await?;
                r.and_then(|rec| {
                    use sqlx::Row;
                    let id: i32 = rec.try_get("id").ok()?;
                    let rj: serde_json::Value = rec.try_get("row_json").ok()?;
                    Some((id, rj))
                })
            }
            _ => None,
        };

        if let Some((id, row_json)) = inserted {
            audit::log_insert_full(&pool, claims.org_id, claims.user_id, &claims.user_code, table, id, &row_json).await;

            // Best-effort body_sn identifier writeback. The operator
            // scanned this SN off a physical QR sticker, so it is the
            // authoritative body SN by definition — record it in the
            // matching *_identifiers table (aircraft from mig 025;
            // controllers + batteries from mig 030) so that a later DJI
            // log import can match through the identifier indirection
            // when the log carries the FC SN rather than the body SN.
            // Idempotent via ON CONFLICT (org_id, identifier, source).
            let identifier_result = match body.entity_type.as_str() {
                "aircraft" => crate::services::aircraft_identifiers::link_identifier(
                    &pool, id, claims.org_id, serial,
                    "body_sn", Some("qr_scan"), Some(claims.user_id),
                ).await,
                "battery" => crate::services::battery_identifiers::link_identifier(
                    &pool, id, claims.org_id, serial,
                    "body_sn", Some("qr_scan"), Some(claims.user_id),
                ).await,
                "controller" => crate::services::controller_identifiers::link_identifier(
                    &pool, id, claims.org_id, serial,
                    "body_sn", Some("qr_scan"), Some(claims.user_id),
                ).await,
                _ => Ok(None),
            };
            if let Err(e) = identifier_result {
                tracing::warn!(
                    error = ?e,
                    entity_type = %body.entity_type,
                    id,
                    serial = %serial,
                    "batch_import: failed to record body_sn identifier — equipment insert succeeded; identifier writeback is best-effort"
                );
            }

            created += 1;
            next_num += 1;
        } else {
            skipped += 1;
        }
    }

    Ok(Json(BatchCommitResult {
        created,
        skipped_duplicates: skipped,
        entity_type: body.entity_type,
    }))
}
