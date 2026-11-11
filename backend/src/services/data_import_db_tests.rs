// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

//! DB-backed round-trip test for the full-database export/import coverage.
//!
//! Seeds a source org with at least one row in each of the tables added in
//! this change (identifier tables, instruction templates + items, checklist
//! completions, drafts, organisation branding with a BYTEA logo, incident
//! notes), exports it, imports into a fresh target org, and asserts:
//!
//! - the new tables produce rows on import,
//! - `maintenance_schedules.instruction_template_id` on the imported schedule
//!   points at the imported template (FK remap works across the document),
//! - `maintenance_instruction_templates.created_by` is rewritten to the
//!   importing actor and the identifier `created_by` is nulled,
//! - the BYTEA logo round-trips byte-identically,
//! - a second import of the same document skips everything (idempotency).
//!
//! Runs against a fresh migrated DB via `#[sqlx::test(migrations = ...)]`.

use super::{apply_import, ConflictStrategy};
use crate::services::data_export::{self, EntityGroup};
use serde_json::Value;
use sqlx::{PgPool, Row};

const LOGO_BYTES: &[u8] = &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0xFF, 0x10, 0x42];

/// Create a fresh org + one admin user, returning (org_id, user_id, user_code).
async fn seed_org_with_admin(pool: &PgPool, label: &str) -> (i32, i32, String) {
    let org_row = sqlx::query("INSERT INTO organisations (name, slug) VALUES ($1, $2) RETURNING id")
        .bind(format!("Transfer Test {label}"))
        .bind(format!("xfer-{label}-{}", uuid::Uuid::new_v4()))
        .fetch_one(pool)
        .await
        .expect("org insert");
    let org_id: i32 = org_row.get("id");

    let user_row = sqlx::query(
        "INSERT INTO users (org_id, name, email, user_code, password_hash, role) \
         VALUES ($1, $2, $3, $4, $5, 'admin') RETURNING id, user_code",
    )
    .bind(org_id)
    .bind("Admin")
    .bind(format!("admin-{org_id}@example.com"))
    .bind(format!("A{:02}", org_id % 100))
    .bind("$2b$12$placeholder.hash.value.not.used.in.these.tests.xxxxxxxxxx")
    .fetch_one(pool)
    .await
    .expect("user insert");
    (
        org_id,
        user_row.get::<i32, _>("id"),
        user_row.get::<String, _>("user_code"),
    )
}

/// Read the `created` count for a table from an import summary's per_table map.
fn created(summary: &super::ImportSummary, table: &str) -> i64 {
    summary
        .per_table
        .get(table)
        .and_then(|v| v.get("created"))
        .and_then(Value::as_i64)
        .unwrap_or(0)
}

