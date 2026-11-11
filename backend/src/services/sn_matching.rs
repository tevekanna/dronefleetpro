// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

//! SN matching helpers for the auto-register flow.
//!
//! Background: `dji-log-parser = "0.5"` sometimes returns serial numbers that
//! differ from the SN printed on the device QR sticker in two distinct ways:
//!
//! 1. **Truncation** — the log SN is 1 character shorter than the body SN
//!    (confirmed for Mavic Pro batteries: QR 14 chars, log 13 chars). The
//!    2-char prefix-tolerance fallback in `find_by_serial` reconciles this.
//!
//! 2. **Structurally different FC SN** — on some DJI airframes the parser
//!    returns the flight-controller / firmware SN, which shares zero common
//!    prefix with the body SN the operator scanned. Confirmed for aircraft
//!    (Inspire 1 — handled via aircraft_identifiers, migration 025) and
//!    empirically observed for controllers and batteries
//!    (in a production database an auto-registered controller with FC SN
//!    03L0000003 shadowed the existing controller with body SN
//!    06FTEST0000002). Migration 030 added controller_identifiers and
//!    battery_identifiers so all three equipment types now use the same
//!    indirection — see `find_match` for the unified 3-step pipeline.
//!    [2026-05-17] Earlier comments in this file claimed the 2-char prefix
//!    tolerance was sufficient for batteries and controllers; that premise
//!    was wrong and is replaced by the symmetric architecture below.
//!
//! Ambiguous prefix matches (≥2 candidates) are treated as "no match" — the
//! caller logs a warning and falls through to auto-register so the operator
//! can fix the duplicate manually rather than letting us guess.

use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Row};

use crate::errors::AppError;
use crate::services::{aircraft_identifiers, battery_identifiers, controller_identifiers};

const PREFIX_TOLERANCE: i32 = 2;

#[derive(Debug)]
pub enum MatchResult {
    /// Exact match on serial_number.
    Exact { id: i32 },
    /// Single prefix candidate found (caller logs at info level).
    PrefixSingle {
        id: i32,
        log_sn: String,
        db_sn: String,
    },
    /// Multiple prefix candidates — ambiguous, treated as no match.
    PrefixAmbiguous { count: usize },
    /// No match at all.
    None,
}

/// Resolved equipment match suitable for the import-preview response and
/// the commit auto-link path. Both code paths build it via `find_match`
/// so they cannot disagree about whether a log SN maps to an existing
/// row.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EquipmentMatch {
    pub id: i32,
    /// `equipment_id` text column (e.g. `"UAV-020"`). NULL on rows that
    /// pre-date the equipment-ID system.
    pub equipment_id: Option<String>,
    /// Canonical serial number as stored in the DB (may differ from the
    /// log SN by 1–2 trailing chars when `prefix_matched` is true).
    pub serial_number: String,
    /// `true` when the lookup hit via the prefix-tolerance fallback in
    /// `find_by_serial`, `false` when an exact SN match succeeded.
    pub prefix_matched: bool,
}

fn validate_table(table: &str) -> Result<(), AppError> {
    if matches!(table, "aircraft" | "batteries" | "remote_controllers") {
        Ok(())
    } else {
        Err(AppError::Internal(anyhow::anyhow!(
            "sn_matching: invalid table name: {}",
            table
        )))
    }
}

/// Pure classification of SQL results — exposed for unit tests.
fn classify(
    log_sn: &str,
    exact_id: Option<i32>,
    prefix_candidates: Vec<(i32, String)>,
) -> MatchResult {
    if let Some(id) = exact_id {
        return MatchResult::Exact { id };
    }
    match prefix_candidates.len() {
        0 => MatchResult::None,
        1 => {
            let (id, db_sn) = prefix_candidates.into_iter().next().unwrap();
            MatchResult::PrefixSingle {
                id,
                log_sn: log_sn.to_string(),
                db_sn,
            }
        }
        n => MatchResult::PrefixAmbiguous { count: n },
    }
}

