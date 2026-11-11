// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

//! Data import logic — natural-key matching, FK remapping, conflict
//! resolution, and single-transaction atomicity.
//!
//! Both `preview_import` (dry run) and `apply_import` (commit) share the
//! same core logic via the `dry_run` flag in [`run_import`]. This
//! guarantees that the preview's reported changes match what the apply
//! actually performs.
//!
//! Dynamic SQL is generated via PostgreSQL's `jsonb_populate_record`:
//! each row arrives as a JSONB document, PG coerces it to the target
//! row type, and INSERT / UPDATE bind a single JSONB parameter. This
//! avoids per-table struct definitions and keeps the import service
//! schema-agnostic for new columns added in future migrations.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sqlx::{PgPool, Postgres, Row, Transaction};

use crate::errors::AppError;
use crate::services::audit;
use crate::services::data_transfer_schema::{
    is_forbidden_on_import, is_known_table, is_silently_ignored_on_import, EntityGroup,
    CANONICAL_TABLES,
};

const MAX_CONFLICT_DETAILS: usize = 100;

#[cfg(test)]
#[path = "data_import_db_tests.rs"]
mod db_tests;

// =====================================================================
// Public types
// =====================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ConflictStrategy {
    Skip,
    Update,
    Fail,
}

#[derive(Debug, Serialize, Clone)]
pub struct ImportSummary {
    pub per_table: Map<String, Value>,
    pub total_created: i64,
    pub total_updated: i64,
    pub total_skipped: i64,
    pub total_failed: i64,
    pub conflicts: Vec<ConflictDetail>,
    pub conflicts_truncated: bool,
    pub format_version: i32,
    pub source_org_id: Option<i32>,
    pub dry_run: bool,
}

#[derive(Debug, Serialize, Clone)]
pub struct ConflictDetail {
    pub table: String,
    pub natural_key: Value,
    pub action: String,
    pub reason: Option<String>,
}

// =====================================================================
// Public entry points
// =====================================================================

pub async fn preview_import(
    pool: &PgPool,
    target_org_id: i32,
    actor_user_id: i32,
    actor_user_code: &str,
    document: Value,
    strategy: ConflictStrategy,
    skip_groups: &[EntityGroup],
) -> Result<ImportSummary, AppError> {
    run_import(
        pool,
        target_org_id,
        actor_user_id,
        actor_user_code,
        document,
        strategy,
        skip_groups,
        true,
    )
    .await
}

pub async fn apply_import(
    pool: &PgPool,
    target_org_id: i32,
    actor_user_id: i32,
    actor_user_code: &str,
    document: Value,
    strategy: ConflictStrategy,
    skip_groups: &[EntityGroup],
) -> Result<ImportSummary, AppError> {
    run_import(
        pool,
        target_org_id,
        actor_user_id,
        actor_user_code,
        document,
        strategy,
        skip_groups,
        false,
    )
    .await
}

// =====================================================================
// Document validation
// =====================================================================

struct ValidatedDocument {
    source_org_id: Option<i32>,
    entity_groups: Value,
    tables: Map<String, Value>,
}

fn validate_document(doc: &Value) -> Result<ValidatedDocument, AppError> {
    let obj = doc
        .as_object()
        .ok_or_else(|| AppError::BadRequest("document must be a JSON object".into()))?;

    let format_version = obj
        .get("format_version")
        .and_then(Value::as_i64)
        .ok_or_else(|| {
            AppError::BadRequest("format_version is required and must be an integer".into())
        })?;
    if format_version != 1 {
        return Err(AppError::BadRequest(format!(
            "Unsupported format_version: {format_version}. This server supports format_version 1"
        )));
    }

    let tables = obj
        .get("tables")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            AppError::BadRequest("tables is required and must be an object".into())
        })?
        .clone();

    for (key, value) in &tables {
        if is_forbidden_on_import(key) {
            return Err(AppError::BadRequest(format!(
                "{key} table cannot be imported"
            )));
        }
        if !is_known_table(key) && !is_silently_ignored_on_import(key) {
            return Err(AppError::BadRequest(format!(
                "Unknown table in import document: {key}"
            )));
        }
        let arr = value.as_array().ok_or_else(|| {
            AppError::BadRequest(format!("tables.{key} must be an array"))
        })?;
        for (i, row) in arr.iter().enumerate() {
            if !row.is_object() {
                return Err(AppError::BadRequest(format!(
                    "tables.{key}[{i}] must be an object"
                )));
            }
        }
    }

    Ok(ValidatedDocument {
        source_org_id: obj.get("source_org_id").and_then(Value::as_i64).map(|n| n as i32),
        entity_groups: obj.get("entity_groups").cloned().unwrap_or(Value::Null),
        tables,
    })
}

// =====================================================================
// FK specs
// =====================================================================

#[derive(Debug, Clone, Copy)]
enum FKAction {
    NullOnUnresolved,
    FailRow,
}

#[derive(Debug, Clone, Copy)]
struct FKSpec {
    col: &'static str,
    refs: &'static str,
    action: FKAction,
}