#[sqlx::test(migrations = "./migrations")]
async fn full_database_round_trip(pool: PgPool) {
    let (src_org, src_user, _src_code) = seed_org_with_admin(&pool, "src").await;
    let (dst_org, dst_user, dst_code) = seed_org_with_admin(&pool, "dst").await;

    // --- Equipment ---
    let model_id: i32 =
        sqlx::query("INSERT INTO aircraft_models (org_id, name) VALUES ($1, 'Inspire 1') RETURNING id")
            .bind(src_org)
            .fetch_one(&pool)
            .await
            .unwrap()
            .get("id");
    let aircraft_id: i32 = sqlx::query(
        "INSERT INTO aircraft (org_id, serial_number, model_id) VALUES ($1, 'BODY-SN-1', $2) RETURNING id",
    )
    .bind(src_org)
    .bind(model_id)
    .fetch_one(&pool)
    .await
    .unwrap()
    .get("id");
    // compatible_model_id (migration 052) points at the seeded model to
    // exercise the batteries.compatible_model_id FK remap on import.
    let battery_id: i32 = sqlx::query(
        "INSERT INTO batteries (org_id, serial_number, aircraft_id, compatible_model_id) VALUES ($1, 'BAT-SN-1', $2, $3) RETURNING id",
    )
    .bind(src_org)
    .bind(aircraft_id)
    .bind(model_id)
    .fetch_one(&pool)
    .await
    .unwrap()
    .get("id");
    let controller_id: i32 = sqlx::query(
        "INSERT INTO remote_controllers (org_id, serial_number) VALUES ($1, 'CTL-SN-1') RETURNING id",
    )
    .bind(src_org)
    .fetch_one(&pool)
    .await
    .unwrap()
    .get("id");

    // Accessory with an aircraft mount link (migration 051) — exercises the
    // accessories.aircraft_id FK remap on import.
    sqlx::query(
        "INSERT INTO accessories (org_id, name, category, weight_kg, aircraft_id) \
         VALUES ($1, 'Zenmuse H20T', 'camera', 0.83, $2)",
    )
    .bind(src_org)
    .bind(aircraft_id)
    .execute(&pool)
    .await
    .unwrap();

    // Identifier rows (fc_sn, created_by = source user).
    sqlx::query(
        "INSERT INTO aircraft_identifiers (aircraft_id, org_id, identifier, source, created_by) \
         VALUES ($1, $2, 'FC-AIR-1', 'fc_sn', $3)",
    )
    .bind(aircraft_id)
    .bind(src_org)
    .bind(src_user)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO battery_identifiers (battery_id, org_id, identifier, source, created_by) \
         VALUES ($1, $2, 'FC-BAT-1', 'fc_sn', $3)",
    )
    .bind(battery_id)
    .bind(src_org)
    .bind(src_user)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO controller_identifiers (controller_id, org_id, identifier, source, created_by) \
         VALUES ($1, $2, 'FC-CTL-1', 'fc_sn', $3)",
    )
    .bind(controller_id)
    .bind(src_org)
    .bind(src_user)
    .execute(&pool)
    .await
    .unwrap();

    // --- Flight (parent for maintenance.flight_id and defect_log.flight_id) ---
    // flight_guid is left NULL: the unique index on it is global (not
    // org-scoped), and the source and target orgs share one DB in this test.
    // Real cross-instance transfers use separate databases. Dedup falls back
    // to (flight_datetime, source_filename), which is enough here.
    let flight_id: i32 = sqlx::query(
        "INSERT INTO flights \
            (org_id, aircraft_id, controller_id, flight_datetime, source_filename) \
         VALUES ($1, $2, $3, NOW(), 'roundtrip.txt') RETURNING id",
    )
    .bind(src_org)
    .bind(aircraft_id)
    .bind(controller_id)
    .fetch_one(&pool)
    .await
    .unwrap()
    .get("id");

    // --- Maintenance templates + items ---
    let template_id: i32 = sqlx::query(
        "INSERT INTO maintenance_instruction_templates \
            (org_id, aircraft_model_id, name, revision, source, created_by) \
         VALUES ($1, $2, 'Inspection A', '01', 'custom', $3) RETURNING id",
    )
    .bind(src_org)
    .bind(model_id)
    .bind(src_user)
    .fetch_one(&pool)
    .await
    .unwrap()
    .get("id");
    let item_id: i32 = sqlx::query(
        "INSERT INTO maintenance_checklist_items (template_id, section_name, item_text) \
         VALUES ($1, 'Airframe', 'Check propellers') RETURNING id",
    )
    .bind(template_id)
    .fetch_one(&pool)
    .await
    .unwrap()
    .get("id");

    // Schedule linked to the instruction template.
    let schedule_id: i32 = sqlx::query(
        "INSERT INTO maintenance_schedules \
            (org_id, aircraft_id, name, interval_type, interval_value, instruction_template_id) \
         VALUES ($1, $2, 'Sched-1', 'calendar_days', 30, $3) RETURNING id",
    )
    .bind(src_org)
    .bind(aircraft_id)
    .bind(template_id)
    .fetch_one(&pool)
    .await
    .unwrap()
    .get("id");

    // Aircraft-target maintenance record (carries flight_id to exercise the
    // migration-027 remap) + completion. The CHECK constraint
    // maintenance_target_exactly_one forbids aircraft_id + controller_id
    // together, so controller_id is exercised by a separate row below.
    let maintenance_id: i32 = sqlx::query(
        "INSERT INTO maintenance \
            (org_id, aircraft_id, performed_date, created_by, remark, action_taken, \
             maint_category, maint_type, sign_off, instruction_template_id, flight_id) \
         VALUES ($1, $2, CURRENT_DATE, 'ADM', 'insp', 'checked', 'Scheduled', 'Test', 'ADM', $3, $4) \
         RETURNING id",
    )
    .bind(src_org)
    .bind(aircraft_id)
    .bind(template_id)
    .bind(flight_id)
    .fetch_one(&pool)
    .await
    .unwrap()
    .get("id");
    sqlx::query(
        "INSERT INTO maintenance_checklist_completions \
            (maintenance_id, checklist_item_id, status, completed_by) \
         VALUES ($1, $2, 'ok', 'ADM')",
    )
    .bind(maintenance_id)
    .bind(item_id)
    .execute(&pool)
    .await
    .unwrap();

    // Controller-target maintenance record (aircraft_id NULL) to exercise the
    // migration-031 controller_id remap independently of the constraint.
    sqlx::query(
        "INSERT INTO maintenance \
            (org_id, controller_id, performed_date, created_by, remark, action_taken, \
             maint_category, maint_type, sign_off) \
         VALUES ($1, $2, CURRENT_DATE, 'ADM', 'ctl service', 'ctl check', 'Scheduled', 'Test', 'ADM')",
    )
    .bind(src_org)
    .bind(controller_id)
    .execute(&pool)
    .await
    .unwrap();

    // Defect referencing the flight (exercises defect_log.flight_id remap).
    sqlx::query(
        "INSERT INTO defect_log \
            (org_id, aircraft_id, flight_id, defect_date, reported_by, description) \
         VALUES ($1, $2, $3, CURRENT_DATE, 'ADM', 'Cracked propeller')",
    )
    .bind(src_org)
    .bind(aircraft_id)
    .bind(flight_id)
    .execute(&pool)
    .await
    .unwrap();

    // Draft.
    sqlx::query(
        "INSERT INTO maintenance_drafts (org_id, schedule_id, saved_by) VALUES ($1, $2, 'ADM')",
    )
    .bind(src_org)
    .bind(schedule_id)
    .execute(&pool)
    .await
    .unwrap();

    // --- Incident + note ---
    let incident_id: i32 = sqlx::query(
        "INSERT INTO incidents (org_id, occurred_at, severity, category, description) \
         VALUES ($1, NOW(), 'low', 'technical', 'Test incident') RETURNING id",
    )
    .bind(src_org)
    .fetch_one(&pool)
    .await
    .unwrap()
    .get("id");
    sqlx::query(
        "INSERT INTO incident_notes (org_id, incident_id, note, created_by) \
         VALUES ($1, $2, 'A follow-up note', $3)",
    )
    .bind(src_org)
    .bind(incident_id)
    .bind(src_user)
    .execute(&pool)
    .await
    .unwrap();

    // --- Branding (BYTEA logo) ---
    sqlx::query(
        "INSERT INTO organisation_branding (org_id, variant, logo, mime_type, size_bytes) \
         VALUES ($1, 'light', $2, 'image/png', $3)",
    )
    .bind(src_org)
    .bind(LOGO_BYTES)
    .bind(LOGO_BYTES.len() as i32)
    .execute(&pool)
    .await
    .unwrap();

    // --- Export the source org (all groups) ---
    let (doc, _counts) = data_export::build_export(&pool, src_org, data_export::EntityGroup::all())
        .await
        .expect("export");

    // --- Import into the target org (Skip) ---
    let summary = apply_import(
        &pool,
        dst_org,
        dst_user,
        &dst_code,
        doc.clone(),
        ConflictStrategy::Skip,
        &[],
    )
    .await
    .expect("first import");

    // New tables each produced exactly one row.
    for table in [
        "aircraft_identifiers",
        "battery_identifiers",
        "controller_identifiers",
        "accessories",
        "maintenance_instruction_templates",
        "maintenance_checklist_items",
        "maintenance_checklist_completions",
        "maintenance_drafts",
        "incident_notes",
        "organisation_branding",
    ] {
        assert_eq!(created(&summary, table), 1, "expected 1 created for {table}");
    }

    // Instruction-template FK remap: the imported schedule points at the
    // imported template, not the source id.
    let imported_template_id: i32 = sqlx::query(
        "SELECT id FROM maintenance_instruction_templates WHERE org_id = $1",
    )
    .bind(dst_org)
    .fetch_one(&pool)
    .await
    .unwrap()
    .get("id");
    let schedule_template_ref: Option<i32> = sqlx::query(
        "SELECT instruction_template_id FROM maintenance_schedules WHERE org_id = $1",
    )
    .bind(dst_org)
    .fetch_one(&pool)
    .await
    .unwrap()
    .get("instruction_template_id");
    assert_eq!(
        schedule_template_ref,
        Some(imported_template_id),
        "schedule.instruction_template_id should remap to the imported template"
    );

    // maintenance.controller_id / .flight_id remap (migrations 031 / 027):
    // both must point at the NEWLY imported parents, not the source ids.
    let imported_controller_id: i32 =
        sqlx::query("SELECT id FROM remote_controllers WHERE org_id = $1")
            .bind(dst_org)
            .fetch_one(&pool)
            .await
            .unwrap()
            .get("id");
    let imported_flight_id: i32 = sqlx::query("SELECT id FROM flights WHERE org_id = $1")
        .bind(dst_org)
        .fetch_one(&pool)
        .await
        .unwrap()
        .get("id");
    // Two maintenance rows were imported: the aircraft-target row carries
    // flight_id, the controller-target row carries controller_id.
    let maint_flight_ref: Option<i32> = sqlx::query(
        "SELECT flight_id FROM maintenance WHERE org_id = $1 AND flight_id IS NOT NULL",
    )
    .bind(dst_org)
    .fetch_one(&pool)
    .await
    .unwrap()
    .get("flight_id");
    assert_eq!(
        maint_flight_ref,
        Some(imported_flight_id),
        "maintenance.flight_id should remap to the imported flight"
    );
    let maint_controller_ref: Option<i32> = sqlx::query(
        "SELECT controller_id FROM maintenance WHERE org_id = $1 AND controller_id IS NOT NULL",
    )
    .bind(dst_org)
    .fetch_one(&pool)
    .await
    .unwrap()
    .get("controller_id");
    assert_eq!(
        maint_controller_ref,
        Some(imported_controller_id),
        "maintenance.controller_id should remap to the imported controller"
    );

    // accessories.aircraft_id remap (migration 051).
    let imported_aircraft_id: i32 = sqlx::query("SELECT id FROM aircraft WHERE org_id = $1")
        .bind(dst_org)
        .fetch_one(&pool)
        .await
        .unwrap()
        .get("id");
    let imported_accessory_aircraft: Option<i32> =
        sqlx::query("SELECT aircraft_id FROM accessories WHERE org_id = $1")
            .bind(dst_org)
            .fetch_one(&pool)
            .await
            .unwrap()
            .get("aircraft_id");
    assert_eq!(
        imported_accessory_aircraft,
        Some(imported_aircraft_id),
        "accessories.aircraft_id should remap to the imported aircraft"
    );

    // batteries.compatible_model_id remap (migration 052): the imported
    // battery must point at the NEWLY imported aircraft model, not the
    // source id.
    let imported_model_id: i32 = sqlx::query("SELECT id FROM aircraft_models WHERE org_id = $1")
        .bind(dst_org)
        .fetch_one(&pool)
        .await
        .unwrap()
        .get("id");
    let imported_battery_model: Option<i32> =
        sqlx::query("SELECT compatible_model_id FROM batteries WHERE org_id = $1")
            .bind(dst_org)
            .fetch_one(&pool)
            .await
            .unwrap()
            .get("compatible_model_id");
    assert_eq!(
        imported_battery_model,
        Some(imported_model_id),
        "batteries.compatible_model_id should remap to the imported model"
    );

    // defect_log.flight_id remap (migration 027).
    let defect_flight_ref: Option<i32> =
        sqlx::query("SELECT flight_id FROM defect_log WHERE org_id = $1")
            .bind(dst_org)
            .fetch_one(&pool)
            .await
            .unwrap()
            .get("flight_id");
    assert_eq!(
        defect_flight_ref,
        Some(imported_flight_id),
        "defect_log.flight_id should remap to the imported flight"
    );

    // created_by fixup: instruction template owned by the importing actor.
    let template_owner: i32 = sqlx::query(
        "SELECT created_by FROM maintenance_instruction_templates WHERE org_id = $1",
    )
    .bind(dst_org)
    .fetch_one(&pool)
    .await
    .unwrap()
    .get("created_by");
    assert_eq!(template_owner, dst_user, "template created_by = importing actor");

    // created_by fixup: identifier created_by nulled.
    let ident_owner: Option<i32> = sqlx::query(
        "SELECT created_by FROM aircraft_identifiers WHERE org_id = $1",
    )
    .bind(dst_org)
    .fetch_one(&pool)
    .await
    .unwrap()
    .get("created_by");
    assert_eq!(ident_owner, None, "identifier created_by should be NULL after import");

    // BYTEA logo round-trips byte-identically.
    let logo: Vec<u8> = sqlx::query(
        "SELECT logo FROM organisation_branding WHERE org_id = $1 AND variant = 'light'",
    )
    .bind(dst_org)
    .fetch_one(&pool)
    .await
    .unwrap()
    .get("logo");
    assert_eq!(logo, LOGO_BYTES, "branding logo BYTEA should round-trip");

    // --- Second import is idempotent (everything skips) ---
    let summary2 = apply_import(
        &pool,
        dst_org,
        dst_user,
        &dst_code,
        doc,
        ConflictStrategy::Skip,
        &[],
    )
    .await
    .expect("second import");
    assert_eq!(
        summary2.total_created, 0,
        "second import of the same document should create nothing"
    );
}

