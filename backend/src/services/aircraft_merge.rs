// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

//! Merge a pending-review aircraft into an existing canonical aircraft.
//!
//! Background: when DJI log import auto-registers an aircraft whose FC SN
//! cannot be reconciled with any existing body SN (PR A), the new row is
//! flagged `needs_manual_sn = TRUE`. The operator then identifies which
//! existing canonical aircraft the log actually belongs to (same physical
//! drone, different SN representation) and merges the pending row into it.
//! All flights, batteries, controllers, defects, incidents, maintenance
//! records, and identifiers move to the target; per-aircraft maintenance
//! schedules on the source are deleted as duplicates (the target already
//! has its own); the source row is then deleted.
//!
//! This module is the programmatic equivalent of the legacy merge SQL scripts
//! and inherits its audit envelope: a single `audit_log` row of
//! `action='DELETE'`, `table_name='aircraft'`, `record_id=source_id`,
//! and `diff_jsonb.reason='manual-merge'`. The historical dedupe entries
//! use `reason='dedupe-merge'`; both shapes are searchable side-by-side.

use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use sqlx::{PgPool, Row};

use crate::errors::AppError;

/// Per-table counts returned to the caller. All values are post-merge
/// totals (i.e. the number of rows actually re-pointed or removed).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeOutcome {
    pub deleted_aircraft_id: i32,
    pub merged_into_aircraft_id: i32,
    pub flights_moved: i64,
    pub batteries_moved: i64,
    pub controllers_moved: i64,
    pub defects_moved: i64,
    pub incidents_moved: i64,
    pub maintenance_moved: i64,
    /// `maintenance_schedules` rows attached to the source aircraft, deleted
    /// rather than moved — both the source and target already carry their
    /// own per-aircraft seeded schedules and the target's are kept.
    pub schedules_deleted_as_duplicates: i64,
    /// `aircraft_identifiers` rows actually re-pointed to the target.
    /// Excludes any source-side rows discarded because they would collide
    /// with an existing target-side `(org, identifier, source)` triple
    /// (UNIQUE constraint from migration 025).
    pub identifiers_migrated: i64,
}