fn fk_specs(table: &str) -> &'static [FKSpec] {
    match table {
        "aircraft" => &[FKSpec { col: "model_id", refs: "aircraft_models", action: FKAction::NullOnUnresolved }],
        // flight_id is NOT NULL → FailRow. org_id is intentionally NOT listed:
        // the drift test skips org_id generically, and process_row overwrites
        // it to the target org (an FKSpec here would try to remap it and fail).
        "flight_log_findings" => &[FKSpec { col: "flight_id", refs: "flights", action: FKAction::FailRow }],
        "aircraft_identifiers" => &[FKSpec { col: "aircraft_id", refs: "aircraft", action: FKAction::FailRow }],
        "batteries" => &[
            FKSpec { col: "aircraft_id", refs: "aircraft", action: FKAction::NullOnUnresolved },
            // Structured compatibility (migration 052); nullable, so an
            // unresolved model reference degrades to NULL rather than
            // failing the battery row.
            FKSpec { col: "compatible_model_id", refs: "aircraft_models", action: FKAction::NullOnUnresolved },
        ],
        "battery_identifiers" => &[FKSpec { col: "battery_id", refs: "batteries", action: FKAction::FailRow }],
        "remote_controllers" => &[FKSpec { col: "aircraft_id", refs: "aircraft", action: FKAction::NullOnUnresolved }],
        "controller_identifiers" => &[FKSpec { col: "controller_id", refs: "remote_controllers", action: FKAction::FailRow }],
        "accessories" => &[FKSpec { col: "aircraft_id", refs: "aircraft", action: FKAction::NullOnUnresolved }],
        "pilot_qualifications" => &[FKSpec { col: "pilot_id", refs: "pilots", action: FKAction::FailRow }],
        "flights" => &[
            FKSpec { col: "pilot_id", refs: "pilots", action: FKAction::FailRow },
            FKSpec { col: "pilot_monitoring_id", refs: "pilots", action: FKAction::NullOnUnresolved },
            FKSpec { col: "aircraft_id", refs: "aircraft", action: FKAction::FailRow },
            FKSpec { col: "battery_id", refs: "batteries", action: FKAction::NullOnUnresolved },
            FKSpec { col: "controller_id", refs: "remote_controllers", action: FKAction::NullOnUnresolved },
        ],
        // model_id is NOT NULL (migration 020) → FailRow.
        "maintenance_schedule_templates" => &[FKSpec { col: "model_id", refs: "aircraft_models", action: FKAction::FailRow }],
        // aircraft_model_id is nullable; NULL means a generic template (mig 035).
        "maintenance_instruction_templates" => &[FKSpec { col: "aircraft_model_id", refs: "aircraft_models", action: FKAction::NullOnUnresolved }],
        "maintenance_checklist_items" => &[FKSpec { col: "template_id", refs: "maintenance_instruction_templates", action: FKAction::FailRow }],
        "maintenance_schedules" => &[
            FKSpec { col: "aircraft_id", refs: "aircraft", action: FKAction::FailRow },
            FKSpec { col: "instruction_template_id", refs: "maintenance_instruction_templates", action: FKAction::NullOnUnresolved },
        ],
        "maintenance" => &[
            FKSpec { col: "aircraft_id", refs: "aircraft", action: FKAction::FailRow },
            FKSpec { col: "schedule_id", refs: "maintenance_schedules", action: FKAction::NullOnUnresolved },
            FKSpec { col: "instruction_template_id", refs: "maintenance_instruction_templates", action: FKAction::NullOnUnresolved },
            // Nullable FKs (migrations 031 / 027). Both parents are processed
            // before maintenance in CANONICAL_TABLES, so lookup_map holds them
            // when present in the document.
            FKSpec { col: "controller_id", refs: "remote_controllers", action: FKAction::NullOnUnresolved },
            FKSpec { col: "flight_id", refs: "flights", action: FKAction::NullOnUnresolved },
        ],
        "maintenance_checklist_completions" => &[
            FKSpec { col: "maintenance_id", refs: "maintenance", action: FKAction::FailRow },
            FKSpec { col: "checklist_item_id", refs: "maintenance_checklist_items", action: FKAction::FailRow },
        ],
        "maintenance_drafts" => &[FKSpec { col: "schedule_id", refs: "maintenance_schedules", action: FKAction::FailRow }],
        "defect_log" => &[
            FKSpec { col: "aircraft_id", refs: "aircraft", action: FKAction::NullOnUnresolved },
            // Nullable FK (migration 027); flights precede defect_log.
            FKSpec { col: "flight_id", refs: "flights", action: FKAction::NullOnUnresolved },
        ],
        "battery_tests" => &[FKSpec { col: "battery_id", refs: "batteries", action: FKAction::FailRow }],
        "incidents" => &[
            FKSpec { col: "flight_id", refs: "flights", action: FKAction::NullOnUnresolved },
            FKSpec { col: "pilot_id", refs: "pilots", action: FKAction::NullOnUnresolved },
            FKSpec { col: "aircraft_id", refs: "aircraft", action: FKAction::NullOnUnresolved },
        ],
        "incident_notes" => &[FKSpec { col: "incident_id", refs: "incidents", action: FKAction::FailRow }],
        // model_id is NOT NULL (migration 034) → FailRow. created_by / updated_by
        // reference users and are handled by fixup_user_columns.
        "checklists" => &[FKSpec { col: "model_id", refs: "aircraft_models", action: FKAction::FailRow }],
        "drone_type_mappings" => &[FKSpec { col: "model_id", refs: "aircraft_models", action: FKAction::FailRow }],
        "audit_log" => &[FKSpec { col: "user_id", refs: "users", action: FKAction::NullOnUnresolved }],
        _ => &[],
    }
}

/// Fix up columns that reference `users`. Users are never exported, so any
/// user id in the document is a source-instance id that is meaningless
/// here and can never resolve via `lookup_map`. For each affected table:
///
/// - NOT NULL user FKs (`maintenance_instruction_templates.created_by`,
///   mig 035; `checklists.created_by`, mig 034) must point at a real local
///   user, so we set them to the importing actor (the admin running the
///   import).
/// - Nullable user FKs (the `created_by` columns on the identifier tables and
///   `incident_notes`, `incidents.created_by_user_id` from mig 022, and
///   `checklists.updated_by`) are set to NULL rather than dangle a foreign id.
///
/// `organisation_branding.updated_by` and `maintenance_schedule_templates.
/// created_by` are deliberately untouched: the former has no FK constraint,
/// the latter is free-text (a user_code, not a user id). `audit_log.user_id`
/// is handled by an FKSpec (nulled when unresolved), not here.
///
/// Every user-referencing FK column in CANONICAL_TABLES is covered either
/// here or by an FKSpec — the `fk_specs_cover_all_schema_fks` drift test
/// enforces that.
fn fixup_user_columns(table: &str, row: &mut Map<String, Value>, actor_user_id: i32) {
    match table {
        "maintenance_instruction_templates" => {
            row.insert("created_by".to_string(), Value::from(actor_user_id));
        }
        "aircraft_identifiers"
        | "controller_identifiers"
        | "battery_identifiers"
        | "incident_notes" => {
            row.insert("created_by".to_string(), Value::Null);
        }
        "incidents" => {
            row.insert("created_by_user_id".to_string(), Value::Null);
        }
        "checklists" => {
            // created_by is NOT NULL → importing actor; updated_by is nullable.
            row.insert("created_by".to_string(), Value::from(actor_user_id));
            row.insert("updated_by".to_string(), Value::Null);
        }
        _ => {}
    }
}

// =====================================================================
// Job tracking
// =====================================================================

async fn insert_job_row(
    pool: &PgPool,
    org_id: i32,
    user_id: i32,
    user_code: &str,
    entity_groups: &Value,
    skip_groups: &[EntityGroup],
    dry_run: bool,
) -> Result<i64, AppError> {
    let options = serde_json::json!({
        "dry_run": dry_run,
        "skip_groups": skip_groups.iter().map(|g| g.as_str()).collect::<Vec<_>>(),
    });
    let row = sqlx::query(
        "INSERT INTO data_transfer_jobs \
            (org_id, user_id, user_code, job_type, status, entity_groups, options) \
         VALUES ($1, $2, $3, 'import', 'running', $4, $5) \
         RETURNING id",
    )
    .bind(org_id)
    .bind(user_id)
    .bind(user_code)
    .bind(entity_groups)
    .bind(options)
    .fetch_one(pool)
    .await?;
    Ok(row.try_get("id")?)
}

async fn finalize_job(pool: &PgPool, job_id: i64, summary: &ImportSummary) {
    let summary_json = serde_json::to_value(summary).unwrap_or(Value::Null);
    let _ = sqlx::query(
        "UPDATE data_transfer_jobs \
         SET status = 'completed', summary = $1, completed_at = NOW() \
         WHERE id = $2",
    )
    .bind(&summary_json)
    .bind(job_id)
    .execute(pool)
    .await;
}

async fn fail_job(pool: &PgPool, job_id: i64, message: &str) {
    let _ = sqlx::query(
        "UPDATE data_transfer_jobs \
         SET status = 'failed', error_message = $1, completed_at = NOW() \
         WHERE id = $2",
    )
    .bind(message)
    .bind(job_id)
    .execute(pool)
    .await;
}

// =====================================================================
// Core driver
// =====================================================================