/// A document carrying an organisation_branding row is fully skipped when the
/// caller excludes the Organisation group, and imports normally otherwise.
#[sqlx::test(migrations = "./migrations")]
async fn import_skips_excluded_organisation_group(pool: PgPool) {
    let (src_org, _src_user, _src_code) = seed_org_with_admin(&pool, "skip-src").await;
    let (dst_org, dst_user, dst_code) = seed_org_with_admin(&pool, "skip-dst").await;

    sqlx::query(
        "INSERT INTO organisation_branding (org_id, variant, logo, mime_type, size_bytes) \
         VALUES ($1, 'light', $2, 'image/png', $3)",
    )
    .bind(src_org)
    .bind(LOGO_BYTES)
    .bind(LOGO_BYTES.len() as i32)
    .execute(&pool)
    .await
    .unwrap();

    let (doc, _counts) = data_export::build_export(&pool, src_org, EntityGroup::all())
        .await
        .expect("export");

    // Import with the Organisation group excluded → branding is skipped.
    let skipped = apply_import(
        &pool,
        dst_org,
        dst_user,
        &dst_code,
        doc.clone(),
        ConflictStrategy::Skip,
        &[EntityGroup::Organisation],
    )
    .await
    .expect("import with skip");

    assert_eq!(created(&skipped, "organisation_branding"), 0);
    let branding = skipped
        .per_table
        .get("organisation_branding")
        .expect("per_table entry present");
    assert_eq!(branding.get("skipped").and_then(Value::as_i64), Some(1));
    assert!(skipped.total_skipped >= 1, "skipped rows counted into total");
    let present: i64 =
        sqlx::query("SELECT COUNT(*) AS n FROM organisation_branding WHERE org_id = $1")
            .bind(dst_org)
            .fetch_one(&pool)
            .await
            .unwrap()
            .get("n");
    assert_eq!(present, 0, "branding must not be written when excluded");

    // Import again with no exclusions → branding is created.
    let imported = apply_import(
        &pool,
        dst_org,
        dst_user,
        &dst_code,
        doc,
        ConflictStrategy::Skip,
        &[],
    )
    .await
    .expect("import without skip");
    assert_eq!(created(&imported, "organisation_branding"), 1);
    let present2: i64 =
        sqlx::query("SELECT COUNT(*) AS n FROM organisation_branding WHERE org_id = $1")
            .bind(dst_org)
            .fetch_one(&pool)
            .await
            .unwrap()
            .get("n");
    assert_eq!(present2, 1);
}

