// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

//! Data export logic — entity-group selection, dependency-ordered table
//! iteration, and JSON-document construction.
//!
//! See handlers/data_transfer.rs for the HTTP wiring.
//!
//! Canonical table list and `EntityGroup` live in
//! [`crate::services::data_transfer_schema`] so import shares the same
//! source of truth.

use serde_json::{Map, Value};
use sqlx::{PgPool, Row};

use crate::errors::AppError;
use crate::services::audit;
use crate::services::data_transfer_schema::CANONICAL_TABLES;

// Re-export so existing call sites (handlers/data_transfer.rs) keep
// working without churn — the canonical definition lives in
// data_transfer_schema.
pub use crate::services::data_transfer_schema::EntityGroup;

/// Build the `FROM … [JOIN …] WHERE …` fragment (target table aliased
/// `t`) shared by both `COUNT(*)` and `SELECT to_jsonb(t.*)`.
///
/// Most tables carry their own `org_id` and scope directly. Two do NOT
/// (migrations 036 / 037): `maintenance_checklist_items` and
/// `maintenance_checklist_completions` are org-scoped through a JOIN to
/// their org-bearing parent. `to_jsonb(t.*)` still emits only the target
/// table's columns, so the JOIN affects row selection but not payload.
fn from_clause(table: &str) -> String {
    match table {
        "flights" => "flights t WHERE t.org_id = $1 AND t.deleted_at IS NULL".to_string(),
        "maintenance_checklist_items" => "maintenance_checklist_items t \
             JOIN maintenance_instruction_templates mit ON t.template_id = mit.id \
             WHERE mit.org_id = $1"
            .to_string(),
        "maintenance_checklist_completions" => "maintenance_checklist_completions t \
             JOIN maintenance m ON t.maintenance_id = m.id \
             WHERE m.org_id = $1"
            .to_string(),
        _ => format!("{table} t WHERE t.org_id = $1"),
    }
}

// =====================================================================
// count_entities
// =====================================================================

/// Per-table counts of rows that would be exported. Returned as a
/// `serde_json::Map` so insertion order (= dependency order) is
/// preserved in the response (requires `serde_json/preserve_order`).
pub async fn count_entities(
    pool: &PgPool,
    org_id: i32,
    groups: &[EntityGroup],
) -> Result<Map<String, Value>, AppError> {
    let mut out = Map::new();

    for (table, group) in CANONICAL_TABLES {
        if !groups.contains(group) {
            continue;
        }
        let sql = format!(
            "SELECT COUNT(*)::BIGINT AS n FROM {}",
            from_clause(table)
        );
        let row = sqlx::query(&sql).bind(org_id).fetch_one(pool).await?;
        let n: i64 = row.try_get("n")?;
        out.insert((*table).to_string(), Value::from(n));
    }

    Ok(out)
}

// =====================================================================
// build_export
// =====================================================================

/// Produce the full export document and a per-table counts map.
///
/// Memory note: a single in-memory document holds every row from every
/// selected table. For orgs with very large `flights.trajectory_json`
/// payloads or long `audit_log` history, the document can reach tens or
/// hundreds of megabytes. Chunked / streaming export is a future
/// improvement.
pub async fn build_export(
    pool: &PgPool,
    org_id: i32,
    groups: &[EntityGroup],
) -> Result<(Value, Map<String, Value>), AppError> {
    let mut tables_out: Map<String, Value> = Map::new();
    let mut counts: Map<String, Value> = Map::new();

    for (table, group) in CANONICAL_TABLES {
        if !groups.contains(group) {
            continue;
        }

        let sql = format!(
            "SELECT to_jsonb(t.*) AS row FROM {}",
            from_clause(table)
        );
        let rows = sqlx::query(&sql).bind(org_id).fetch_all(pool).await?;

        let mut arr: Vec<Value> = Vec::with_capacity(rows.len());
        for r in rows {
            let v: Value = r.try_get("row")?;
            let v = if *table == "settings" { scrub_settings_row(v) } else { v };
            arr.push(v);
        }

        counts.insert((*table).to_string(), Value::from(arr.len() as i64));
        tables_out.insert((*table).to_string(), Value::Array(arr));
    }

    let doc = Value::Object(Map::from_iter([
        ("format_version".to_string(), Value::from(1)),
        (
            "exported_at".to_string(),
            Value::from(chrono::Utc::now().to_rfc3339()),
        ),
        ("source_org_id".to_string(), Value::from(org_id)),
        (
            "entity_groups".to_string(),
            Value::Array(groups.iter().map(|g| Value::from(g.as_str())).collect()),
        ),
        ("tables".to_string(), Value::Object(tables_out)),
    ]));

    Ok((doc, counts))
}

/// Replace `value` with `***SCRUBBED***` for rows whose `key` matches the
/// secret-setting pattern. Reuses [`audit::is_secret_setting_key`] to
/// keep the rule in one place.
fn scrub_settings_row(v: Value) -> Value {
    let Value::Object(mut obj) = v else { return v };
    let is_secret = obj
        .get("key")
        .and_then(Value::as_str)
        .map(audit::is_secret_setting_key)
        .unwrap_or(false);
    if is_secret {
        obj.insert("value".to_string(), Value::from("***SCRUBBED***"));
    }
    Value::Object(obj)
}

// =====================================================================
// Estimated size heuristic
// =====================================================================

/// Crude byte estimate used by the preview endpoint. Real size is
/// dominated by JSONB columns (trajectories, audit diffs) which we can't
/// estimate without sampling. 1 KB per row is a deliberate undercount
/// for trajectory-heavy orgs and an overcount for tiny reference rows;
/// both errors trend toward "the user sees a magnitude, not a precision".
pub fn estimate_size_bytes(counts: &Map<String, Value>) -> i64 {
    counts
        .values()
        .filter_map(|v| v.as_i64())
        .sum::<i64>()
        .saturating_mul(1024)
}

// =====================================================================
// Tests
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_secret_value_is_scrubbed() {
        let row = serde_json::json!({
            "org_id": 1, "key": "dji_api_key", "value": "real-secret-value-123",
        });
        let out = scrub_settings_row(row);
        assert_eq!(out["value"], serde_json::json!("***SCRUBBED***"));
        assert_eq!(out["key"], serde_json::json!("dji_api_key"));
    }

    #[test]
    fn settings_non_secret_value_preserved() {
        let row = serde_json::json!({
            "org_id": 1, "key": "max_flight_distance_m", "value": "5000",
        });
        let out = scrub_settings_row(row);
        assert_eq!(out["value"], serde_json::json!("5000"));
    }
}
