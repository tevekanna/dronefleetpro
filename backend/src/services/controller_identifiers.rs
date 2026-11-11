// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

//! Service for managing controller_identifiers — the association table linking
//! one remote controller to multiple identifier strings from different sources.
//!
//! Background: dji-log-parser 0.5 returns FC-side / firmware-side serial
//! numbers for some DJI controllers (M600, Inspire-class) that are
//! structurally different from the body / printed serial number the
//! operator registered the controller under via QR scan. The 2-char prefix
//! tolerance in sn_matching::find_by_serial cannot bridge that gap. This
//! module provides the lookup that complements the canonical
//! remote_controllers.serial_number column. Mirrors aircraft_identifiers.

use sqlx::{PgExecutor, Row};

use crate::errors::AppError;
use crate::services::sn_matching::EquipmentMatch;

/// Source values accepted by the controller_identifiers.source CHECK constraint.
/// Keep in sync with migration 030.
pub const SOURCE_BODY_SN: &str = "body_sn";
pub const SOURCE_FC_SN: &str = "fc_sn";
pub const SOURCE_ALIAS_DJIGO: &str = "alias_djigo";
pub const SOURCE_ALIAS_DJIFLY: &str = "alias_djifly";
#[allow(dead_code)]
pub const SOURCE_ALIAS_MANUAL: &str = "alias_manual";
#[allow(dead_code)]
pub const SOURCE_QR_LABEL: &str = "qr_label";

/// DJI placeholder names that should never be persisted as aliases.
/// Compared case-insensitively against the trimmed alias. Kept in lock-step
/// with aircraft_identifiers — same wild placeholders surface on the
/// controller side from the same DJI app builds.
const ALIAS_PLACEHOLDER_BLOCKLIST: &[&str] = &[
    "choose a name for your inspire",
];

/// True if the candidate alias is suitable to persist as an identifier.
/// Filters empty strings, whitespace-only strings, and DJI placeholder names.
pub fn is_persistable_alias(alias: &str) -> bool {
    let trimmed = alias.trim();
    if trimmed.is_empty() {
        return false;
    }
    let lower = trimmed.to_ascii_lowercase();
    !ALIAS_PLACEHOLDER_BLOCKLIST
        .iter()
        .any(|blocked| lower == *blocked)
}

/// Looks up a remote controller by any of its non-canonical identifiers.
///
/// Returns `Ok(Some)` only when EXACTLY one controller matches. Returns
/// `Ok(None)` for zero matches AND for ambiguous (>=2) matches — the
/// latter is logged at warn level so operators can investigate.
///
/// Does NOT consult `remote_controllers.serial_number` — that remains the
/// caller's responsibility (in `sn_matching::find_match`).
pub async fn find_controller_by_identifier<'c, E>(
    executor: E,
    org_id: i32,
    identifier: &str,
) -> Result<Option<EquipmentMatch>, AppError>
where
    E: PgExecutor<'c>,
{
    let identifier_trimmed = identifier.trim();
    if identifier_trimmed.is_empty() {
        return Ok(None);
    }

    let rows = sqlx::query(
        "SELECT c.id, c.equipment_id, c.serial_number \
         FROM controller_identifiers ci \
         JOIN remote_controllers c ON c.id = ci.controller_id \
         WHERE ci.org_id = $1 AND ci.identifier = $2 \
         LIMIT 2",
    )
    .bind(org_id)
    .bind(identifier_trimmed)
    .fetch_all(executor)
    .await
    .map_err(AppError::Database)?;

    match rows.len() {
        1 => {
            let row = &rows[0];
            let id: i32 = row.get("id");
            let equipment_id: Option<String> = row.try_get("equipment_id").ok().flatten();
            let serial_number: String = row.get("serial_number");
            Ok(Some(EquipmentMatch {
                id,
                equipment_id,
                serial_number,
                prefix_matched: false,
            }))
        }
        0 => Ok(None),
        _ => {
            tracing::warn!(
                identifier = %identifier_trimmed,
                org_id,
                "controller_identifiers ambiguous — multiple controllers match this identifier; treating as no match"
            );
            Ok(None)
        }
    }
}