// =====================================================================
// Drift-proofing: fk_specs must cover every FK the live schema declares
// =====================================================================

/// User-referencing FK columns intentionally NOT remapped via `fk_specs`.
/// User rows are never exported, so their ids are meaningless across
/// instances; each of these is rewritten by `fixup_user_columns` (to the
/// importing actor for NOT NULL owners, or NULL for nullable audit columns).
///
/// Deliberately excluded from this list:
/// - `org_id -> organisations` on every table — overwritten to the target org
///   in `process_row`, handled by a generic rule in the test below.
/// - `audit_log.user_id -> users` — covered by an FKSpec (nulled when
///   unresolved), so it is satisfied by the fk_specs branch, not the allowlist.
const USER_FK_ALLOWLIST: &[(&str, &str)] = &[
    ("maintenance_instruction_templates", "created_by"), // NOT NULL -> importing actor
    ("aircraft_identifiers", "created_by"),              // nullable -> NULL
    ("battery_identifiers", "created_by"),               // nullable -> NULL
    ("controller_identifiers", "created_by"),            // nullable -> NULL
    ("incident_notes", "created_by"),                    // nullable -> NULL
    ("incidents", "created_by_user_id"),                 // nullable -> NULL
    ("checklists", "created_by"),                        // NOT NULL -> importing actor
    ("checklists", "updated_by"),                        // nullable -> NULL
];

