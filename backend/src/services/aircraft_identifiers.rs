// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

//! Service for managing aircraft_identifiers — the association table linking
//! one aircraft to multiple identifier strings from different sources.
//!
//! Background: DJI Inspire 1 (and some Matrice variants) record a flight
//! controller serial number in their log files that is structurally different
//! from the body/printed serial number the operator registered the aircraft
//! under. For example, `W13TEST0000001` (body SN) vs `0410000001` (FC SN) —
//! same physical drone, no common prefix. The 2-char prefix tolerance in
//! sn_matching::find_by_serial cannot bridge this gap. This module provides
//! the lookup that complements the canonical aircraft.serial_number column.

use sqlx::{PgExecutor, Row};

use crate::errors::AppError;
use crate::services::sn_matching::EquipmentMatch;

/// Source values accepted by the aircraft_identifiers.source CHECK constraint.
/// Keep in sync with migrations 025 and 043 (043 added 'ardupilot_uid').
pub const SOURCE_BODY_SN: &str = "body_sn";
pub const SOURCE_FC_SN: &str = "fc_sn";
pub const SOURCE_ALIAS_DJIGO: &str = "alias_djigo";
pub const SOURCE_ALIAS_DJIFLY: &str = "alias_djifly";
#[allow(dead_code)]
pub const SOURCE_ALIAS_MANUAL: &str = "alias_manual";
#[allow(dead_code)]
pub const SOURCE_QR_LABEL: &str = "qr_label";
/// ArduPilot autopilot board CPU UID — the serial-number analogue for an
/// ArduPilot airframe (no body SN is broadcast in MAVLink). Used by the .tlog
/// commit path (link / auto-register) in handlers::flights::commit.
pub const SOURCE_ARDUPILOT_UID: &str = "ardupilot_uid";

/// DJI placeholder names that should never be persisted as aliases.
/// Compared case-insensitively against the trimmed alias.
const ALIAS_PLACEHOLDER_BLOCKLIST: &[&str] = &[
    "choose a name for your inspire",
    // Add more placeholders here as they are encountered in the wild.
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

/// Looks up an aircraft by any of its non-canonical identifiers.
///
/// Returns `Ok(Some)` only when EXACTLY one aircraft matches. Returns
/// `Ok(None)` for zero matches AND for ambiguous (>=2) matches — the
/// latter is logged at warn level so operators can investigate.
///
/// Does NOT consult `aircraft.serial_number` — that remains the caller's
/// responsibility (in `sn_matching::find_match`).
pub async fn find_aircraft_by_identifier<'c, E>(
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
        "SELECT a.id, a.equipment_id, a.serial_number \
         FROM aircraft_identifiers ai \
         JOIN aircraft a ON a.id = ai.aircraft_id \
         WHERE ai.org_id = $1 AND ai.identifier = $2 \
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
                "aircraft_identifiers ambiguous — multiple aircraft match this identifier; treating as no match"
            );
            Ok(None)
        }
    }
}

/// Inserts an aircraft_identifier row. Idempotent on `(org_id, identifier, source)` —
/// re-inserting the same triple is a no-op (returns Ok(None)).
///
/// Returns `Ok(Some(id))` when a new row was created, `Ok(None)` when the row
/// already existed (ON CONFLICT DO NOTHING fired) or when identifier was blank.
pub async fn link_identifier<'c, E>(
    executor: E,
    aircraft_id: i32,
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
        "INSERT INTO aircraft_identifiers \
            (aircraft_id, org_id, identifier, source, observed_in, created_by) \
         VALUES ($1, $2, $3, $4, $5, $6) \
         ON CONFLICT (org_id, identifier, source) DO NOTHING \
         RETURNING id",
    )
    .bind(aircraft_id)
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