/// Look up equipment by serial number with exact-then-prefix matching.
///
/// `table` must be one of: `"aircraft"`, `"batteries"`, `"remote_controllers"`.
/// (Validated to prevent SQL injection — the table name is interpolated into
/// the query.)
///
/// Returns `MatchResult::None` for empty/whitespace input — callers should
/// use that to skip the lookup entirely.
pub async fn find_by_serial(
    pool: &PgPool,
    table: &'static str,
    org_id: i32,
    log_sn: &str,
) -> Result<MatchResult, AppError> {
    validate_table(table)?;

    let log_sn_trimmed = log_sn.trim();
    if log_sn_trimmed.is_empty() {
        return Ok(MatchResult::None);
    }

    let exact_sql = format!(
        "SELECT id FROM {} \
         WHERE org_id = $1 AND serial_number = $2 LIMIT 1",
        table
    );
    let exact_id: Option<i32> = sqlx::query(&exact_sql)
        .bind(org_id)
        .bind(log_sn_trimmed)
        .fetch_optional(pool)
        .await?
        .map(|r| r.get::<i32, _>("id"));

    if exact_id.is_some() {
        return Ok(classify(log_sn_trimmed, exact_id, Vec::new()));
    }

    let prefix_sql = format!(
        "SELECT id, serial_number FROM {} \
         WHERE org_id = $1 \
         AND serial_number LIKE $2 || '%' \
         AND length(serial_number) - length($2) <= $3 \
         AND length(serial_number) - length($2) >= 0 \
         LIMIT 5",
        table
    );
    let candidates: Vec<(i32, String)> = sqlx::query(&prefix_sql)
        .bind(org_id)
        .bind(log_sn_trimmed)
        .bind(PREFIX_TOLERANCE)
        .fetch_all(pool)
        .await?
        .into_iter()
        .map(|r| (r.get::<i32, _>("id"), r.get::<String, _>("serial_number")))
        .collect();

    Ok(classify(log_sn_trimmed, None, candidates))
}

/// Looks up `equipment_id` and the canonical `serial_number` for an
/// already-resolved row id. Used by `find_match` to enrich a
/// `MatchResult` into an `EquipmentMatch`.
async fn lookup_canonical(
    pool: &PgPool,
    table: &'static str,
    org_id: i32,
    id: i32,
) -> Result<(Option<String>, String), AppError> {
    validate_table(table)?;
    let sql = format!(
        "SELECT equipment_id, serial_number FROM {} WHERE id = $1 AND org_id = $2",
        table
    );
    let row = sqlx::query(&sql)
        .bind(id)
        .bind(org_id)
        .fetch_one(pool)
        .await?;
    let equipment_id: Option<String> = row.try_get("equipment_id").ok().flatten();
    let serial_number: String = row.get("serial_number");
    Ok((equipment_id, serial_number))
}