/// Enumerates every foreign key on every table in `CANONICAL_TABLES` from the
/// live (migrated) schema and asserts each FK column is EITHER covered by an
/// `FKSpec` in `fk_specs(table)` with a matching referenced table, OR is an
/// `org_id -> organisations` column, OR is present in `USER_FK_ALLOWLIST`.
///
/// This is the guard that stops `fk_specs` silently falling behind a future
/// migration: adding a new FK without a matching spec fails this test with a
/// message naming the table, column, and referenced table.
#[sqlx::test(migrations = "./migrations")]
async fn fk_specs_cover_all_schema_fks(pool: PgPool) {
    use crate::services::data_transfer_schema::CANONICAL_TABLES;

    let tables: Vec<String> = CANONICAL_TABLES.iter().map(|(t, _)| t.to_string()).collect();

    let rows = sqlx::query(
        "SELECT conrelid::regclass::text AS table_name, \
                a.attname AS column_name, \
                confrelid::regclass::text AS referenced_table \
         FROM pg_constraint c \
         JOIN unnest(c.conkey) WITH ORDINALITY AS k(attnum, ord) ON true \
         JOIN pg_attribute a ON a.attrelid = c.conrelid AND a.attnum = k.attnum \
         WHERE c.contype = 'f' AND conrelid::regclass::text = ANY($1) \
         ORDER BY 1, 2",
    )
    .bind(&tables)
    .fetch_all(&pool)
    .await
    .expect("pg_constraint query");

    // regclass renders unqualified for public-schema relations, but strip any
    // schema prefix defensively.
    let strip = |s: String| s.rsplit('.').next().unwrap_or(&s).to_string();

    let mut gaps: Vec<String> = Vec::new();
    for row in &rows {
        let table = strip(row.get::<String, _>("table_name"));
        let col = row.get::<String, _>("column_name");
        let refs = strip(row.get::<String, _>("referenced_table"));

        // org_id is overwritten to the target org in process_row.
        if col == "org_id" && refs == "organisations" {
            continue;
        }
        // Covered by an FKSpec whose referenced table matches.
        if super::fk_specs(table.as_str())
            .iter()
            .any(|s| s.col == col && s.refs == refs)
        {
            continue;
        }
        // Allowlisted user column, handled by fixup_user_columns.
        if refs == "users" && USER_FK_ALLOWLIST.contains(&(table.as_str(), col.as_str())) {
            continue;
        }
        gaps.push(format!("{table}.{col} -> {refs}"));
    }

    assert!(
        gaps.is_empty(),
        "Import FK remap is missing coverage for the following foreign key column(s). \
         A migration added an FK that fk_specs does not remap — add an FKSpec in \
         data_import::fk_specs (nullable -> NullOnUnresolved, NOT NULL -> FailRow), or, \
         for a users reference, handle it in fixup_user_columns and add it to \
         USER_FK_ALLOWLIST:\n  {}",
        gaps.join("\n  ")
    );
}