#[allow(clippy::too_many_arguments)]
async fn run_import(
    pool: &PgPool,
    target_org_id: i32,
    actor_user_id: i32,
    actor_user_code: &str,
    document: Value,
    strategy: ConflictStrategy,
    skip_groups: &[EntityGroup],
    dry_run: bool,
) -> Result<ImportSummary, AppError> {
    let validated = validate_document(&document)?;
    let job_id = insert_job_row(
        pool,
        target_org_id,
        actor_user_id,
        actor_user_code,
        &validated.entity_groups,
        skip_groups,
        dry_run,
    )
    .await?;

    let result = run_import_in_tx(
        pool,
        target_org_id,
        actor_user_id,
        &validated,
        strategy,
        skip_groups,
        dry_run,
    )
    .await;

    match result {
        Ok(summary) => {
            finalize_job(pool, job_id, &summary).await;
            if !dry_run {
                let summary_json = serde_json::to_value(&summary).unwrap_or(Value::Null);
                audit::log_import(
                    pool,
                    target_org_id,
                    actor_user_id,
                    actor_user_code,
                    job_id,
                    summary_json,
                )
                .await;
            }
            Ok(summary)
        }
        Err(e) => {
            fail_job(pool, job_id, &format!("{e:?}")).await;
            Err(e)
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_import_in_tx(
    pool: &PgPool,
    target_org_id: i32,
    actor_user_id: i32,
    doc: &ValidatedDocument,
    strategy: ConflictStrategy,
    skip_groups: &[EntityGroup],
    dry_run: bool,
) -> Result<ImportSummary, AppError> {
    let mut tx = pool.begin().await?;

    let mut summary = ImportSummary {
        per_table: Map::new(),
        total_created: 0,
        total_updated: 0,
        total_skipped: 0,
        total_failed: 0,
        conflicts: Vec::new(),
        conflicts_truncated: false,
        format_version: 1,
        source_org_id: doc.source_org_id,
        dry_run,
    };

    let mut lookup_map: HashMap<(String, i64), i64> = HashMap::new();
    let mut column_cache: HashMap<String, Vec<String>> = HashMap::new();

    for (table_name, group) in CANONICAL_TABLES {
        let rows_arr = match doc.tables.get(*table_name) {
            Some(Value::Array(arr)) => arr,
            _ => continue,
        };

        let mut counts = TableCounts::default();

        if skip_groups.contains(group) {
            // The table's entity group was excluded by the caller's import
            // options. Skip every row without touching the DB and record a
            // single summary entry (rows still counted so totals reconcile).
            if !rows_arr.is_empty() {
                counts.skipped = rows_arr.len() as i64;
                push_conflict(
                    &mut summary,
                    table_name,
                    serde_json::json!({ "skipped_rows": rows_arr.len() }),
                    "skipped",
                    Some("entity group excluded by import options".to_string()),
                    dry_run,
                );
            }
        } else {
            for row_value in rows_arr {
                let row_obj = match row_value.as_object() {
                    Some(o) => o.clone(),
                    None => {
                        counts.failed += 1;
                        continue;
                    }
                };
                process_row(
                    &mut tx,
                    table_name,
                    row_obj,
                    target_org_id,
                    actor_user_id,
                    strategy,
                    dry_run,
                    &mut lookup_map,
                    &mut column_cache,
                    &mut counts,
                    &mut summary,
                )
                .await?;
            }
        }
        summary.total_created += counts.created;
        summary.total_updated += counts.updated;
        summary.total_skipped += counts.skipped;
        summary.total_failed += counts.failed;
        summary.per_table.insert((*table_name).to_string(), counts.to_value());
    }

    if dry_run {
        tx.rollback().await?;
    } else {
        tx.commit().await?;
    }

    Ok(summary)
}

#[derive(Default)]
struct TableCounts {
    created: i64,
    updated: i64,
    skipped: i64,
    failed: i64,
}

impl TableCounts {
    fn to_value(&self) -> Value {
        Value::Object(Map::from_iter([
            ("created".to_string(), Value::from(self.created)),
            ("updated".to_string(), Value::from(self.updated)),
            ("skipped".to_string(), Value::from(self.skipped)),
            ("failed".to_string(), Value::from(self.failed)),
        ]))
    }
}

// =====================================================================
// Per-row processing
// =====================================================================

#[allow(clippy::too_many_arguments)]
async fn process_row(
    tx: &mut Transaction<'_, Postgres>,
    table: &str,
    mut row: Map<String, Value>,
    target_org_id: i32,
    actor_user_id: i32,
    strategy: ConflictStrategy,
    dry_run: bool,
    lookup_map: &mut HashMap<(String, i64), i64>,
    column_cache: &mut HashMap<String, Vec<String>>,
    counts: &mut TableCounts,
    summary: &mut ImportSummary,
) -> Result<(), AppError> {
    let old_id = row.get("id").and_then(Value::as_i64);

    // Always overwrite org_id with target. Tables without an org_id column
    // (maintenance_checklist_items / _completions) simply ignore this key —
    // jsonb_populate_record drops JSON fields with no matching column.
    row.insert("org_id".to_string(), Value::from(target_org_id));

    // Rewrite user-referencing columns before FK remap / insert: source
    // user ids are meaningless across instances. See fixup_user_columns.
    fixup_user_columns(table, &mut row, actor_user_id);

    // FK remap. May produce a "fail this row" outcome.
    let fk_outcome = remap_fks(table, &mut row, lookup_map, summary);
    if matches!(fk_outcome, FkOutcome::Failed) {
        counts.failed += 1;
        return Ok(());
    }

    // audit_log: insert verbatim with ON CONFLICT (id) DO NOTHING.
    if table == "audit_log" {
        let cols = columns_for(tx, table, column_cache).await?;
        insert_audit_row(tx, &cols, &row, dry_run, counts).await?;
        return Ok(());
    }

    // flights: clear deleted_at — soft-delete semantics are scoped to the
    // source instance. Importing a row implicitly resurrects it.
    if table == "flights" {
        row.insert("deleted_at".to_string(), Value::Null);
    }

    // Look up existing target-org row by natural key.
    let existing = find_existing(tx, table, &row, target_org_id).await?;

    match (existing, strategy) {
        // Conflict detected.
        (Some((existing_id, nk)), strat) => {
            // Settings: scrubbed-value protection.
            if table == "settings" {
                handle_existing_setting(
                    tx, &row, existing_id, strat, dry_run, counts, summary, &nk,
                )
                .await?;
                if let Some(oid) = old_id {
                    lookup_map.insert((table.to_string(), oid), existing_id);
                }
                return Ok(());
            }

            // organisation_branding: composite PK, no id column — the generic
            // update_row (WHERE t.id = $2) cannot address it. Nothing
            // references branding by id, so no lookup_map entry is needed.
            if table == "organisation_branding" {
                handle_existing_branding(
                    tx, &row, strat, dry_run, counts, summary, &nk, column_cache,
                )
                .await?;
                return Ok(());
            }

            match strat {
                ConflictStrategy::Skip => {
                    counts.skipped += 1;
                    push_conflict(summary, table, nk, "skipped", None, dry_run);
                }
                ConflictStrategy::Fail => {
                    if dry_run {
                        // Preview mode: record would-fail and keep counting so
                        // the user sees the full picture instead of a 409 on
                        // the first conflict.
                        counts.failed += 1;
                        push_conflict(
                            summary,
                            table,
                            nk,
                            "would_fail",
                            Some(
                                "existing row matches natural key — would import under Skip/Update"
                                    .to_string(),
                            ),
                            dry_run,
                        );
                    } else {
                        return Err(AppError::Conflict(format!(
                            "Import aborted: existing row in {table} matches natural key {nk}"
                        )));
                    }
                }
                ConflictStrategy::Update => {
                    update_row(tx, table, &row, existing_id, column_cache).await?;
                    counts.updated += 1;
                    push_conflict(summary, table, nk, "updated", None, dry_run);
                }
            }
            if let Some(oid) = old_id {
                lookup_map.insert((table.to_string(), oid), existing_id);
            }
        }

        // No existing row — INSERT.
        (None, _) => {
            // Settings: insert literal SCRUBBED value if no prior — flag it.
            if table == "settings" {
                if let Some(v) = row.get("value") {
                    if v.as_str() == Some("***SCRUBBED***") {
                        push_conflict(
                            summary,
                            table,
                            row.get("key").cloned().unwrap_or(Value::Null),
                            "kept_existing_secret",
                            Some(
                                "created with placeholder, requires manual update".into(),
                            ),
                            dry_run,
                        );
                    }
                }
            }

            let new_id = insert_row(tx, table, &row, column_cache, dry_run).await?;
            if let Some(oid) = old_id {
                if let Some(nid) = new_id {
                    lookup_map.insert((table.to_string(), oid), nid);
                }
            }
            counts.created += 1;
        }
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn handle_existing_setting(
    tx: &mut Transaction<'_, Postgres>,
    row: &Map<String, Value>,
    _existing_id: i64,
    strategy: ConflictStrategy,
    dry_run: bool,
    counts: &mut TableCounts,
    summary: &mut ImportSummary,
    natural_key: &Value,
) -> Result<(), AppError> {
    let incoming_value = row.get("value").cloned().unwrap_or(Value::Null);
    let incoming_is_scrubbed = incoming_value.as_str() == Some("***SCRUBBED***");

    match strategy {
        ConflictStrategy::Skip => {
            counts.skipped += 1;
            push_conflict(summary, "settings", natural_key.clone(), "skipped", None, dry_run);
        }
        ConflictStrategy::Fail => {
            if dry_run {
                counts.failed += 1;
                push_conflict(
                    summary,
                    "settings",
                    natural_key.clone(),
                    "would_fail",
                    Some(
                        "existing row matches natural key — would import under Skip/Update"
                            .to_string(),
                    ),
                    dry_run,
                );
            } else {
                return Err(AppError::Conflict(format!(
                    "Import aborted: existing row in settings matches natural key {natural_key}"
                )));
            }
        }
        ConflictStrategy::Update => {
            if incoming_is_scrubbed {
                // Keep existing real value; treat as skip + log.
                counts.skipped += 1;
                push_conflict(
                    summary,
                    "settings",
                    natural_key.clone(),
                    "kept_existing_secret",
                    Some("incoming value was scrubbed; existing real value preserved".into()),
                    dry_run,
                );
            } else {
                let key = row.get("key").and_then(Value::as_str).unwrap_or("");
                let org_id = row.get("org_id").and_then(Value::as_i64).unwrap_or(0) as i32;
                sqlx::query(
                    "UPDATE settings SET value = $1 WHERE org_id = $2 AND key = $3",
                )
                .bind(&incoming_value)
                .bind(org_id)
                .bind(key)
                .execute(&mut **tx)
                .await?;
                counts.updated += 1;
                push_conflict(summary, "settings", natural_key.clone(), "updated", None, dry_run);
            }
        }
    }
    Ok(())
}

/// Conflict handler for `organisation_branding`. Mirrors the settings
/// pattern: the row has a composite PK `(org_id, variant)` and no `id`
/// column, so an Update must key on those two columns rather than
/// `WHERE t.id = $2`.
#[allow(clippy::too_many_arguments)]
async fn handle_existing_branding(
    tx: &mut Transaction<'_, Postgres>,
    row: &Map<String, Value>,
    strategy: ConflictStrategy,
    dry_run: bool,
    counts: &mut TableCounts,
    summary: &mut ImportSummary,
    natural_key: &Value,
    cache: &mut HashMap<String, Vec<String>>,
) -> Result<(), AppError> {
    match strategy {
        ConflictStrategy::Skip => {
            counts.skipped += 1;
            push_conflict(summary, "organisation_branding", natural_key.clone(), "skipped", None, dry_run);
        }
        ConflictStrategy::Fail => {
            if dry_run {
                counts.failed += 1;
                push_conflict(
                    summary,
                    "organisation_branding",
                    natural_key.clone(),
                    "would_fail",
                    Some(
                        "existing row matches natural key — would import under Skip/Update"
                            .to_string(),
                    ),
                    dry_run,
                );
            } else {
                return Err(AppError::Conflict(format!(
                    "Import aborted: existing row in organisation_branding matches natural key {natural_key}"
                )));
            }
        }
        ConflictStrategy::Update => {
            let cols = columns_for(tx, "organisation_branding", cache).await?;
            let skip: &[&str] = &["org_id", "variant"];
            let set_cols: Vec<&str> = cols
                .iter()
                .filter(|c| !skip.contains(&c.as_str()))
                .map(String::as_str)
                .collect();
            if !set_cols.is_empty() {
                let set_clause = set_cols
                    .iter()
                    .map(|c| format!("{c} = newr.{c}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                let payload = Value::Object(row.clone());
                let org_id = row.get("org_id").and_then(Value::as_i64).unwrap_or(0) as i32;
                let variant = row.get("variant").and_then(Value::as_str).unwrap_or("");
                let sql = format!(
                    "UPDATE organisation_branding t SET {set_clause} \
                     FROM (SELECT * FROM jsonb_populate_record(NULL::organisation_branding, $1)) AS newr \
                     WHERE t.org_id = $2 AND t.variant = $3"
                );
                sqlx::query(&sql)
                    .bind(&payload)
                    .bind(org_id)
                    .bind(variant)
                    .execute(&mut **tx)
                    .await?;
            }
            counts.updated += 1;
            push_conflict(summary, "organisation_branding", natural_key.clone(), "updated", None, dry_run);
        }
    }
    Ok(())
}

// =====================================================================
// Natural-key lookup
// =====================================================================

async fn find_existing(
    tx: &mut Transaction<'_, Postgres>,
    table: &str,
    row: &Map<String, Value>,
    org_id: i32,
) -> Result<Option<(i64, Value)>, AppError> {
    macro_rules! lookup {
        ($sql:expr, $($bind:expr),* ; $key:expr) => {{
            let opt = sqlx::query($sql)
                $(.bind($bind))*
                .fetch_optional(&mut **tx).await?;
            Ok(opt.map(|r| (r.get::<i32, _>("id") as i64, $key)))
        }};
    }

    match table {
        "aircraft_models" => {
            let name = row.get("name").and_then(Value::as_str).unwrap_or("");
            lookup!(
                "SELECT id FROM aircraft_models WHERE org_id=$1 AND LOWER(name)=LOWER($2)",
                org_id, name; serde_json::json!({"name": name})
            )
        }
        "aircraft" => {
            let sn = row.get("serial_number").and_then(Value::as_str).unwrap_or("");
            lookup!(
                "SELECT id FROM aircraft WHERE org_id=$1 AND serial_number=$2",
                org_id, sn; serde_json::json!({"serial_number": sn})
            )
        }
        "batteries" => {
            let sn = row.get("serial_number").and_then(Value::as_str).unwrap_or("");
            lookup!(
                "SELECT id FROM batteries WHERE org_id=$1 AND serial_number=$2",
                org_id, sn; serde_json::json!({"serial_number": sn})
            )
        }
        "remote_controllers" => {
            let sn = row.get("serial_number").and_then(Value::as_str).unwrap_or("");
            lookup!(
                "SELECT id FROM remote_controllers WHERE org_id=$1 AND serial_number=$2",
                org_id, sn; serde_json::json!({"serial_number": sn})
            )
        }
        // Present SNs are unique per org (accessories_org_serial_key, mig 050);
        // SN is nullable, so SN-less rows fall back to name + equipment_id.
        "accessories" => {
            if let Some(sn) = row.get("serial_number").and_then(Value::as_str) {
                lookup!(
                    "SELECT id FROM accessories WHERE org_id=$1 AND serial_number=$2",
                    org_id, sn; serde_json::json!({"serial_number": sn})
                )
            } else {
                let name = row.get("name").and_then(Value::as_str).unwrap_or("");
                let equipment_id = row.get("equipment_id").and_then(Value::as_str);
                let opt = sqlx::query(
                    "SELECT id FROM accessories \
                     WHERE org_id=$1 AND serial_number IS NULL AND name=$2 \
                       AND equipment_id IS NOT DISTINCT FROM $3 \
                     LIMIT 1",
                )
                .bind(org_id)
                .bind(name)
                .bind(equipment_id)
                .fetch_optional(&mut **tx)
                .await?;
                Ok(opt.map(|r| {
                    (
                        r.get::<i32, _>("id") as i64,
                        serde_json::json!({"name": name, "equipment_id": equipment_id}),
                    )
                }))
            }
        }
        "pilots" => find_pilot(tx, row, org_id).await,
        "pilot_qualifications" => {
            let pilot_id = row.get("pilot_id").and_then(Value::as_i64).unwrap_or(0) as i32;
            let type_name = row.get("type_name").and_then(Value::as_str).unwrap_or("");
            if pilot_id == 0 {
                return Ok(None);
            }
            lookup!(
                "SELECT id FROM pilot_qualifications WHERE pilot_id=$1 AND type_name=$2",
                pilot_id, type_name;
                serde_json::json!({"pilot_id": pilot_id, "type_name": type_name})
            )
        }
        "flights" => find_flight(tx, row, org_id).await,
        "maintenance_schedules" => {
            let aircraft_id = row.get("aircraft_id").and_then(Value::as_i64).unwrap_or(0) as i32;
            let name = row.get("name").and_then(Value::as_str).unwrap_or("");
            if aircraft_id == 0 {
                return Ok(None);
            }
            lookup!(
                "SELECT id FROM maintenance_schedules WHERE org_id=$1 AND aircraft_id=$2 AND name=$3",
                org_id, aircraft_id, name;
                serde_json::json!({"aircraft_id": aircraft_id, "name": name})
            )
        }
        "maintenance" => {
            let aircraft_id = row.get("aircraft_id").and_then(Value::as_i64).map(|n| n as i32);
            let performed_date = row.get("performed_date").cloned().unwrap_or(Value::Null);
            let sign_off = row.get("sign_off").and_then(Value::as_str).unwrap_or("");
            let action_taken = row.get("action_taken").cloned().unwrap_or(Value::Null);
            let opt = sqlx::query(
                "SELECT id FROM maintenance \
                 WHERE org_id=$1 AND aircraft_id IS NOT DISTINCT FROM $2 \
                   AND performed_date=$3::date AND sign_off=$4 \
                   AND COALESCE(action_taken,'')=COALESCE($5,'') \
                 LIMIT 1",
            )
            .bind(org_id)
            .bind(aircraft_id)
            .bind(performed_date.as_str())
            .bind(sign_off)
            .bind(action_taken.as_str())
            .fetch_optional(&mut **tx)
            .await?;
            Ok(opt.map(|r| {
                (
                    r.get::<i32, _>("id") as i64,
                    serde_json::json!({
                        "aircraft_id": aircraft_id, "performed_date": performed_date,
                        "sign_off": sign_off, "action_taken": action_taken,
                    }),
                )
            }))
        }
        "defect_log" => {
            let aircraft_id = row.get("aircraft_id").and_then(Value::as_i64).map(|n| n as i32);
            let defect_date = row.get("defect_date").cloned().unwrap_or(Value::Null);
            let reported_by = row.get("reported_by").and_then(Value::as_str).unwrap_or("");
            let description = row.get("description").and_then(Value::as_str).unwrap_or("");
            let opt = sqlx::query(
                "SELECT id FROM defect_log \
                 WHERE org_id=$1 AND aircraft_id IS NOT DISTINCT FROM $2 \
                   AND defect_date=$3::date AND reported_by=$4 AND description=$5 \
                 LIMIT 1",
            )
            .bind(org_id)
            .bind(aircraft_id)
            .bind(defect_date.as_str())
            .bind(reported_by)
            .bind(description)
            .fetch_optional(&mut **tx)
            .await?;
            Ok(opt.map(|r| {
                (
                    r.get::<i32, _>("id") as i64,
                    serde_json::json!({
                        "aircraft_id": aircraft_id, "defect_date": defect_date,
                        "reported_by": reported_by,
                    }),
                )
            }))
        }
        "battery_tests" => {
            let battery_id = row.get("battery_id").and_then(Value::as_i64).unwrap_or(0) as i32;
            let test_date = row.get("test_date").cloned().unwrap_or(Value::Null);
            let test_type = row.get("test_type").and_then(Value::as_str).unwrap_or("");
            let opt = sqlx::query(
                "SELECT id FROM battery_tests \
                 WHERE org_id=$1 AND battery_id=$2 AND test_date=$3::date AND test_type=$4 \
                 LIMIT 1",
            )
            .bind(org_id)
            .bind(battery_id)
            .bind(test_date.as_str())
            .bind(test_type)
            .fetch_optional(&mut **tx)
            .await?;
            Ok(opt.map(|r| {
                (
                    r.get::<i32, _>("id") as i64,
                    serde_json::json!({
                        "battery_id": battery_id, "test_date": test_date, "test_type": test_type,
                    }),
                )
            }))
        }
        "incidents" => {
            let occurred_at = row.get("occurred_at").cloned().unwrap_or(Value::Null);
            let aircraft_id = row.get("aircraft_id").and_then(Value::as_i64).map(|n| n as i32);
            let description = row.get("description").and_then(Value::as_str).unwrap_or("");
            let opt = sqlx::query(
                "SELECT id FROM incidents \
                 WHERE org_id=$1 AND occurred_at=$2::timestamptz \
                   AND aircraft_id IS NOT DISTINCT FROM $3 AND description=$4 \
                 LIMIT 1",
            )
            .bind(org_id)
            .bind(occurred_at.as_str())
            .bind(aircraft_id)
            .bind(description)
            .fetch_optional(&mut **tx)
            .await?;
            Ok(opt.map(|r| {
                (
                    r.get::<i32, _>("id") as i64,
                    serde_json::json!({
                        "occurred_at": occurred_at, "aircraft_id": aircraft_id,
                        "description": description,
                    }),
                )
            }))
        }
        "settings" => {
            let key = row.get("key").and_then(Value::as_str).unwrap_or("");
            let opt = sqlx::query("SELECT 1::INT AS id FROM settings WHERE org_id=$1 AND key=$2")
                .bind(org_id)
                .bind(key)
                .fetch_optional(&mut **tx)
                .await?;
            Ok(opt.map(|_| (0_i64, serde_json::json!({"key": key}))))
        }
        "drone_type_mappings" => {
            let app_source = row.get("app_source").and_then(Value::as_str).unwrap_or("");
            let drone_type_code = row.get("drone_type_code").and_then(Value::as_str).unwrap_or("");
            lookup!(
                "SELECT id FROM drone_type_mappings WHERE org_id=$1 AND app_source=$2 AND drone_type_code=$3",
                org_id, app_source, drone_type_code;
                serde_json::json!({"app_source": app_source, "drone_type_code": drone_type_code})
            )
        }
        "checklists" => {
            // Migration 034 replaced the legacy free-text `model` column with a
            // model_id FK. The natural key for cross-instance identity is
            // (org_id, model_id, type, revision) — title isn't part of the key
            // because UNIQUE(model_id, type) already enforces one row per
            // (model, type) combination.
            let model_id = row.get("model_id").and_then(Value::as_i64).map(|v| v as i32).unwrap_or(0);
            let typ = row.get("type").and_then(Value::as_str).unwrap_or("");
            let revision = row.get("revision").and_then(Value::as_str).unwrap_or("01");
            lookup!(
                "SELECT id FROM checklists WHERE org_id=$1 AND model_id=$2 AND type=$3 AND revision=$4",
                org_id, model_id, typ, revision;
                serde_json::json!({"model_id": model_id, "type": typ, "revision": revision})
            )
        }
        // Identifier tables share UNIQUE(org_id, identifier, source).
        "aircraft_identifiers" | "controller_identifiers" | "battery_identifiers" => {
            let identifier = row.get("identifier").and_then(Value::as_str).unwrap_or("");
            let source = row.get("source").and_then(Value::as_str).unwrap_or("");
            let sql = format!(
                "SELECT id FROM {table} WHERE org_id=$1 AND identifier=$2 AND source=$3"
            );
            let opt = sqlx::query(&sql)
                .bind(org_id)
                .bind(identifier)
                .bind(source)
                .fetch_optional(&mut **tx)
                .await?;
            Ok(opt.map(|r| {
                (
                    r.get::<i32, _>("id") as i64,
                    serde_json::json!({"identifier": identifier, "source": source}),
                )
            }))
        }
        // UNIQUE(org_id, model_id, name); model_id is already remapped.
        "maintenance_schedule_templates" => {
            let model_id = row.get("model_id").and_then(Value::as_i64).map(|n| n as i32).unwrap_or(0);
            let name = row.get("name").and_then(Value::as_str).unwrap_or("");
            if model_id == 0 {
                return Ok(None);
            }
            lookup!(
                "SELECT id FROM maintenance_schedule_templates WHERE org_id=$1 AND model_id=$2 AND name=$3",
                org_id, model_id, name;
                serde_json::json!({"model_id": model_id, "name": name})
            )
        }
        // No UNIQUE constraint; identity is org + model (nullable) + name + revision.
        "maintenance_instruction_templates" => {
            let model_id = row.get("aircraft_model_id").and_then(Value::as_i64).map(|n| n as i32);
            let name = row.get("name").and_then(Value::as_str).unwrap_or("");
            let revision = row.get("revision").and_then(Value::as_str).unwrap_or("01");
            let opt = sqlx::query(
                "SELECT id FROM maintenance_instruction_templates \
                 WHERE org_id=$1 AND aircraft_model_id IS NOT DISTINCT FROM $2 \
                   AND name=$3 AND revision=$4 LIMIT 1",
            )
            .bind(org_id)
            .bind(model_id)
            .bind(name)
            .bind(revision)
            .fetch_optional(&mut **tx)
            .await?;
            Ok(opt.map(|r| {
                (
                    r.get::<i32, _>("id") as i64,
                    serde_json::json!({
                        "aircraft_model_id": model_id, "name": name, "revision": revision,
                    }),
                )
            }))
        }
        // No org_id column; template_id (remapped) is already org-scoped.
        "maintenance_checklist_items" => {
            let template_id = row.get("template_id").and_then(Value::as_i64).map(|n| n as i32).unwrap_or(0);
            let section_name = row.get("section_name").and_then(Value::as_str).unwrap_or("");
            let item_text = row.get("item_text").and_then(Value::as_str).unwrap_or("");
            if template_id == 0 {
                return Ok(None);
            }
            let opt = sqlx::query(
                "SELECT id FROM maintenance_checklist_items \
                 WHERE template_id=$1 AND section_name=$2 AND item_text=$3 LIMIT 1",
            )
            .bind(template_id)
            .bind(section_name)
            .bind(item_text)
            .fetch_optional(&mut **tx)
            .await?;
            Ok(opt.map(|r| {
                (
                    r.get::<i32, _>("id") as i64,
                    serde_json::json!({
                        "template_id": template_id, "section_name": section_name,
                        "item_text": item_text,
                    }),
                )
            }))
        }
        // UNIQUE(maintenance_id, checklist_item_id); both remapped. No org_id.
        "maintenance_checklist_completions" => {
            let maintenance_id = row.get("maintenance_id").and_then(Value::as_i64).map(|n| n as i32).unwrap_or(0);
            let checklist_item_id = row.get("checklist_item_id").and_then(Value::as_i64).map(|n| n as i32).unwrap_or(0);
            if maintenance_id == 0 || checklist_item_id == 0 {
                return Ok(None);
            }
            lookup!(
                "SELECT id FROM maintenance_checklist_completions \
                 WHERE maintenance_id=$1 AND checklist_item_id=$2",
                maintenance_id, checklist_item_id;
                serde_json::json!({
                    "maintenance_id": maintenance_id, "checklist_item_id": checklist_item_id,
                })
            )
        }
        // UNIQUE(org_id, schedule_id); schedule_id remapped.
        "maintenance_drafts" => {
            let schedule_id = row.get("schedule_id").and_then(Value::as_i64).map(|n| n as i32).unwrap_or(0);
            if schedule_id == 0 {
                return Ok(None);
            }
            lookup!(
                "SELECT id FROM maintenance_drafts WHERE org_id=$1 AND schedule_id=$2",
                org_id, schedule_id;
                serde_json::json!({"schedule_id": schedule_id})
            )
        }
        // Append-only table; org + incident (remapped) + created_at + note
        // makes re-imports idempotent.
        "incident_notes" => {
            let incident_id = row.get("incident_id").and_then(Value::as_i64).map(|n| n as i32);
            let created_at = row.get("created_at").cloned().unwrap_or(Value::Null);
            let note = row.get("note").and_then(Value::as_str).unwrap_or("");
            let opt = sqlx::query(
                "SELECT id FROM incident_notes \
                 WHERE org_id=$1 AND incident_id IS NOT DISTINCT FROM $2 \
                   AND created_at=$3::timestamptz AND note=$4 LIMIT 1",
            )
            .bind(org_id)
            .bind(incident_id)
            .bind(created_at.as_str())
            .bind(note)
            .fetch_optional(&mut **tx)
            .await?;
            Ok(opt.map(|r| {
                (
                    r.get::<i32, _>("id") as i64,
                    serde_json::json!({
                        "incident_id": incident_id, "created_at": created_at, "note": note,
                    }),
                )
            }))
        }
        // Composite PK (org_id, variant), no id column. Return sentinel id 0;
        // the update path (handle_existing_branding) keys on (org_id, variant).
        "organisation_branding" => {
            let variant = row.get("variant").and_then(Value::as_str).unwrap_or("");
            let opt = sqlx::query(
                "SELECT 1::INT AS id FROM organisation_branding WHERE org_id=$1 AND variant=$2",
            )
            .bind(org_id)
            .bind(variant)
            .fetch_optional(&mut **tx)
            .await?;
            Ok(opt.map(|_| (0_i64, serde_json::json!({"variant": variant}))))
        }
        _ => Ok(None),
    }
}

async fn find_pilot(
    tx: &mut Transaction<'_, Postgres>,
    row: &Map<String, Value>,
    org_id: i32,
) -> Result<Option<(i64, Value)>, AppError> {
    let lic = row.get("license_number").and_then(Value::as_str).unwrap_or("").trim();
    let email = row.get("email").and_then(Value::as_str).unwrap_or("").trim();
    let name = row.get("name").and_then(Value::as_str).unwrap_or("").trim();

    if !lic.is_empty() {
        if let Some(r) = sqlx::query(
            "SELECT id FROM pilots WHERE org_id=$1 AND license_number=$2 LIMIT 1",
        )
        .bind(org_id)
        .bind(lic)
        .fetch_optional(&mut **tx)
        .await?
        {
            return Ok(Some((
                r.get::<i32, _>("id") as i64,
                serde_json::json!({"license_number": lic}),
            )));
        }
    }
    if !email.is_empty() {
        if let Some(r) = sqlx::query(
            "SELECT id FROM pilots WHERE org_id=$1 AND email=$2 LIMIT 1",
        )
        .bind(org_id)
        .bind(email)
        .fetch_optional(&mut **tx)
        .await?
        {
            return Ok(Some((
                r.get::<i32, _>("id") as i64,
                serde_json::json!({"email": email}),
            )));
        }
    }
    if !name.is_empty() {
        if let Some(r) = sqlx::query(
            "SELECT id FROM pilots WHERE org_id=$1 AND name=$2 LIMIT 1",
        )
        .bind(org_id)
        .bind(name)
        .fetch_optional(&mut **tx)
        .await?
        {
            return Ok(Some((
                r.get::<i32, _>("id") as i64,
                serde_json::json!({"name": name}),
            )));
        }
    }
    Ok(None)
}

async fn find_flight(
    tx: &mut Transaction<'_, Postgres>,
    row: &Map<String, Value>,
    org_id: i32,
) -> Result<Option<(i64, Value)>, AppError> {
    let guid = row.get("flight_guid").and_then(Value::as_str).unwrap_or("");
    if !guid.is_empty() {
        if let Some(r) = sqlx::query(
            "SELECT id FROM flights WHERE flight_guid=$1 AND org_id=$2 LIMIT 1",
        )
        .bind(guid)
        .bind(org_id)
        .fetch_optional(&mut **tx)
        .await?
        {
            return Ok(Some((
                r.get::<i32, _>("id") as i64,
                serde_json::json!({"flight_guid": guid}),
            )));
        }
    }
    let dt = row.get("flight_datetime").cloned().unwrap_or(Value::Null);
    let src = row.get("source_filename").and_then(Value::as_str).unwrap_or("");
    if dt.is_null() || src.is_empty() {
        return Ok(None);
    }
    let opt = sqlx::query(
        "SELECT id FROM flights \
         WHERE org_id=$1 AND flight_datetime=$2::timestamptz AND source_filename=$3 \
           AND deleted_at IS NULL LIMIT 1",
    )
    .bind(org_id)
    .bind(dt.as_str())
    .bind(src)
    .fetch_optional(&mut **tx)
    .await?;
    Ok(opt.map(|r| {
        (
            r.get::<i32, _>("id") as i64,
            serde_json::json!({"flight_datetime": dt, "source_filename": src}),
        )
    }))
}

// =====================================================================
// FK remap
// =====================================================================

enum FkOutcome {
    Ok,
    Failed,
}

fn remap_fks(
    table: &str,
    row: &mut Map<String, Value>,
    lookup_map: &HashMap<(String, i64), i64>,
    summary: &mut ImportSummary,
) -> FkOutcome {
    for spec in fk_specs(table) {
        let cur = row.get(spec.col).cloned().unwrap_or(Value::Null);
        // Preserve null/missing incoming values — the source had no FK
        // here, so there is nothing to remap. The schema's column-level
        // nullability decides whether the eventual INSERT succeeds; if
        // the column is NOT NULL the DB will reject it, which falls
        // through to the regular error path.
        let Some(old_id) = cur.as_i64() else { continue };
        let key = (spec.refs.to_string(), old_id);
        if let Some(new_id) = lookup_map.get(&key).copied() {
            row.insert(spec.col.to_string(), Value::from(new_id));
        } else {
            match spec.action {
                FKAction::NullOnUnresolved => {
                    row.insert(spec.col.to_string(), Value::Null);
                    push_conflict(
                        summary,
                        table,
                        Value::from(old_id),
                        "fk_unresolved",
                        Some(format!("{} ({}={}) set to NULL", spec.col, spec.refs, old_id)),
                        false,
                    );
                }
                FKAction::FailRow => {
                    push_conflict(
                        summary,
                        table,
                        Value::from(old_id),
                        "failed",
                        Some(format!(
                            "required FK {} unresolved ({}={})",
                            spec.col, spec.refs, old_id
                        )),
                        false,
                    );
                    return FkOutcome::Failed;
                }
            }
        }
    }
    FkOutcome::Ok
}

// =====================================================================
// Dynamic SQL
// =====================================================================

async fn columns_for(
    tx: &mut Transaction<'_, Postgres>,
    table: &str,
    cache: &mut HashMap<String, Vec<String>>,
) -> Result<Vec<String>, AppError> {
    if let Some(c) = cache.get(table) {
        return Ok(c.clone());
    }
    let rows = sqlx::query(
        "SELECT column_name FROM information_schema.columns \
         WHERE table_schema='public' AND table_name=$1 ORDER BY ordinal_position",
    )
    .bind(table)
    .fetch_all(&mut **tx)
    .await?;
    let cols: Vec<String> = rows
        .into_iter()
        .map(|r| r.get::<String, _>("column_name"))
        .collect();
    cache.insert(table.to_string(), cols.clone());
    Ok(cols)
}

async fn insert_row(
    tx: &mut Transaction<'_, Postgres>,
    table: &str,
    row: &Map<String, Value>,
    cache: &mut HashMap<String, Vec<String>>,
    _dry_run: bool,
) -> Result<Option<i64>, AppError> {
    let cols = columns_for(tx, table, cache).await?;
    // settings has no id; everything else strips id (let SERIAL allocate).
    let cols_filtered: Vec<&str> = cols
        .iter()
        .filter(|c| c.as_str() != "id")
        .map(String::as_str)
        .collect();
    let cols_list = cols_filtered.join(", ");
    let payload = Value::Object(row.clone());

    if cols.iter().any(|c| c == "id") {
        let sql = format!(
            "INSERT INTO {table} ({cols_list}) \
             SELECT {cols_list} FROM jsonb_populate_record(NULL::{table}, $1) \
             RETURNING id"
        );
        let r = sqlx::query(&sql).bind(&payload).fetch_one(&mut **tx).await?;
        Ok(Some(r.get::<i32, _>("id") as i64))
    } else {
        // settings: composite PK, no SERIAL id
        let sql = format!(
            "INSERT INTO {table} ({cols_list}) \
             SELECT {cols_list} FROM jsonb_populate_record(NULL::{table}, $1)"
        );
        sqlx::query(&sql).bind(&payload).execute(&mut **tx).await?;
        Ok(None)
    }
}

async fn insert_audit_row(
    tx: &mut Transaction<'_, Postgres>,
    cols: &[String],
    row: &Map<String, Value>,
    _dry_run: bool,
    counts: &mut TableCounts,
) -> Result<(), AppError> {
    // audit_log: preserve source id, ON CONFLICT(id) DO NOTHING.
    let cols_list = cols.join(", ");
    let payload = Value::Object(row.clone());
    let sql = format!(
        "INSERT INTO audit_log ({cols_list}) \
         SELECT {cols_list} FROM jsonb_populate_record(NULL::audit_log, $1) \
         ON CONFLICT (id) DO NOTHING \
         RETURNING id"
    );
    let res = sqlx::query(&sql)
        .bind(&payload)
        .fetch_optional(&mut **tx)
        .await?;
    if res.is_some() {
        counts.created += 1;
    } else {
        counts.skipped += 1;
    }
    Ok(())
}

async fn update_row(
    tx: &mut Transaction<'_, Postgres>,
    table: &str,
    row: &Map<String, Value>,
    existing_id: i64,
    cache: &mut HashMap<String, Vec<String>>,
) -> Result<(), AppError> {
    let cols = columns_for(tx, table, cache).await?;
    let skip: &[&str] = &["id", "created_at"];
    let set_cols: Vec<&str> = cols
        .iter()
        .filter(|c| !skip.contains(&c.as_str()))
        .map(String::as_str)
        .collect();
    if set_cols.is_empty() {
        return Ok(());
    }
    let set_clause = set_cols
        .iter()
        .map(|c| format!("{c} = newr.{c}"))
        .collect::<Vec<_>>()
        .join(", ");
    let payload = Value::Object(row.clone());
    let sql = format!(
        "UPDATE {table} t SET {set_clause} \
         FROM (SELECT * FROM jsonb_populate_record(NULL::{table}, $1)) AS newr \
         WHERE t.id = $2"
    );
    sqlx::query(&sql)
        .bind(&payload)
        .bind(existing_id as i32)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

// =====================================================================
// Conflict capture
// =====================================================================

fn push_conflict(
    summary: &mut ImportSummary,
    table: &str,
    natural_key: Value,
    action: &str,
    reason: Option<String>,
    dry_run: bool,
) {
    if summary.conflicts.len() >= MAX_CONFLICT_DETAILS {
        summary.conflicts_truncated = true;
        return;
    }
    let action_str = if dry_run {
        match action {
            "skipped" => "would_skip".to_string(),
            "updated" => "would_update".to_string(),
            other => other.to_string(),
        }
    } else {
        action.to_string()
    };
    summary.conflicts.push(ConflictDetail {
        table: table.to_string(),
        natural_key,
        action: action_str,
        reason,
    });
}

// =====================================================================
// Tests
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_rejects_wrong_format_version() {
        let doc = serde_json::json!({"format_version": 2, "tables": {}});
        let r = validate_document(&doc);
        assert!(matches!(r, Err(AppError::BadRequest(_))));
    }

    #[test]
    fn validate_rejects_users_table() {
        let doc = serde_json::json!({"format_version": 1, "tables": {"users": []}});
        let r = validate_document(&doc);
        assert!(matches!(r, Err(AppError::BadRequest(_))));
    }

    #[test]
    fn validate_rejects_unknown_table() {
        let doc = serde_json::json!({"format_version": 1, "tables": {"foo_bar": []}});
        let r = validate_document(&doc);
        assert!(matches!(r, Err(AppError::BadRequest(_))));
    }

    #[test]
    fn validate_accepts_minimal_doc() {
        let doc = serde_json::json!({"format_version": 1, "tables": {}});
        assert!(validate_document(&doc).is_ok());
    }

    #[test]
    fn validate_accepts_silently_ignored_table() {
        let doc = serde_json::json!({"format_version": 1, "tables": {"data_transfer_jobs": []}});
        assert!(validate_document(&doc).is_ok());
    }

    #[test]
    fn validate_rejects_non_object_row() {
        let doc = serde_json::json!({"format_version": 1, "tables": {"aircraft": ["not an object"]}});
        let r = validate_document(&doc);
        assert!(matches!(r, Err(AppError::BadRequest(_))));
    }

    #[test]
    fn conflict_detail_dry_run_prefixes_action() {
        let mut s = ImportSummary {
            per_table: Map::new(),
            total_created: 0,
            total_updated: 0,
            total_skipped: 0,
            total_failed: 0,
            conflicts: Vec::new(),
            conflicts_truncated: false,
            format_version: 1,
            source_org_id: None,
            dry_run: true,
        };
        push_conflict(&mut s, "aircraft", Value::Null, "skipped", None, true);
        assert_eq!(s.conflicts[0].action, "would_skip");
    }

    #[test]
    fn fixup_replaces_instruction_template_created_by_with_actor() {
        let mut row = serde_json::json!({
            "name": "M600 100h", "created_by": 999, "revision": "01",
        })
        .as_object()
        .unwrap()
        .clone();
        fixup_user_columns("maintenance_instruction_templates", &mut row, 7);
        assert_eq!(row.get("created_by"), Some(&Value::from(7)));
    }

    #[test]
    fn fixup_nulls_identifier_created_by() {
        for table in [
            "aircraft_identifiers",
            "controller_identifiers",
            "battery_identifiers",
            "incident_notes",
        ] {
            let mut row = serde_json::json!({ "created_by": 42, "identifier": "X" })
                .as_object()
                .unwrap()
                .clone();
            fixup_user_columns(table, &mut row, 7);
            assert_eq!(row.get("created_by"), Some(&Value::Null), "table {table}");
        }
    }

    #[test]
    fn fixup_leaves_other_tables_untouched() {
        // maintenance_schedule_templates.created_by is free-text (a user_code),
        // not a user id — must not be rewritten.
        let mut row = serde_json::json!({ "created_by": "LAR", "name": "x" })
            .as_object()
            .unwrap()
            .clone();
        fixup_user_columns("maintenance_schedule_templates", &mut row, 7);
        assert_eq!(row.get("created_by"), Some(&Value::from("LAR")));
    }

    #[test]
    fn conflict_truncation_kicks_in_at_100() {
        let mut s = ImportSummary {
            per_table: Map::new(),
            total_created: 0,
            total_updated: 0,
            total_skipped: 0,
            total_failed: 0,
            conflicts: Vec::new(),
            conflicts_truncated: false,
            format_version: 1,
            source_org_id: None,
            dry_run: false,
        };
        for _ in 0..150 {
            push_conflict(&mut s, "aircraft", Value::Null, "skipped", None, false);
        }
        assert_eq!(s.conflicts.len(), 100);
        assert!(s.conflicts_truncated);
    }
}