/// Best-effort recording of identifier rows for a freshly auto-registered aircraft.
/// Inserts (a) the SN row classified as either `body_sn` or `fc_sn` (see
/// `from_unmatched_log` below), and (b) `alias_djigo` or `alias_djifly` from the
/// DJI-side alias when present and persistable.
///
/// `from_unmatched_log` toggles the SN row's classification:
/// * `false` — the caller has independent confidence that the SN is the chassis
///   body SN (e.g. QR-scanner batch import, manual create). Stored as `body_sn`.
/// * `true` — the SN came from a DJI flight log that didn't match any existing
///   aircraft. On some airframes (Inspire 1, Matrice 600, …) the log carries
///   the flight-controller SN rather than the body SN, so we classify it as
///   `fc_sn` and let the pending-review flow confirm or merge.
///
/// Failures are logged at warn level but never propagated — matching the relaxed
/// sequential pattern of commit.rs (the flight insert and aircraft INSERT are the
/// primary side-effects; identifier rows are best-effort denormalization).
pub async fn record_auto_register_identifiers<'c, E>(
    executor: E,
    aircraft_id: i32,
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
        aircraft_id,
        org_id,
        body_sn,
        sn_source,
        Some(&observed_in),
        created_by,
    )
    .await
    {
        tracing::warn!(error = ?e, aircraft_id, sn_source, "failed to record SN identifier");
    }

    if let Some(alias) = alias {
        if is_persistable_alias(alias) {
            let source = match app_source {
                Some("DJIFly") => SOURCE_ALIAS_DJIFLY,
                _ => SOURCE_ALIAS_DJIGO,
            };
            if let Err(e) = link_identifier(
                executor,
                aircraft_id,
                org_id,
                alias,
                source,
                Some(&observed_in),
                created_by,
            )
            .await
            {
                tracing::warn!(error = ?e, aircraft_id, alias, "failed to record alias identifier");
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
        assert!(is_persistable_alias("UAV-049"));
        assert!(is_persistable_alias("UAV101"));
        assert!(is_persistable_alias("My Drone"));
    }

    /// Helper: insert minimal org + aircraft and return the aircraft id.
    /// 002_seed_reference_data.sql seeds a default org; we upsert id=1 here
    /// to keep the test self-contained even if the seed shape ever changes.
    async fn seed_aircraft(pool: &PgPool, serial: &str) -> i32 {
        sqlx::query("INSERT INTO organisations (id, name, slug) VALUES (1, 'Test Org', 'test-org') ON CONFLICT (id) DO NOTHING")
            .execute(pool)
            .await
            .unwrap();
        let row = sqlx::query(
            "INSERT INTO aircraft (org_id, serial_number, equipment_id) VALUES (1, $1, 'UAV-TEST') RETURNING id",
        )
        .bind(serial)
        .fetch_one(pool)
        .await
        .unwrap();
        row.get::<i32, _>("id")
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn find_by_identifier_returns_none_for_unknown(pool: PgPool) {
        let result = find_aircraft_by_identifier(&pool, 1, "UNKNOWN").await.unwrap();
        assert!(result.is_none());
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn find_by_identifier_finds_fc_sn(pool: PgPool) {
        let aircraft_id = seed_aircraft(&pool, "W13TEST0000001").await;
        link_identifier(&pool, aircraft_id, 1, "0410000001", SOURCE_FC_SN, None, None)
            .await
            .unwrap();

        let result = find_aircraft_by_identifier(&pool, 1, "0410000001").await.unwrap();
        let m = result.expect("expected match");
        assert_eq!(m.id, aircraft_id);
        assert_eq!(m.serial_number, "W13TEST0000001");
        assert_eq!(m.equipment_id.as_deref(), Some("UAV-TEST"));
        assert!(!m.prefix_matched);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn find_by_identifier_finds_alias(pool: PgPool) {
        let aircraft_id = seed_aircraft(&pool, "W13TEST0000001").await;
        link_identifier(&pool, aircraft_id, 1, "UAV101", SOURCE_ALIAS_DJIGO, None, None)
            .await
            .unwrap();

        let result = find_aircraft_by_identifier(&pool, 1, "UAV101").await.unwrap();
        assert_eq!(result.unwrap().id, aircraft_id);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn link_identifier_is_idempotent(pool: PgPool) {
        let aircraft_id = seed_aircraft(&pool, "W13TEST0000001").await;
        let first =
            link_identifier(&pool, aircraft_id, 1, "0410000001", SOURCE_FC_SN, None, None)
                .await
                .unwrap();
        let second =
            link_identifier(&pool, aircraft_id, 1, "0410000001", SOURCE_FC_SN, None, None)
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
        let aircraft_id = seed_aircraft(&pool, "W13TEST0000001").await;
        record_auto_register_identifiers(
            &pool,
            aircraft_id,
            1,
            "W13TEST0000001",
            Some("UAV-049"),
            Some("DJIGO4"),
            42,
            Some(1),
            false,
        )
        .await;

        let count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM aircraft_identifiers WHERE aircraft_id = $1")
                .bind(aircraft_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(count.0, 2, "expected body_sn + alias_djigo");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn record_auto_register_skips_placeholder_alias(pool: PgPool) {
        let aircraft_id = seed_aircraft(&pool, "W13TEST0000001").await;
        record_auto_register_identifiers(
            &pool,
            aircraft_id,
            1,
            "W13TEST0000001",
            Some("choose a name for your inspire"),
            Some("DJIGO4"),
            42,
            Some(1),
            false,
        )
        .await;

        let count: (i64,) =
            sqlx::query_as("SELECT COUNT(*) FROM aircraft_identifiers WHERE aircraft_id = $1")
                .bind(aircraft_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(count.0, 1, "expected only body_sn (alias was placeholder)");
    }

    /// `from_unmatched_log = false` classifies the SN row as body_sn.
    #[sqlx::test(migrations = "./migrations")]
    async fn record_auto_register_from_known_source_uses_body_sn(pool: PgPool) {
        let aircraft_id = seed_aircraft(&pool, "W13TEST0000001").await;
        record_auto_register_identifiers(
            &pool,
            aircraft_id,
            1,
            "W13TEST0000001",
            None,
            None,
            42,
            Some(1),
            false,
        )
        .await;

        let row: (String,) = sqlx::query_as(
            "SELECT source FROM aircraft_identifiers \
             WHERE aircraft_id = $1 AND identifier = $2",
        )
        .bind(aircraft_id)
        .bind("W13TEST0000001")
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(row.0, "body_sn");
    }

    /// `from_unmatched_log = true` classifies the SN row as fc_sn — needed for
    /// the DJI-log auto-register path where the log SN often turns out to be
    /// the flight controller SN of an aircraft we've already registered under
    /// its chassis body SN. The pending-review UI uses this classification to
    /// distinguish "needs human confirmation" rows from "trusted body SN" rows.
    #[sqlx::test(migrations = "./migrations")]
    async fn record_auto_register_from_unmatched_log_uses_fc_sn(pool: PgPool) {
        let aircraft_id = seed_aircraft(&pool, "0410000001").await;
        record_auto_register_identifiers(
            &pool,
            aircraft_id,
            1,
            "0410000001",
            Some("UAV-049"),
            Some("DJIGO4"),
            42,
            Some(1),
            true,
        )
        .await;

        let sn_row: (String,) = sqlx::query_as(
            "SELECT source FROM aircraft_identifiers \
             WHERE aircraft_id = $1 AND identifier = $2",
        )
        .bind(aircraft_id)
        .bind("0410000001")
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(sn_row.0, "fc_sn");

        // Alias row classification is unaffected by from_unmatched_log.
        let alias_row: (String,) = sqlx::query_as(
            "SELECT source FROM aircraft_identifiers \
             WHERE aircraft_id = $1 AND identifier = $2",
        )
        .bind(aircraft_id)
        .bind("UAV-049")
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(alias_row.0, "alias_djigo");
    }
}