/// Inserts a controller_identifier row. Idempotent on `(org_id, identifier, source)` —
/// re-inserting the same triple is a no-op (returns Ok(None)).
///
/// Returns `Ok(Some(id))` when a new row was created, `Ok(None)` when the row
/// already existed (ON CONFLICT DO NOTHING fired) or when identifier was blank.
pub async fn link_identifier<'c, E>(
    executor: E,
    controller_id: i32,
    org_id: i32,
    identifier: &str,
    source: &str,
    observed_in: Option<&str>,
    created_by: Option<i32>,
) -> Result<Option<i32>, AppError>
where
    E: PgExecutor<'c>,
{
    let identifier_trimmed = identifier.trim();
    if identifier_trimmed.is_empty() {
        return Ok(None);
    }

    let row = sqlx::query(
        "INSERT INTO controller_identifiers \
            (controller_id, org_id, identifier, source, observed_in, created_by) \
         VALUES ($1, $2, $3, $4, $5, $6) \
         ON CONFLICT (org_id, identifier, source) DO NOTHING \
         RETURNING id",
    )
    .bind(controller_id)
    .bind(org_id)
    .bind(identifier_trimmed)
    .bind(source)
    .bind(observed_in)
    .bind(created_by)
    .fetch_optional(executor)
    .await
    .map_err(AppError::Database)?;

    Ok(row.map(|r| r.get::<i32, _>("id")))
}