/// Move all child rows from `source_id` to `target_id`, audit the merge,
/// then delete the source aircraft. All work runs in a single transaction
/// with `FOR UPDATE` locks on both aircraft rows so concurrent merges
/// cannot interleave.
///
/// Validation (returns `Conflict` / `BadRequest` / `Forbidden` before any
/// mutation):
///   - `source_id == target_id` → `BadRequest`
///   - either aircraft missing → `NotFound`
///   - either aircraft outside the caller's org → `Forbidden`
///   - source not flagged `needs_manual_sn` → `Conflict`
///   - target already flagged `needs_manual_sn` → `Conflict`
///   - source / target `model_id` differ or are NULL → `Conflict`
///   - target `status != 'active'` → `Conflict`
///
/// The aircraft DELETE is the last write inside the transaction, *after*
/// the audit row is inserted, so the audit entry is committed atomically
/// with the deletion. This matches the pattern used by the manual
/// dedupe scripts that preceded this function (2026-05-16).
pub async fn merge_aircraft(
    pool: &PgPool,
    source_id: i32,
    target_id: i32,
    org_id: i32,
    user_id: i32,
    user_code: &str,
) -> Result<MergeOutcome, AppError> {
    if source_id == target_id {
        return Err(AppError::BadRequest(
            "source_id and target_id must be different".into(),
        ));
    }

    let mut tx = pool.begin().await?;

    // Lock both rows. Order by id so two concurrent merges that share an
    // aircraft can't deadlock on opposite lock orders.
    let (lock_first, lock_second) = if source_id < target_id {
        (source_id, target_id)
    } else {
        (target_id, source_id)
    };
    sqlx::query("SELECT id FROM aircraft WHERE id = $1 FOR UPDATE")
        .bind(lock_first)
        .fetch_optional(&mut *tx)
        .await?;
    sqlx::query("SELECT id FROM aircraft WHERE id = $1 FOR UPDATE")
        .bind(lock_second)
        .fetch_optional(&mut *tx)
        .await?;

    let source = sqlx::query(
        "SELECT id, org_id, status, model_id, needs_manual_sn, equipment_id, serial_number, \
                to_jsonb(aircraft) AS snapshot \
         FROM aircraft WHERE id = $1",
    )
    .bind(source_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Source aircraft {} not found", source_id)))?;

    let target = sqlx::query(
        "SELECT id, org_id, status, model_id, needs_manual_sn, equipment_id \
         FROM aircraft WHERE id = $1",
    )
    .bind(target_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Target aircraft {} not found", target_id)))?;

    let source_org: i32 = source.get("org_id");
    let target_org: i32 = target.get("org_id");
    if source_org != org_id || target_org != org_id {
        return Err(AppError::Forbidden(
            "Source and target aircraft must belong to your organisation".into(),
        ));
    }

    let source_needs_manual_sn: bool = source.get("needs_manual_sn");
    if !source_needs_manual_sn {
        return Err(AppError::Conflict(
            "Only pending-review aircraft can be merged as the source".into(),
        ));
    }

    let target_needs_manual_sn: bool = target.get("needs_manual_sn");
    if target_needs_manual_sn {
        return Err(AppError::Conflict(
            "Cannot merge into another pending-review aircraft".into(),
        ));
    }

    let source_model_id: Option<i32> = source.get("model_id");
    let target_model_id: Option<i32> = target.get("model_id");
    match (source_model_id, target_model_id) {
        (Some(s), Some(t)) if s == t => {}
        _ => {
            return Err(AppError::Conflict(
                "Aircraft model mismatch — both source and target must reference the same model"
                    .into(),
            ));
        }
    }

    let target_status: String = target.get("status");
    if target_status != "active" {
        return Err(AppError::Conflict("Target aircraft must be active".into()));
    }

    let source_snapshot: JsonValue = source.get("snapshot");
    let target_equipment_id: Option<String> =
        target.try_get("equipment_id").ok().flatten();

    // Capture schedules snapshot, then delete them as duplicates.
    // (The target already has its own per-aircraft schedules, seeded by
    // services::maintenance_defaults when each aircraft was created.)
    let schedules_row = sqlx::query(
        "WITH deleted AS ( \
           DELETE FROM maintenance_schedules \
           WHERE aircraft_id = $1 AND org_id = $2 \
           RETURNING * \
         ) \
         SELECT COUNT(*)::bigint AS n, \
                COALESCE(jsonb_agg(to_jsonb(deleted)), '[]'::jsonb) AS snapshot \
         FROM deleted",
    )
    .bind(source_id)
    .bind(org_id)
    .fetch_one(&mut *tx)
    .await?;
    let schedules_deleted: i64 = schedules_row.get("n");
    let schedules_snapshot: JsonValue = schedules_row.get("snapshot");

    let flights_moved = move_children(&mut tx, "flights", source_id, target_id).await?;
    let batteries_moved = move_children(&mut tx, "batteries", source_id, target_id).await?;
    let controllers_moved =
        move_children(&mut tx, "remote_controllers", source_id, target_id).await?;
    let defects_moved = move_children(&mut tx, "defect_log", source_id, target_id).await?;
    let incidents_moved = move_children(&mut tx, "incidents", source_id, target_id).await?;
    let maintenance_moved =
        move_children(&mut tx, "maintenance", source_id, target_id).await?;

    // aircraft_identifiers: delete source-side rows that would collide with
    // an existing target-side (identifier, source) pair (UNIQUE constraint
    // from migration 025), then re-point the rest.
    let _collisions_discarded: i64 = sqlx::query(
        "WITH del AS ( \
           DELETE FROM aircraft_identifiers src \
           WHERE src.aircraft_id = $1 AND src.org_id = $3 \
             AND EXISTS ( \
               SELECT 1 FROM aircraft_identifiers tgt \
               WHERE tgt.aircraft_id = $2 \
                 AND tgt.org_id = src.org_id \
                 AND tgt.identifier = src.identifier \
                 AND tgt.source = src.source \
             ) \
           RETURNING id \
         ) SELECT COUNT(*)::bigint AS n FROM del",
    )
    .bind(source_id)
    .bind(target_id)
    .bind(org_id)
    .fetch_one(&mut *tx)
    .await?
    .get("n");

    let identifiers_migrated: i64 = sqlx::query(
        "WITH mov AS ( \
           UPDATE aircraft_identifiers SET aircraft_id = $1 \
           WHERE aircraft_id = $2 AND org_id = $3 \
           RETURNING id \
         ) SELECT COUNT(*)::bigint AS n FROM mov",
    )
    .bind(target_id)
    .bind(source_id)
    .bind(org_id)
    .fetch_one(&mut *tx)
    .await?
    .get("n");

    // Recompute denormalised counters on the target. Same expressions as
    // services::aircraft_aggregates uses on backfill; kept literal here so
    // the merge is self-contained and matches the scratch script verbatim.
    sqlx::query(
        "UPDATE aircraft SET \
           total_flights = (SELECT COUNT(*) FROM flights \
                             WHERE aircraft_id = $1 AND deleted_at IS NULL), \
           total_seconds = COALESCE((SELECT SUM(duration_seconds) FROM flights \
                                       WHERE aircraft_id = $1 AND deleted_at IS NULL), 0), \
           last_flight   = (SELECT MAX(flight_datetime) FROM flights \
                             WHERE aircraft_id = $1 AND deleted_at IS NULL) \
         WHERE id = $1",
    )
    .bind(target_id)
    .execute(&mut *tx)
    .await?;

    // Build and insert the audit row INSIDE the tx so the audit entry
    // and the aircraft DELETE commit atomically. Mirrors the scratch
    // dedupe script's envelope; uses reason='manual-merge' (the scratch
    // ones used reason='dedupe-merge' so the two sources stay
    // distinguishable in the audit viewer).
    let diff = serde_json::json!({
        "before": source_snapshot,
        "reason": "manual-merge",
        "merged_into_aircraft_id": target_id,
        "merged_into_equipment_id": target_equipment_id,
        "flights_moved": flights_moved,
        "batteries_moved": batteries_moved,
        "controllers_moved": controllers_moved,
        "defects_moved": defects_moved,
        "incidents_moved": incidents_moved,
        "maintenance_moved": maintenance_moved,
        "maintenance_schedules_deleted_as_duplicates": schedules_deleted,
        "identifiers_migrated": identifiers_migrated,
        "deleted_schedules_snapshot": schedules_snapshot,
    });

    let ip_address = crate::services::client_ip::current_client_ip();
    sqlx::query(
        "INSERT INTO audit_log (org_id, user_id, user_code, action, table_name, record_id, \
                                diff_jsonb, ip_address) \
         VALUES ($1, $2, $3, 'DELETE', 'aircraft', $4, $5, $6)",
    )
    .bind(org_id)
    .bind(user_id)
    .bind(user_code)
    .bind(source_id)
    .bind(diff)
    .bind(ip_address.as_deref())
    .execute(&mut *tx)
    .await?;

    sqlx::query("DELETE FROM aircraft WHERE id = $1 AND org_id = $2")
        .bind(source_id)
        .bind(org_id)
        .execute(&mut *tx)
        .await?;

    tx.commit().await?;

    Ok(MergeOutcome {
        deleted_aircraft_id: source_id,
        merged_into_aircraft_id: target_id,
        flights_moved,
        batteries_moved,
        controllers_moved,
        defects_moved,
        incidents_moved,
        maintenance_moved,
        schedules_deleted_as_duplicates: schedules_deleted,
        identifiers_migrated,
    })
}

/// Re-point one child table's `aircraft_id` from `source` to `target` and
/// return the row count. The table name is validated against a small
/// allowlist so the format-string interpolation cannot be coerced.
async fn move_children(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    table: &'static str,
    source_id: i32,
    target_id: i32,
) -> Result<i64, AppError> {
    match table {
        "flights" | "batteries" | "remote_controllers" | "defect_log" | "incidents"
        | "maintenance" => {}
        _ => {
            return Err(AppError::Internal(anyhow::anyhow!(
                "aircraft_merge::move_children: invalid table {}",
                table
            )));
        }
    }
    let sql = format!(
        "WITH mov AS ( \
           UPDATE {} SET aircraft_id = $1 WHERE aircraft_id = $2 RETURNING aircraft_id \
         ) SELECT COUNT(*)::bigint AS n FROM mov",
        table
    );
    let n: i64 = sqlx::query(&sql)
        .bind(target_id)
        .bind(source_id)
        .fetch_one(&mut **tx)
        .await?
        .get("n");
    Ok(n)
}

// =====================================================================
// Integration tests (sqlx::test — each test gets an ephemeral DB with
// migrations applied + the default organisation row from
// 002_seed_reference_data.sql).
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;

    async fn default_org_id(pool: &PgPool) -> i32 {
        sqlx::query("SELECT id FROM organisations ORDER BY id LIMIT 1")
            .fetch_one(pool)
            .await
            .expect("default org seeded")
            .get::<i32, _>("id")
    }

    async fn default_admin_user(pool: &PgPool, org_id: i32) -> (i32, String) {
        let row = sqlx::query(
            "SELECT id, user_code FROM users \
             WHERE org_id = $1 AND role = 'admin' ORDER BY id LIMIT 1",
        )
        .bind(org_id)
        .fetch_optional(pool)
        .await
        .unwrap();
        match row {
            Some(r) => (r.get::<i32, _>("id"), r.get::<String, _>("user_code")),
            None => {
                // Fallback: tests that don't get the seed admin (e.g.
                // an org that we'll insert below) — create one.
                let new = sqlx::query(
                    "INSERT INTO users (org_id, name, email, user_code, password_hash, role) \
                     VALUES ($1, 'Test Admin', 'admin@test.local', 'TST', \
                             '$2b$12$xxxxxxxxxxxxxxxxxxxxxx', 'admin') RETURNING id, user_code",
                )
                .bind(org_id)
                .fetch_one(pool)
                .await
                .unwrap();
                (new.get::<i32, _>("id"), new.get::<String, _>("user_code"))
            }
        }
    }

    async fn seed_model(pool: &PgPool, org_id: i32, name: &str) -> i32 {
        sqlx::query(
            "INSERT INTO aircraft_models (org_id, name) VALUES ($1, $2) RETURNING id",
        )
        .bind(org_id)
        .bind(name)
        .fetch_one(pool)
        .await
        .unwrap()
        .get::<i32, _>("id")
    }

    async fn seed_aircraft(
        pool: &PgPool,
        org_id: i32,
        sn: &str,
        equipment_id: &str,
        model_id: Option<i32>,
        needs_manual_sn: bool,
        status: &str,
    ) -> i32 {
        sqlx::query(
            "INSERT INTO aircraft \
               (org_id, serial_number, equipment_id, model_id, status, needs_manual_sn) \
             VALUES ($1, $2, $3, $4, $5, $6) RETURNING id",
        )
        .bind(org_id)
        .bind(sn)
        .bind(equipment_id)
        .bind(model_id)
        .bind(status)
        .bind(needs_manual_sn)
        .fetch_one(pool)
        .await
        .unwrap()
        .get::<i32, _>("id")
    }

    async fn seed_flight(pool: &PgPool, org_id: i32, aircraft_id: i32, duration: i32) -> i32 {
        sqlx::query(
            "INSERT INTO flights (org_id, aircraft_id, duration_seconds, flight_datetime) \
             VALUES ($1, $2, $3, NOW()) RETURNING id",
        )
        .bind(org_id)
        .bind(aircraft_id)
        .bind(duration)
        .fetch_one(pool)
        .await
        .unwrap()
        .get::<i32, _>("id")
    }

    async fn seed_schedule(pool: &PgPool, org_id: i32, aircraft_id: i32, name: &str) -> i32 {
        sqlx::query(
            "INSERT INTO maintenance_schedules \
               (org_id, aircraft_id, name, interval_type, interval_value) \
             VALUES ($1, $2, $3, 'flight_hours', 50.0) RETURNING id",
        )
        .bind(org_id)
        .bind(aircraft_id)
        .bind(name)
        .fetch_one(pool)
        .await
        .unwrap()
        .get::<i32, _>("id")
    }

    async fn seed_identifier(
        pool: &PgPool,
        org_id: i32,
        aircraft_id: i32,
        identifier: &str,
        source: &str,
    ) {
        sqlx::query(
            "INSERT INTO aircraft_identifiers (aircraft_id, org_id, identifier, source) \
             VALUES ($1, $2, $3, $4)",
        )
        .bind(aircraft_id)
        .bind(org_id)
        .bind(identifier)
        .bind(source)
        .execute(pool)
        .await
        .unwrap();
    }

    // --- Happy path ---

    #[sqlx::test(migrations = "./migrations")]
    async fn merge_succeeds_and_moves_all_children(pool: PgPool) {
        let org_id = default_org_id(&pool).await;
        let (user_id, user_code) = default_admin_user(&pool, org_id).await;
        let model_id = seed_model(&pool, org_id, "DJI Inspire 1 X5").await;

        let target = seed_aircraft(
            &pool, org_id, "W13TEST0000001", "UAV-049",
            Some(model_id), false, "active",
        )
        .await;
        let source = seed_aircraft(
            &pool, org_id, "0410000001", "UAV-PENDING",
            Some(model_id), true, "active",
        )
        .await;

        // Flights on both — only source's should move.
        let _ = seed_flight(&pool, org_id, target, 600).await;
        let _ = seed_flight(&pool, org_id, source, 1200).await;
        let _ = seed_flight(&pool, org_id, source, 900).await;

        // Schedules on both — source's should be deleted as duplicates.
        seed_schedule(&pool, org_id, target, "50h check").await;
        seed_schedule(&pool, org_id, source, "50h check").await;

        // Identifiers on source — should migrate to target.
        seed_identifier(&pool, org_id, source, "0410000001", "body_sn").await;
        seed_identifier(&pool, org_id, source, "Inspire 1 X5", "alias_djigo").await;

        let outcome = merge_aircraft(&pool, source, target, org_id, user_id, &user_code)
            .await
            .expect("merge should succeed");

        assert_eq!(outcome.deleted_aircraft_id, source);
        assert_eq!(outcome.merged_into_aircraft_id, target);
        assert_eq!(outcome.flights_moved, 2, "two flights should have moved");
        assert_eq!(outcome.schedules_deleted_as_duplicates, 1);
        assert_eq!(outcome.identifiers_migrated, 2);

        // Source aircraft is gone.
        let gone: Option<(i32,)> =
            sqlx::query_as("SELECT id FROM aircraft WHERE id = $1")
                .bind(source)
                .fetch_optional(&pool)
                .await
                .unwrap();
        assert!(gone.is_none(), "source aircraft should be deleted");

        // Target now owns 3 flights.
        let target_flights: i64 = sqlx::query(
            "SELECT COUNT(*)::bigint AS n FROM flights WHERE aircraft_id = $1",
        )
        .bind(target)
        .fetch_one(&pool)
        .await
        .unwrap()
        .get("n");
        assert_eq!(target_flights, 3);

        // Target counters recomputed.
        let counters = sqlx::query(
            "SELECT total_flights, total_seconds FROM aircraft WHERE id = $1",
        )
        .bind(target)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(counters.get::<i32, _>("total_flights"), 3);
        assert_eq!(counters.get::<i32, _>("total_seconds"), 600 + 1200 + 900);

        // Audit entry written with reason='manual-merge'.
        let audit = sqlx::query(
            "SELECT diff_jsonb FROM audit_log \
             WHERE action = 'DELETE' AND table_name = 'aircraft' AND record_id = $1",
        )
        .bind(source)
        .fetch_one(&pool)
        .await
        .unwrap();
        let diff: serde_json::Value = audit.get("diff_jsonb");
        assert_eq!(diff["reason"], "manual-merge");
        assert_eq!(diff["merged_into_aircraft_id"], target);
        assert_eq!(diff["merged_into_equipment_id"], "UAV-049");
        assert_eq!(diff["flights_moved"], 2);
        assert_eq!(diff["maintenance_schedules_deleted_as_duplicates"], 1);
        assert!(
            diff["deleted_schedules_snapshot"].is_array(),
            "snapshot must be an array, got: {}",
            diff["deleted_schedules_snapshot"]
        );
        assert!(diff["before"].is_object());
    }

    // --- Validation: source not flagged needs_manual_sn ---

    #[sqlx::test(migrations = "./migrations")]
    async fn rejects_when_source_not_pending_review(pool: PgPool) {
        let org_id = default_org_id(&pool).await;
        let (user_id, user_code) = default_admin_user(&pool, org_id).await;
        let model_id = seed_model(&pool, org_id, "Mavic 3").await;
        let target = seed_aircraft(
            &pool, org_id, "TGT-SN", "UAV-T1", Some(model_id), false, "active",
        )
        .await;
        let source = seed_aircraft(
            &pool, org_id, "SRC-SN", "UAV-T2", Some(model_id), false, "active",
        )
        .await;

        let err = merge_aircraft(&pool, source, target, org_id, user_id, &user_code)
            .await
            .expect_err("expected Conflict");
        match err {
            AppError::Conflict(msg) => {
                assert!(msg.contains("pending-review"), "got: {msg}");
            }
            other => panic!("expected Conflict, got: {:?}", other),
        }
    }

    // --- Validation: target flagged needs_manual_sn ---

    #[sqlx::test(migrations = "./migrations")]
    async fn rejects_when_target_is_pending_review(pool: PgPool) {
        let org_id = default_org_id(&pool).await;
        let (user_id, user_code) = default_admin_user(&pool, org_id).await;
        let model_id = seed_model(&pool, org_id, "Mavic 3").await;
        let target = seed_aircraft(
            &pool, org_id, "TGT-SN", "UAV-T3", Some(model_id), true, "active",
        )
        .await;
        let source = seed_aircraft(
            &pool, org_id, "SRC-SN", "UAV-T4", Some(model_id), true, "active",
        )
        .await;

        let err = merge_aircraft(&pool, source, target, org_id, user_id, &user_code)
            .await
            .expect_err("expected Conflict");
        match err {
            AppError::Conflict(msg) => {
                assert!(msg.contains("another pending-review"), "got: {msg}");
            }
            other => panic!("expected Conflict, got: {:?}", other),
        }
    }

    // --- Validation: model_id mismatch ---

    #[sqlx::test(migrations = "./migrations")]
    async fn rejects_when_models_differ(pool: PgPool) {
        let org_id = default_org_id(&pool).await;
        let (user_id, user_code) = default_admin_user(&pool, org_id).await;
        let model_a = seed_model(&pool, org_id, "Mavic 3").await;
        let model_b = seed_model(&pool, org_id, "Mavic Air 2").await;
        let target = seed_aircraft(
            &pool, org_id, "TGT-SN", "UAV-T5", Some(model_a), false, "active",
        )
        .await;
        let source = seed_aircraft(
            &pool, org_id, "SRC-SN", "UAV-T6", Some(model_b), true, "active",
        )
        .await;

        let err = merge_aircraft(&pool, source, target, org_id, user_id, &user_code)
            .await
            .expect_err("expected Conflict");
        match err {
            AppError::Conflict(msg) => {
                assert!(msg.contains("model mismatch"), "got: {msg}");
            }
            other => panic!("expected Conflict, got: {:?}", other),
        }
    }

    // --- Validation: model_id NULL on either side ---

    #[sqlx::test(migrations = "./migrations")]
    async fn rejects_when_model_id_is_null(pool: PgPool) {
        let org_id = default_org_id(&pool).await;
        let (user_id, user_code) = default_admin_user(&pool, org_id).await;
        let model_id = seed_model(&pool, org_id, "Mavic 3").await;
        let target = seed_aircraft(
            &pool, org_id, "TGT-SN", "UAV-T7", Some(model_id), false, "active",
        )
        .await;
        let source = seed_aircraft(
            &pool, org_id, "SRC-SN", "UAV-T8", None, true, "active",
        )
        .await;

        let err = merge_aircraft(&pool, source, target, org_id, user_id, &user_code)
            .await
            .expect_err("expected Conflict");
        assert!(matches!(err, AppError::Conflict(_)));
    }

    // --- Validation: target retired ---

    #[sqlx::test(migrations = "./migrations")]
    async fn rejects_when_target_is_retired(pool: PgPool) {
        let org_id = default_org_id(&pool).await;
        let (user_id, user_code) = default_admin_user(&pool, org_id).await;
        let model_id = seed_model(&pool, org_id, "Mavic 3").await;
        let target = seed_aircraft(
            &pool, org_id, "TGT-SN", "UAV-T9", Some(model_id), false, "retired",
        )
        .await;
        let source = seed_aircraft(
            &pool, org_id, "SRC-SN", "UAV-TA", Some(model_id), true, "active",
        )
        .await;

        let err = merge_aircraft(&pool, source, target, org_id, user_id, &user_code)
            .await
            .expect_err("expected Conflict");
        match err {
            AppError::Conflict(msg) => {
                assert!(msg.contains("must be active"), "got: {msg}");
            }
            other => panic!("expected Conflict, got: {:?}", other),
        }
    }

    // --- Validation: source.id == target.id ---

    #[sqlx::test(migrations = "./migrations")]
    async fn rejects_self_merge(pool: PgPool) {
        let org_id = default_org_id(&pool).await;
        let (user_id, user_code) = default_admin_user(&pool, org_id).await;
        let model_id = seed_model(&pool, org_id, "Mavic 3").await;
        let a = seed_aircraft(
            &pool, org_id, "ONLY-SN", "UAV-TB", Some(model_id), true, "active",
        )
        .await;

        let err = merge_aircraft(&pool, a, a, org_id, user_id, &user_code)
            .await
            .expect_err("expected BadRequest");
        assert!(matches!(err, AppError::BadRequest(_)));
    }

    // --- Validation: org mismatch ---

    #[sqlx::test(migrations = "./migrations")]
    async fn rejects_org_mismatch(pool: PgPool) {
        let org_id = default_org_id(&pool).await;
        let (user_id, user_code) = default_admin_user(&pool, org_id).await;

        // Create a second org for the source aircraft.
        let other_org: i32 = sqlx::query(
            "INSERT INTO organisations (name, slug) VALUES ('Other Co', 'other-co') RETURNING id",
        )
        .fetch_one(&pool)
        .await
        .unwrap()
        .get("id");
        let other_model = seed_model(&pool, other_org, "Mavic 3").await;
        let model_id = seed_model(&pool, org_id, "Mavic 3").await;

        let target = seed_aircraft(
            &pool, org_id, "TGT-SN", "UAV-TC", Some(model_id), false, "active",
        )
        .await;
        let source = seed_aircraft(
            &pool, other_org, "SRC-SN", "UAV-TD", Some(other_model), true, "active",
        )
        .await;

        let err = merge_aircraft(&pool, source, target, org_id, user_id, &user_code)
            .await
            .expect_err("expected Forbidden");
        assert!(matches!(err, AppError::Forbidden(_)));
    }

    // --- aircraft_identifiers collision handling ---
    //
    // The unique constraint on `(org_id, identifier, source)` from
    // migration 025 makes the collision state unreachable in production:
    // two aircraft in the same org cannot simultaneously hold the same
    // (identifier, source). The merge function still carries a defensive
    // DELETE-then-UPDATE that would handle the collision if the
    // constraint were ever relaxed (or if a future schema change made it
    // per-aircraft instead of per-org).
    //
    // To exercise that branch, this test drops the unique constraint
    // before seeding the colliding rows. Each sqlx::test gets its own
    // ephemeral DB so the drop is isolated.

    #[sqlx::test(migrations = "./migrations")]
    async fn identifier_collision_is_discarded_not_duplicated(pool: PgPool) {
        let org_id = default_org_id(&pool).await;
        let (user_id, user_code) = default_admin_user(&pool, org_id).await;
        let model_id = seed_model(&pool, org_id, "Matrice 600").await;
        let target = seed_aircraft(
            &pool, org_id, "M64TEST0000043", "UAV-004",
            Some(model_id), false, "active",
        )
        .await;
        let source = seed_aircraft(
            &pool, org_id, "0670000002", "UAV-PENDING-M600",
            Some(model_id), true, "active",
        )
        .await;

        // Drop the org-wide uniqueness so we can stage the (otherwise
        // impossible) collision state the merge code is hardened against.
        sqlx::query(
            "ALTER TABLE aircraft_identifiers \
             DROP CONSTRAINT aircraft_identifiers_org_id_identifier_source_key",
        )
        .execute(&pool)
        .await
        .unwrap();

        // The same FC SN is already registered on the target — and now
        // appears on the source too. Both with source='fc_sn'.
        seed_identifier(&pool, org_id, target, "0670000002", "fc_sn").await;
        seed_identifier(&pool, org_id, source, "0670000002", "fc_sn").await;
        // Plus a non-colliding identifier on source — should still migrate.
        seed_identifier(&pool, org_id, source, "M600-alias", "alias_djigo").await;

        let outcome = merge_aircraft(&pool, source, target, org_id, user_id, &user_code)
            .await
            .expect("merge should succeed despite collision");

        // Only the non-colliding identifier migrated; the colliding fc_sn
        // row was discarded from source rather than re-pointed.
        assert_eq!(
            outcome.identifiers_migrated, 1,
            "collision row must be discarded, only the alias should be reported as migrated"
        );

        // Target now owns exactly two identifiers: the original fc_sn
        // (kept) and the migrated alias. The collision must not have
        // produced a duplicate.
        let count: i64 = sqlx::query(
            "SELECT COUNT(*)::bigint AS n FROM aircraft_identifiers \
             WHERE aircraft_id = $1 AND org_id = $2",
        )
        .bind(target)
        .bind(org_id)
        .fetch_one(&pool)
        .await
        .unwrap()
        .get("n");
        assert_eq!(count, 2);

        // Specifically: fc_sn=0670000002 still exists exactly once.
        let dup_check: i64 = sqlx::query(
            "SELECT COUNT(*)::bigint AS n FROM aircraft_identifiers \
             WHERE org_id = $1 AND identifier = '0670000002' AND source = 'fc_sn'",
        )
        .bind(org_id)
        .fetch_one(&pool)
        .await
        .unwrap()
        .get("n");
        assert_eq!(dup_check, 1, "must have exactly one fc_sn=0670000002 row");
    }
}