/// Unified 3-step lookup that returns the resolved row's equipment_id +
/// canonical SN when any of {exact serial, identifier-table hit, prefix
/// tolerance} succeeds. Used by both the import-preview handler and the
/// commit handler so the modal's match indicator and the actual auto-link
/// decision cannot diverge.
///
/// Pipeline (in priority order):
/// 1. **Exact** on `<table>.serial_number` — the strongest signal, always wins.
/// 2. **Identifier table** — `<entity>_identifiers` (aircraft_identifiers from
///    migration 025; controller_identifiers and battery_identifiers from
///    migration 030). Carries FC SNs, DJI aliases, manual aliases, QR labels.
///    Ranks above prefix-tolerance because identifier rows are curated /
///    intentional and prefix-tolerance is a heuristic for the dji-log-parser
///    truncation bug.
/// 3. **Prefix-tolerance** on `<table>.serial_number` — handles the
///    dji-log-parser 1-char truncation case.
///
/// Implementation choice: inline `match table` in step 2 rather than a
/// closure/trait, because the per-table identifier lookup is the only thing
/// that varies between equipment types and Rust's async-closure ergonomics
/// don't pay back the abstraction overhead at three call sites.
///
/// Returns `Ok(None)` when:
/// * the log SN is empty / whitespace,
/// * no exact match, no identifier hit, no prefix candidates, OR
/// * multiple prefix candidates were found (ambiguous — refuses to guess;
///   logs a warning under `target = "sn_matching"`).
pub async fn find_match(
    pool: &PgPool,
    table: &'static str,
    org_id: i32,
    log_sn: &str,
) -> Result<Option<EquipmentMatch>, AppError> {
    validate_table(table)?;

    let log_sn_trimmed = log_sn.trim();
    if log_sn_trimmed.is_empty() {
        return Ok(None);
    }

    // 1) Exact match on the canonical serial_number column.
    if let Some(m) = find_exact(pool, table, org_id, log_sn_trimmed).await? {
        tracing::debug!(
            target: "sn_matching",
            "{} matched via {}.serial_number exact (id={}, log_sn='{}')",
            table, table, m.id, log_sn_trimmed
        );
        return Ok(Some(m));
    }

    // 2) Identifier table for this equipment type. All three tables exist
    //    (mig 025 for aircraft_identifiers, mig 030 for controller / battery).
    //    Branching is upfront-validated by validate_table; the catch-all arm
    //    is unreachable.
    let identifier_match = match table {
        "aircraft" => {
            aircraft_identifiers::find_aircraft_by_identifier(pool, org_id, log_sn_trimmed)
                .await?
        }
        "remote_controllers" => {
            controller_identifiers::find_controller_by_identifier(pool, org_id, log_sn_trimmed)
                .await?
        }
        "batteries" => {
            battery_identifiers::find_battery_by_identifier(pool, org_id, log_sn_trimmed).await?
        }
        _ => None,
    };
    if let Some(m) = identifier_match {
        tracing::debug!(
            target: "sn_matching",
            "{} matched via identifier table (id={}, log_sn='{}')",
            table, m.id, log_sn_trimmed
        );
        return Ok(Some(m));
    }

    // 3) Prefix-tolerance fallback on the canonical serial_number column.
    classify_match_result(
        pool,
        table,
        org_id,
        log_sn_trimmed,
        find_by_serial(pool, table, org_id, log_sn_trimmed).await?,
    )
    .await
}

/// Exact-match-only lookup on a canonical `serial_number` column. Returns
/// an `EquipmentMatch` with `prefix_matched = false` when found. Used as
/// step 1 of `find_match` for all three equipment tables.
async fn find_exact(
    pool: &PgPool,
    table: &'static str,
    org_id: i32,
    log_sn: &str,
) -> Result<Option<EquipmentMatch>, AppError> {
    validate_table(table)?;
    let sql = format!(
        "SELECT id, equipment_id, serial_number FROM {} \
         WHERE org_id = $1 AND serial_number = $2 LIMIT 1",
        table
    );
    let row = sqlx::query(&sql)
        .bind(org_id)
        .bind(log_sn)
        .fetch_optional(pool)
        .await?;

    Ok(row.map(|r| {
        let id: i32 = r.get("id");
        let equipment_id: Option<String> = r.try_get("equipment_id").ok().flatten();
        let serial_number: String = r.get("serial_number");
        EquipmentMatch {
            id,
            equipment_id,
            serial_number,
            prefix_matched: false,
        }
    }))
}