/// Best-effort recording of identifier rows for a freshly auto-registered
/// remote controller. Inserts (a) the SN row classified as either `body_sn`
/// or `fc_sn` (see `from_unmatched_log` below), and (b) `alias_djigo` or
/// `alias_djifly` from the DJI-side alias when present and persistable.
///
/// `from_unmatched_log` toggles the SN row's classification:
/// * `false` — the caller has independent confidence that the SN is the
///   chassis body SN (e.g. QR-scanner batch import, manual create). Stored
///   as `body_sn`.
/// * `true` — the SN came from a DJI flight log that didn't match any
///   existing controller. On M600 / Inspire-class hardware the log carries
///   the FC SN rather than the body SN, so we classify it as `fc_sn` and
///   let the pending-review flow confirm or merge.
///
/// Failures are logged at warn level but never propagated — matching the
/// relaxed sequential pattern of commit.rs (the flight insert and the
/// controller INSERT are the primary side-effects; identifier rows are
/// best-effort denormalization).
pub async fn record_auto_register_identifiers<'c, E>(
    executor: E,
    controller_id: i32,
    org_id: i32,
    body_sn: &str,
    alias: Option<&str>,
    app_source: Option<&str>,
    flight_id: i32,
    created_by: Option<i32>,
    from_unmatched_log: bool,
) where
    E: PgExecutor<'c> + Copy,
{
    let observed_in = format!("flight:{flight_id}");
    let sn_source = if from_unmatched_log {
        SOURCE_FC_SN
    } else {
        SOURCE_BODY_SN
    };

    if let Err(e) = link_identifier(
        executor,
        controller_id,
        org_id,
        body_sn,
        sn_source,
        Some(&observed_in),
        created_by,
    )
    .await
    {
        tracing::warn!(error = ?e, controller_id, sn_source, "failed to record SN identifier");
    }

    if let Some(alias) = alias {
        if is_persistable_alias(alias) {
            let source = match app_source {
                Some("DJIFly") => SOURCE_ALIAS_DJIFLY,
                _ => SOURCE_ALIAS_DJIGO,
            };
            if let Err(e) = link_identifier(
                executor,
                controller_id,
                org_id,
                alias,
                source,
                Some(&observed_in),
                created_by,
            )
            .await
            {
                tracing::warn!(error = ?e, controller_id, alias, "failed to record alias identifier");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::PgPool;

    #[test]
    fn alias_placeholder_filtered() {
        assert!(!is_persistable_alias("choose a name for your inspire"));
        assert!(!is_persistable_alias("Choose A Name For Your Inspire"));
        assert!(!is_persistable_alias("  CHOOSE A NAME FOR YOUR INSPIRE  "));
    }

    #[test]
    fn alias_blank_filtered() {
        assert!(!is_persistable_alias(""));
        assert!(!is_persistable_alias("   "));
        assert!(!is_persistable_alias("\t\n"));
    }

    #[test]
    fn alias_real_persists() {
        assert!(is_persistable_alias("CTL-006"));
        assert!(is_persistable_alias("CTL101"));
        assert!(is_persistable_alias("My Controller"));
    }

    async fn seed_controller(pool: &PgPool, serial: &str) -> i32 {
        sqlx::query("INSERT INTO organisations (id, name, slug) VALUES (1, 'Test Org', 'test-org') ON CONFLICT (id) DO NOTHING")
            .execute(pool)
            .await
            .unwrap();
        let row = sqlx::query(
            "INSERT INTO remote_controllers (org_id, serial_number, equipment_id) VALUES (1, $1, 'CTL-TEST') RETURNING id",
        )
        .bind(serial)
        .fetch_one(pool)
        .await
        .unwrap();
        row.get::<i32, _>("id")
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn find_by_identifier_returns_none_for_unknown(pool: PgPool) {
        let result = find_controller_by_identifier(&pool, 1, "UNKNOWN").await.unwrap();
        assert!(result.is_none());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn find_by_identifier_finds_fc_sn(pool: PgPool) {
        let controller_id = seed_controller(&pool, "06FTEST0000002").await;
        link_identifier(&pool, controller_id, 1, "03L0000003", SOURCE_FC_SN, None, None)
            .await
            .unwrap();

        let result = find_controller_by_identifier(&pool, 1, "03L0000003").await.unwrap();
        let m = result.expect("expected match");
        assert_eq!(m.id, controller_id);
        assert_eq!(m.serial_number, "06FTEST0000002");
        assert_eq!(m.equipment_id.as_deref(), Some("CTL-TEST"));
        assert!(!m.prefix_matched);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn link_identifier_is_idempotent(pool: PgPool) {
        let controller_id = seed_controller(&pool, "06FTEST0000002").await;
        let first =
            link_identifier(&pool, controller_id, 1, "03L0000003", SOURCE_FC_SN, None, None)
                .await
                .unwrap();
        let second =
            link_identifier(&pool, controller_id, 1, "03L0000003", SOURCE_FC_SN, None, None)
                .await
                .unwrap();
        assert!(first.is_some(), "first insert should create row");
        assert!(
            second.is_none(),
            "second insert should be a no-op via ON CONFLICT"
        );
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn record_auto_register_inserts_both_identifiers(pool: PgPool) {
        let controller_id = seed_controller(&pool, "06FTEST0000002").await;
        record_auto_register_identifiers(
            &pool,
            controller_id,
            1,
            "06FTEST0000002",
            Some("CTL-006"),
            Some("DJIGO4"),
            42,
            Some(1),
            false,
        )
        .await;

        let count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM controller_identifiers WHERE controller_id = $1 AND source IN ('body_sn','alias_djigo')")
                .bind(controller_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(count.0, 2, "expected body_sn + alias_djigo");
    }

    /// `from_unmatched_log = true` classifies the SN row as fc_sn.
    #[sqlx::test(migrations = "./migrations")]
    async fn record_auto_register_from_unmatched_log_uses_fc_sn(pool: PgPool) {
        let controller_id = seed_controller(&pool, "03L0000003").await;
        record_auto_register_identifiers(
            &pool,
            controller_id,
            1,
            "03L0000003",
            None,
            None,
            42,
            Some(1),
            true,
        )
        .await;

        let row: (String,) = sqlx::query_as(
            "SELECT source FROM controller_identifiers \
             WHERE controller_id = $1 AND identifier = $2",
        )
        .bind(controller_id)
        .bind("03L0000003")
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(row.0, "fc_sn");
    }
}
