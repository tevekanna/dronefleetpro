// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

//! Equipment ID generation for fleet equipment (aircraft, batteries,
//! remote controllers).
//!
//! Generates the next available `{prefix}NNN` identifier (e.g.
//! `UAV-001`, `BAT-007`, `CTL-012`) for the given org and table by
//! reading the current MAX from the DB, parsing the trailing number,
//! and adding one. Zero-padded to 3 digits with `:03` formatting,
//! which keeps DESC string-ordering correct up to NNN-999.
//!
//! **Not atomic**: this is a SELECT-then-INSERT pattern. Two concurrent
//! callers can pick the same `next_num` and both INSERT successfully
//! because the column has no UNIQUE constraint at the DB level. In the
//! current usage profile (single-operator clicking through one form at
//! a time, or one-shot batch imports) this is acceptable. If usage
//! patterns change, switch to a Postgres sequence + trigger or add a
//! UNIQUE constraint and retry on conflict.

use crate::db::DbPool;
use crate::errors::AppError;

/// Returns the next available `{prefix}NNN` equipment identifier for
/// the given org+table, e.g. `UAV-049`. Caller is responsible for
/// passing matching `(table, prefix)` pairs:
///   * `"aircraft"` ↔ `"UAV-"`
///   * `"batteries"` ↔ `"BAT-"`
///   * `"remote_controllers"` ↔ `"CTL-"`
///
/// The `table` parameter is interpolated into the query string. Callers
/// must ensure it is one of the three values above (no untrusted input).
pub(crate) async fn next_equipment_id(
    pool: &DbPool,
    org_id: i32,
    table: &str,
    prefix: &str,
) -> Result<String, AppError> {
    let query = format!(
        "SELECT equipment_id FROM {} WHERE org_id = $1 AND equipment_id LIKE $2 ORDER BY equipment_id DESC LIMIT 1",
        table
    );
    let pattern = format!("{}%", prefix);
    let row: Option<(Option<String>,)> = sqlx::query_as(&query)
        .bind(org_id)
        .bind(&pattern)
        .fetch_optional(pool)
        .await?;

    let next_num = match row {
        Some((Some(ref eid),)) => {
            let num_str = eid.trim_start_matches(prefix);
            num_str.parse::<u32>().unwrap_or(0) + 1
        }
        _ => 1,
    };

    Ok(format!("{}{:03}", prefix, next_num))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::PgPool;

    async fn default_org_id(pool: &PgPool) -> i32 {
        let row: (i32,) = sqlx::query_as("SELECT id FROM organisations ORDER BY id LIMIT 1")
            .fetch_one(pool)
            .await
            .expect("default org seeded");
        row.0
    }

    async fn seed_aircraft(pool: &PgPool, org_id: i32, sn: &str, eid: Option<&str>) {
        sqlx::query(
            "INSERT INTO aircraft (org_id, serial_number, equipment_id, status) \
             VALUES ($1, $2, $3, 'active')",
        )
        .bind(org_id)
        .bind(sn)
        .bind(eid)
        .execute(pool)
        .await
        .expect("aircraft insert");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn next_equipment_id_returns_001_on_empty_table(pool: PgPool) {
        let org_id = default_org_id(&pool).await;
        let id = next_equipment_id(&pool, org_id, "aircraft", "UAV-")
            .await
            .expect("query ok");
        assert_eq!(id, "UAV-001");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn next_equipment_id_increments_above_existing_max(pool: PgPool) {
        let org_id = default_org_id(&pool).await;
        seed_aircraft(&pool, org_id, "SN-A", Some("UAV-005")).await;
        seed_aircraft(&pool, org_id, "SN-B", Some("UAV-002")).await;

        let id = next_equipment_id(&pool, org_id, "aircraft", "UAV-")
            .await
            .expect("query ok");
        assert_eq!(id, "UAV-006", "should pick MAX+1, not just any+1");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn next_equipment_id_skips_null_existing(pool: PgPool) {
        // The bug this PR fixes: prior auto-registers left equipment_id NULL.
        // Generation must still pick a fresh UAV-NNN even when some rows
        // exist with NULL ids.
        let org_id = default_org_id(&pool).await;
        seed_aircraft(&pool, org_id, "SN-NULL-1", None).await;
        seed_aircraft(&pool, org_id, "SN-NULL-2", None).await;
        seed_aircraft(&pool, org_id, "SN-WITH", Some("UAV-007")).await;

        let id = next_equipment_id(&pool, org_id, "aircraft", "UAV-")
            .await
            .expect("query ok");
        assert_eq!(id, "UAV-008");
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn next_equipment_id_preserves_zero_padding_above_100(pool: PgPool) {
        // String DESC ordering only stays correct while numbers are
        // zero-padded to a fixed width. Sanity check that the formatter
        // produces UAV-100 rather than UAV-099 when MAX is UAV-099.
        let org_id = default_org_id(&pool).await;
        seed_aircraft(&pool, org_id, "SN-99", Some("UAV-099")).await;

        let id = next_equipment_id(&pool, org_id, "aircraft", "UAV-")
            .await
            .expect("query ok");
        assert_eq!(id, "UAV-100");
    }
}