/// Shared post-processing for `MatchResult` → `EquipmentMatch`. Used by
/// both the aircraft path (after exact + identifier-table miss) and
/// the battery/controller paths (where there is no identifier table).
async fn classify_match_result(
    pool: &PgPool,
    table: &'static str,
    org_id: i32,
    log_sn: &str,
    result: MatchResult,
) -> Result<Option<EquipmentMatch>, AppError> {
    match result {
        MatchResult::Exact { id } => {
            tracing::debug!(
                target: "sn_matching",
                "{} matched via {}.serial_number exact (id={}, log_sn='{}')",
                table, table, id, log_sn
            );
            let (equipment_id, serial_number) =
                lookup_canonical(pool, table, org_id, id).await?;
            Ok(Some(EquipmentMatch {
                id,
                equipment_id,
                serial_number,
                prefix_matched: false,
            }))
        }
        MatchResult::PrefixSingle { id, log_sn: log, db_sn } => {
            tracing::info!(
                target: "sn_matching",
                "{} prefix match: log_sn='{}' matched db_sn='{}' (id={})",
                table, log, db_sn, id
            );
            let (equipment_id, _canonical) =
                lookup_canonical(pool, table, org_id, id).await?;
            Ok(Some(EquipmentMatch {
                id,
                equipment_id,
                serial_number: db_sn,
                prefix_matched: true,
            }))
        }
        MatchResult::PrefixAmbiguous { count } => {
            tracing::warn!(
                target: "sn_matching",
                "{} '{}' has {} prefix candidates — refusing to guess, will auto-register",
                table, log_sn, count
            );
            Ok(None)
        }
        MatchResult::None => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_table_accepts_known_tables() {
        assert!(validate_table("aircraft").is_ok());
        assert!(validate_table("batteries").is_ok());
        assert!(validate_table("remote_controllers").is_ok());
    }

    #[test]
    fn validate_table_rejects_unknown_table() {
        let r = validate_table("flights");
        assert!(r.is_err());
        let r = validate_table("aircraft; DROP TABLE foo;--");
        assert!(r.is_err());
    }

    #[test]
    fn classify_exact_takes_precedence() {
        let r = classify(
            "ABC123",
            Some(42),
            vec![(99, "ABC1234".to_string())],
        );
        match r {
            MatchResult::Exact { id } => assert_eq!(id, 42),
            _ => panic!("expected Exact, got {:?}", r),
        }
    }

    #[test]
    fn classify_single_prefix_candidate_returns_prefix_single() {
        // Truncated log SN: 13 chars; DB has 14-char canonical SN.
        let r = classify(
            "093TEST000006",
            None,
            vec![(51, "093TEST0000069".to_string())],
        );
        match r {
            MatchResult::PrefixSingle { id, log_sn, db_sn } => {
                assert_eq!(id, 51);
                assert_eq!(log_sn, "093TEST000006");
                assert_eq!(db_sn, "093TEST0000069");
            }
            _ => panic!("expected PrefixSingle, got {:?}", r),
        }
    }

    #[test]
    fn classify_multiple_prefix_candidates_returns_ambiguous() {
        let r = classify(
            "PREFIX",
            None,
            vec![
                (1, "PREFIX01".to_string()),
                (2, "PREFIX02".to_string()),
            ],
        );
        match r {
            MatchResult::PrefixAmbiguous { count } => assert_eq!(count, 2),
            _ => panic!("expected PrefixAmbiguous, got {:?}", r),
        }
    }

    #[test]
    fn classify_no_candidates_returns_none() {
        let r = classify("NOMATCH", None, Vec::new());
        assert!(matches!(r, MatchResult::None));
    }

    // ---- find_match integration tests ----
    //
    // These hit a real Postgres via sqlx::test (each test gets its own
    // ephemeral DB seeded with the project's migrations, including the
    // default organisation row from 002_seed_reference_data.sql). The
    // tests cover the three states the import-preview UI now renders:
    // exact match, prefix match (dji-log-parser SN truncation), and
    // no match.

    async fn seed_aircraft(
        pool: &PgPool,
        org_id: i32,
        serial: &str,
        equipment_id: &str,
    ) -> i32 {
        let row = sqlx::query(
            "INSERT INTO aircraft (org_id, serial_number, equipment_id, status) \
             VALUES ($1, $2, $3, 'active') RETURNING id",
        )
        .bind(org_id)
        .bind(serial)
        .bind(equipment_id)
        .fetch_one(pool)
        .await
        .expect("aircraft insert");
        row.get::<i32, _>("id")
    }

    async fn default_org_id(pool: &PgPool) -> i32 {
        let row = sqlx::query("SELECT id FROM organisations ORDER BY id LIMIT 1")
            .fetch_one(pool)
            .await
            .expect("default org seeded");
        row.get::<i32, _>("id")
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn find_match_returns_exact_with_prefix_matched_false(pool: PgPool) {
        let org_id = default_org_id(&pool).await;
        let id = seed_aircraft(&pool, org_id, "08RTEST000003D", "UAV-T01").await;

        let m = find_match(&pool, "aircraft", org_id, "08RTEST000003D")
            .await
            .expect("query ok")
            .expect("expected Some");

        assert_eq!(m.id, id);
        assert_eq!(m.serial_number, "08RTEST000003D");
        assert_eq!(m.equipment_id.as_deref(), Some("UAV-T01"));
        assert!(!m.prefix_matched);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn find_match_returns_prefix_with_prefix_matched_true(pool: PgPool) {
        let org_id = default_org_id(&pool).await;
        let id = seed_aircraft(&pool, org_id, "08RTEST000003D", "UAV-T02").await;

        // log SN missing the trailing "D" — simulates dji-log-parser
        // truncation observed on Mavic Pro batteries.
        let m = find_match(&pool, "aircraft", org_id, "08RTEST000003")
            .await
            .expect("query ok")
            .expect("expected Some via prefix tolerance");

        assert_eq!(m.id, id);
        assert_eq!(m.serial_number, "08RTEST000003D");
        assert!(m.prefix_matched);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn find_match_returns_none_when_no_match(pool: PgPool) {
        let org_id = default_org_id(&pool).await;
        seed_aircraft(&pool, org_id, "08RTEST000003D", "UAV-T03").await;

        let m = find_match(&pool, "aircraft", org_id, "TOTALLY-DIFFERENT-SN")
            .await
            .expect("query ok");

        assert!(m.is_none(), "expected None for unrelated SN, got {:?}", m);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn find_match_returns_none_when_prefix_ambiguous(pool: PgPool) {
        let org_id = default_org_id(&pool).await;
        // Two SNs sharing a 13-char prefix — both within the 2-char
        // tolerance window for the truncated log SN. Ambiguous → None.
        seed_aircraft(&pool, org_id, "08RTEST000003D", "UAV-T04").await;
        seed_aircraft(&pool, org_id, "08RTEST000003X", "UAV-T05").await;

        let m = find_match(&pool, "aircraft", org_id, "08RTEST000003")
            .await
            .expect("query ok");

        assert!(m.is_none(), "expected None for ambiguous prefix, got {:?}", m);
    }

    /// Aircraft branch: identifier table hit takes precedence over prefix-tolerance.
    /// Real-world scenario: Inspire 1 body SN registered, log carries FC SN.
    #[sqlx::test(migrations = "./migrations")]
    async fn find_match_aircraft_identifier_path(pool: PgPool) {
        let org_id = default_org_id(&pool).await;
        let id = seed_aircraft(&pool, org_id, "W13TEST0000001", "UAV-049").await;
        sqlx::query(
            "INSERT INTO aircraft_identifiers (aircraft_id, org_id, identifier, source) \
             VALUES ($1, $2, '0410000001', 'fc_sn')",
        )
        .bind(id)
        .bind(org_id)
        .execute(&pool)
        .await
        .unwrap();

        let m = find_match(&pool, "aircraft", org_id, "0410000001")
            .await
            .unwrap()
            .expect("expected match via identifier table");
        assert_eq!(m.id, id);
        assert_eq!(m.serial_number, "W13TEST0000001");
        assert!(
            !m.prefix_matched,
            "identifier-table hit must report prefix_matched=false"
        );
    }

    /// Aircraft branch: exact match on serial_number still wins over identifier table.
    #[sqlx::test(migrations = "./migrations")]
    async fn find_match_aircraft_exact_serial_wins(pool: PgPool) {
        let org_id = default_org_id(&pool).await;
        let id = seed_aircraft(&pool, org_id, "CANONICAL_SN", "UAV-T06").await;
        // Ironic case: same string is also stored as identifier — exact-serial path should still win.
        sqlx::query(
            "INSERT INTO aircraft_identifiers (aircraft_id, org_id, identifier, source) \
             VALUES ($1, $2, 'CANONICAL_SN', 'other')",
        )
        .bind(id)
        .bind(org_id)
        .execute(&pool)
        .await
        .unwrap();

        let m = find_match(&pool, "aircraft", org_id, "CANONICAL_SN")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(m.id, id);
        assert!(!m.prefix_matched);
    }

    /// Cross-table leak guard: a battery lookup must consult only
    /// `battery_identifiers`, never `aircraft_identifiers` — even if the
    /// search string happens to exist as an aircraft identifier.
    #[sqlx::test(migrations = "./migrations")]
    async fn find_match_battery_does_not_leak_into_aircraft_identifiers(pool: PgPool) {
        let org_id = default_org_id(&pool).await;
        let aircraft_id =
            seed_aircraft(&pool, org_id, "AC_SN_FOR_BAT_TEST", "UAV-T07").await;
        sqlx::query(
            "INSERT INTO aircraft_identifiers (aircraft_id, org_id, identifier, source) \
             VALUES ($1, $2, 'BAT_SN', 'other')",
        )
        .bind(aircraft_id)
        .bind(org_id)
        .execute(&pool)
        .await
        .unwrap();

        let m = find_match(&pool, "batteries", org_id, "BAT_SN").await.unwrap();
        assert!(
            m.is_none(),
            "battery lookup must consult battery_identifiers, not aircraft_identifiers"
        );
    }

    /// Positive: battery branch resolves via battery_identifiers FC SN row.
    #[sqlx::test(migrations = "./migrations")]
    async fn find_match_battery_identifier_path(pool: PgPool) {
        let org_id = default_org_id(&pool).await;
        let bat_row = sqlx::query(
            "INSERT INTO batteries (org_id, serial_number, equipment_id) \
             VALUES ($1, '093TEST0000069', 'BAT-T01') RETURNING id",
        )
        .bind(org_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        let bat_id: i32 = bat_row.get("id");
        sqlx::query(
            "INSERT INTO battery_identifiers (battery_id, org_id, identifier, source) \
             VALUES ($1, $2, 'BAT_FC_SN_001', 'fc_sn')",
        )
        .bind(bat_id)
        .bind(org_id)
        .execute(&pool)
        .await
        .unwrap();

        let m = find_match(&pool, "batteries", org_id, "BAT_FC_SN_001")
            .await
            .unwrap()
            .expect("expected match via battery_identifiers");
        assert_eq!(m.id, bat_id);
        assert_eq!(m.serial_number, "093TEST0000069");
        assert!(
            !m.prefix_matched,
            "identifier-table hit must report prefix_matched=false"
        );
    }

    /// Positive: controller branch resolves via controller_identifiers FC SN row.
    /// Scenario seen in a production database: CTL-031 / 03L0000003 was the
    /// FC SN of the existing CTL-006 / 06FTEST0000002 — same physical
    /// hardware, two SNs.
    #[sqlx::test(migrations = "./migrations")]
    async fn find_match_controller_identifier_path(pool: PgPool) {
        let org_id = default_org_id(&pool).await;
        let ctl_row = sqlx::query(
            "INSERT INTO remote_controllers (org_id, serial_number, equipment_id) \
             VALUES ($1, '06FTEST0000002', 'CTL-006') RETURNING id",
        )
        .bind(org_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        let ctl_id: i32 = ctl_row.get("id");
        sqlx::query(
            "INSERT INTO controller_identifiers (controller_id, org_id, identifier, source) \
             VALUES ($1, $2, '03L0000003', 'fc_sn')",
        )
        .bind(ctl_id)
        .bind(org_id)
        .execute(&pool)
        .await
        .unwrap();

        let m = find_match(&pool, "remote_controllers", org_id, "03L0000003")
            .await
            .unwrap()
            .expect("expected match via controller_identifiers");
        assert_eq!(m.id, ctl_id);
        assert_eq!(m.serial_number, "06FTEST0000002");
        assert!(!m.prefix_matched);
    }

    /// Cross-table leak guard for controllers — symmetric to the battery leak guard.
    #[sqlx::test(migrations = "./migrations")]
    async fn find_match_controller_does_not_leak_into_aircraft_identifiers(pool: PgPool) {
        let org_id = default_org_id(&pool).await;
        let aircraft_id =
            seed_aircraft(&pool, org_id, "AC_SN_FOR_CTL_TEST", "UAV-T08").await;
        sqlx::query(
            "INSERT INTO aircraft_identifiers (aircraft_id, org_id, identifier, source) \
             VALUES ($1, $2, 'CTL_SN', 'other')",
        )
        .bind(aircraft_id)
        .bind(org_id)
        .execute(&pool)
        .await
        .unwrap();

        let m = find_match(&pool, "remote_controllers", org_id, "CTL_SN").await.unwrap();
        assert!(
            m.is_none(),
            "controller lookup must consult controller_identifiers, not aircraft_identifiers"
        );
    }

}
