// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

use serde::Serialize;
use serde_json::{Map, Value};
use sqlx::PgPool;

// =====================================================================
// Scrub list — fields and settings keys that must never hit diff_jsonb.
// Keep in sync with the project documentation.
// =====================================================================

/// Exact field names stripped from any audited row regardless of table.
const FIELD_SCRUB_EXACT: &[&str] = &["password_hash", "jwt_secret", "token_hash"];

/// Suffix patterns (case-insensitive) stripped from any audited row.
const FIELD_SCRUB_SUFFIXES: &[&str] = &["_token", "_secret"];

/// Exact field names — kept as a separate list so `api_key` matches exactly
/// (not just `*_key`), allowing `public_key` style fields to pass through.
const FIELD_SCRUB_EQUALS_CI: &[&str] = &["api_key"];

/// Exact `settings.key` values whose `value` column must be scrubbed.
const SETTINGS_SECRET_KEYS_EXACT: &[&str] = &["dji_api_key"];
const SETTINGS_SECRET_KEY_SUFFIXES: &[&str] = &["_key", "_secret", "_token"];
const SETTINGS_SECRET_KEY_CONTAINS: &[&str] = &["password"];

fn is_scrubbed_field(name: &str) -> bool {
    let lc = name.to_ascii_lowercase();
    if FIELD_SCRUB_EXACT.iter().any(|e| *e == lc) {
        return true;
    }
    if FIELD_SCRUB_EQUALS_CI.iter().any(|e| e.eq_ignore_ascii_case(&lc)) {
        return true;
    }
    FIELD_SCRUB_SUFFIXES.iter().any(|s| lc.ends_with(s))
}

pub fn is_secret_setting_key(key: &str) -> bool {
    let lc = key.to_ascii_lowercase();
    if SETTINGS_SECRET_KEYS_EXACT.iter().any(|e| *e == lc) {
        return true;
    }
    if SETTINGS_SECRET_KEY_SUFFIXES.iter().any(|s| lc.ends_with(s)) {
        return true;
    }
    SETTINGS_SECRET_KEY_CONTAINS.iter().any(|s| lc.contains(s))
}

fn scrub_object(mut obj: Map<String, Value>, table: &str) -> Map<String, Value> {
    obj.retain(|k, _| !is_scrubbed_field(k));
    if table == "settings" {
        let is_secret = obj
            .get("key")
            .and_then(Value::as_str)
            .map(is_secret_setting_key)
            .unwrap_or(false);
        if is_secret {
            obj.remove("value");
        }
    }
    obj
}

/// Serialise a row for audit storage and strip scrubbed fields.
/// Returns Value::Object; non-object inputs are returned unchanged.
pub fn serialise_for_audit<T: Serialize>(row: &T, table: &str) -> Value {
    let v = match serde_json::to_value(row) {
        Ok(v) => v,
        Err(_) => return Value::Object(Map::new()),
    };
    if let Value::Object(obj) = v {
        Value::Object(scrub_object(obj, table))
    } else {
        v
    }
}

/// Detect scrubbed fields that differ between before and after the scrub pass.
/// Used so that a change to `password_hash` (or similar) still surfaces as
/// `_scrubbed_changed: ["password_hash"]` in the diff — the fact of change is
/// preserved without ever storing the value.
fn scrubbed_field_changes(before: &Value, after: &Value, table: &str) -> Vec<String> {
    let b_obj = before.as_object();
    let a_obj = after.as_object();
    let (b_obj, a_obj) = match (b_obj, a_obj) {
        (Some(b), Some(a)) => (b, a),
        _ => return Vec::new(),
    };
    let mut changed: Vec<String> = Vec::new();
    // Field-name scrubs: any key that would be scrubbed AND differs between before/after.
    for k in b_obj.keys().chain(a_obj.keys()) {
        if changed.contains(k) {
            continue;
        }
        if is_scrubbed_field(k) {
            let bv = b_obj.get(k);
            let av = a_obj.get(k);
            if bv != av {
                changed.push(k.clone());
            }
        }
    }
    // Settings value-based scrub: if the key is secret and value differs, flag "value".
    if table == "settings" {
        let b_key = b_obj.get("key").and_then(Value::as_str);
        let a_key = a_obj.get("key").and_then(Value::as_str);
        let secret = [b_key, a_key]
            .into_iter()
            .flatten()
            .any(is_secret_setting_key);
        if secret {
            let b_val = b_obj.get("value");
            let a_val = a_obj.get("value");
            if b_val != a_val && !changed.iter().any(|k| k == "value") {
                changed.push("value".into());
            }
        }
    }
    changed.sort();
    changed
}

fn shallow_diff(before: &Value, after: &Value) -> (Map<String, Value>, Map<String, Value>) {
    let mut before_diff = Map::new();
    let mut after_diff = Map::new();
    let (b_obj, a_obj) = match (before.as_object(), after.as_object()) {
        (Some(b), Some(a)) => (b, a),
        _ => return (before_diff, after_diff),
    };
    for (k, av) in a_obj {
        if b_obj.get(k) != Some(av) {
            before_diff.insert(k.clone(), b_obj.get(k).cloned().unwrap_or(Value::Null));
            after_diff.insert(k.clone(), av.clone());
        }
    }
    for (k, bv) in b_obj {
        if !a_obj.contains_key(k) {
            before_diff.insert(k.clone(), bv.clone());
            after_diff.insert(k.clone(), Value::Null);
        }
    }
    (before_diff, after_diff)
}

async fn write_audit_row(
    pool: &PgPool,
    org_id: i32,
    user_id: i32,
    user_code: &str,
    action: &str,
    table: &str,
    record_id: i32,
    diff: Option<Value>,
) {
    // The originating client IP is resolved by the request middleware and
    // carried in a tokio task-local — see services::client_ip. Falls back
    // to NULL outside of a request (e.g. tests, background tasks).
    let ip_address = crate::services::client_ip::current_client_ip();

    let result = if let Some(d) = diff {
        sqlx::query(
            "INSERT INTO audit_log (org_id, user_id, user_code, action, table_name, record_id, diff_jsonb, ip_address) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
        )
        .bind(org_id)
        .bind(user_id)
        .bind(user_code)
        .bind(action)
        .bind(table)
        .bind(record_id)
        .bind(d)
        .bind(ip_address.as_deref())
        .execute(pool)
        .await
    } else {
        sqlx::query(
            "INSERT INTO audit_log (org_id, user_id, user_code, action, table_name, record_id, ip_address) \
             VALUES ($1,$2,$3,$4,$5,$6,$7)",
        )
        .bind(org_id)
        .bind(user_id)
        .bind(user_code)
        .bind(action)
        .bind(table)
        .bind(record_id)
        .bind(ip_address.as_deref())
        .execute(pool)
        .await
    };
    if let Err(e) = result {
        tracing::warn!("audit_log insert failed for {} {}: {:?}", action, table, e);
    }
}

// =====================================================================
// Audit helpers for INSERT, UPDATE, DELETE.
//
// Each accepts a serializable row and produces a structured diff_jsonb per the
// audit log contract. Use `log_update_diff` with before+after snapshots so the
// audit viewer can render "Changed fields" views — never pass empty diffs.
//
// Field scrubbing (password_hash, *_token, *_secret, api_key, jwt_secret, and
// settings.value for secret-pattern keys) is applied automatically.
// =====================================================================

pub async fn log_insert_full<T: Serialize>(
    pool: &PgPool,
    org_id: i32,
    user_id: i32,
    user_code: &str,
    table: &str,
    record_id: i32,
    row: &T,
) {
    let after = serialise_for_audit(row, table);
    let diff = serde_json::json!({ "after": after });
    write_audit_row(pool, org_id, user_id, user_code, "INSERT", table, record_id, Some(diff)).await;
}

pub async fn log_delete_full<T: Serialize>(
    pool: &PgPool,
    org_id: i32,
    user_id: i32,
    user_code: &str,
    table: &str,
    record_id: i32,
    row: &T,
) {
    let before = serialise_for_audit(row, table);
    let diff = serde_json::json!({ "before": before });
    write_audit_row(pool, org_id, user_id, user_code, "DELETE", table, record_id, Some(diff)).await;
}

pub async fn log_update_diff<T: Serialize>(
    pool: &PgPool,
    org_id: i32,
    user_id: i32,
    user_code: &str,
    table: &str,
    record_id: i32,
    before: &T,
    after: &T,
) {
    let raw_before = serde_json::to_value(before).unwrap_or(Value::Null);
    let raw_after = serde_json::to_value(after).unwrap_or(Value::Null);

    let scrubbed_changes = scrubbed_field_changes(&raw_before, &raw_after, table);

    let before_v = if let Value::Object(obj) = raw_before.clone() {
        Value::Object(scrub_object(obj, table))
    } else {
        raw_before
    };
    let after_v = if let Value::Object(obj) = raw_after.clone() {
        Value::Object(scrub_object(obj, table))
    } else {
        raw_after
    };

    let (mut before_diff, mut after_diff) = shallow_diff(&before_v, &after_v);

    // Settings-specific: always include the `key` column in both sides of the
    // diff, even if unchanged. Without this the audit viewer can't tell which
    // setting was modified when only its value changed.
    if table == "settings" {
        if let (Some(b), Some(a)) = (before_v.as_object(), after_v.as_object()) {
            if let Some(key) = a.get("key").or_else(|| b.get("key")).cloned() {
                before_diff.entry("key".to_string()).or_insert(key.clone());
                after_diff.entry("key".to_string()).or_insert(key);
            }
        }
    }

    if !scrubbed_changes.is_empty() {
        // Record the fact of change without storing the value. Frontend will
        // render this as a changed field named "_scrubbed_changed" whose
        // value is the list of redacted field names.
        before_diff.insert("_scrubbed_changed".into(), Value::Null);
        after_diff.insert(
            "_scrubbed_changed".into(),
            serde_json::json!(scrubbed_changes),
        );
    }

    let diff = serde_json::json!({
        "before": Value::Object(before_diff),
        "after": Value::Object(after_diff),
    });
    write_audit_row(pool, org_id, user_id, user_code, "UPDATE", table, record_id, Some(diff)).await;
}

/// Write an audit row with an arbitrary `action` name and a verbatim
/// `details` JSON payload. Intended for one-off action types that don't
/// fit the standard INSERT / UPDATE / DELETE shape — e.g.
/// `LINK_FC_SN_IDENTIFIER` and `FORCE_REGISTER_FC_SN_PRONE` written by
/// `handlers::flights::commit` when an operator overrides the default
/// refusal for FC-SN-prone aircraft models.
///
/// Failures are logged inside `write_audit_row` and not propagated —
/// returning `Result<(), AppError>` keeps the door open for stricter
/// callers later, but for now the value is always `Ok(())`.
pub async fn log_custom_action(
    pool: &PgPool,
    org_id: i32,
    user_id: i32,
    user_code: &str,
    action: &str,
    table: &str,
    record_id: i32,
    details: Value,
) -> Result<(), crate::errors::AppError> {
    write_audit_row(
        pool,
        org_id,
        user_id,
        user_code,
        action,
        table,
        record_id,
        Some(details),
    )
    .await;
    Ok(())
}

// =====================================================================
// Data export / import audit helpers.
//
// Both write to audit_log with table_name = 'data_export' or 'data_import'
// and record_id pointing to data_transfer_jobs.id. The summary is stored
// verbatim in diff_jsonb so the audit viewer can render counts.
//
// Note: record_id in audit_log is INTEGER (i32) but data_transfer_jobs.id
// is BIGSERIAL (i64). The cast `job_id as i32` is acceptable here — we are
// not going to exceed 2 billion export jobs per org. If concern arises
// later, audit_log.record_id can be widened in a separate migration.
// =====================================================================

/// Log a single audit entry summarising a bulk UPDATE that affects many rows.
/// Use this when per-row UPDATE auditing would produce thousands of entries
/// (e.g. nullifying flights.pilot_id when pilots are purged).
///
/// The diff_jsonb has shape:
///   { "bulk_update": { "column": "pilot_id", "set_to": null, "row_count": 247 } }
///
/// `record_id` is set to 0 to indicate "no specific record". Action is
/// "UPDATE" so the entry surfaces in normal action filters; the diff shape
/// distinguishes it from per-row updates.
pub async fn log_bulk_update(
    pool: &PgPool,
    org_id: i32,
    user_id: i32,
    user_code: &str,
    table_name: &str,
    column: &str,
    set_to: Value,
    row_count: i64,
) {
    let diff = serde_json::json!({
        "bulk_update": {
            "column": column,
            "set_to": set_to,
            "row_count": row_count,
        }
    });
    write_audit_row(
        pool, org_id, user_id, user_code, "UPDATE", table_name, 0, Some(diff),
    )
    .await;
}

/// Log a data export operation. record_id is the data_transfer_jobs.id.
/// Summary should contain entity counts as a JSON object, e.g.:
///   { "aircraft": 12, "pilots": 5, "flights": 247 }
pub async fn log_export(
    pool: &PgPool,
    org_id: i32,
    user_id: i32,
    user_code: &str,
    job_id: i64,
    summary: Value,
) {
    write_audit_row(
        pool,
        org_id,
        user_id,
        user_code,
        "EXPORT",
        "data_export",
        job_id as i32,
        Some(summary),
    )
    .await;
}

/// Log a data import operation. record_id is the data_transfer_jobs.id.
/// Summary should contain per-entity counts of created/updated/skipped/failed.
pub async fn log_import(
    pool: &PgPool,
    org_id: i32,
    user_id: i32,
    user_code: &str,
    job_id: i64,
    summary: Value,
) {
    write_audit_row(
        pool,
        org_id,
        user_id,
        user_code,
        "IMPORT",
        "data_import",
        job_id as i32,
        Some(summary),
    )
    .await;
}

// =====================================================================
// Authentication audit helper.
//
// Auth events (LOGIN_SUCCESS, LOGIN_FAILURE, LOGOUT, TOKEN_REDEEMED) differ
// from data events because some fields may legitimately be unknown — a login
// attempt with an unknown email has no user_id and no org_id. Accept Options
// so the row can carry NULLs where appropriate.
// =====================================================================

/// Write an authentication audit event. Accepts Options for org_id, user_id,
/// and record_id so failed logins on unknown emails can be recorded with NULL
/// tenant context. Failures to insert are logged but never bubbled up — auth
/// audit writes must not block the auth response.
pub async fn log_auth_event(
    pool: &PgPool,
    org_id: Option<i32>,
    user_id: Option<i32>,
    user_code: &str,
    action: &str,
    table_name: &str,
    record_id: Option<i32>,
    diff: Option<Value>,
) {
    let ip_address = crate::services::client_ip::current_client_ip();

    let result = sqlx::query(
        "INSERT INTO audit_log (org_id, user_id, user_code, action, table_name, record_id, diff_jsonb, ip_address) \
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
    )
    .bind(org_id)
    .bind(user_id)
    .bind(user_code)
    .bind(action)
    .bind(table_name)
    .bind(record_id)
    .bind(diff)
    .bind(ip_address.as_deref())
    .execute(pool)
    .await;

    if let Err(e) = result {
        tracing::warn!(
            "audit_log insert failed for auth event {} {}: {:?}",
            action,
            table_name,
            e
        );
    }
}

// =====================================================================
// Unit tests
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn password_hash_absent_from_insert_diff_after() {
        let user_row = json!({
            "id": 1,
            "name": "Alex",
            "email": "alex@example.com",
            "password_hash": "$2b$12$secretverysecret",
            "role": "admin",
        });
        let Value::Object(obj) = user_row else { panic!() };
        let cleaned = scrub_object(obj, "users");
        let out = Value::Object(cleaned).to_string();
        assert!(!out.contains("password_hash"), "got: {out}");
        assert!(!out.contains("$2b$12$"), "got: {out}");
    }

    #[test]
    fn token_hash_scrubbed_from_user_tokens() {
        let token_row = json!({
            "id": 5,
            "user_id": 7,
            "purpose": "invitation",
            "token_hash": "$2b$12$verySecretBcryptOfTheToken",
            "expires_at": "2026-05-01T00:00:00Z",
        });
        let Value::Object(obj) = token_row else { panic!() };
        let cleaned = scrub_object(obj, "user_tokens");
        let out = Value::Object(cleaned).to_string();
        assert!(!out.contains("token_hash"), "got: {out}");
        assert!(!out.contains("verySecretBcrypt"), "got: {out}");
    }

    #[test]
    fn settings_value_scrubbed_for_secret_keys() {
        let settings_row = json!({
            "org_id": 1,
            "key": "dji_api_key",
            "value": "super-secret-key-12345",
        });
        let Value::Object(obj) = settings_row else { panic!() };
        let cleaned = scrub_object(obj, "settings");
        assert_eq!(cleaned.get("key"), Some(&json!("dji_api_key")));
        assert!(cleaned.get("value").is_none(), "value must be absent, got: {:?}", cleaned.get("value"));
    }

    #[test]
    fn settings_value_preserved_for_non_secret_keys() {
        let settings_row = json!({
            "org_id": 1,
            "key": "max_flight_distance_m",
            "value": "5000",
        });
        let Value::Object(obj) = settings_row else { panic!() };
        let cleaned = scrub_object(obj, "settings");
        assert_eq!(cleaned.get("value"), Some(&json!("5000")),
            "non-secret settings value must be preserved");
    }

    #[test]
    fn password_only_change_produces_scrubbed_marker() {
        let before = json!({
            "id": 1, "email": "x@y.com",
            "password_hash": "$2b$12$OLD",
        });
        let after = json!({
            "id": 1, "email": "x@y.com",
            "password_hash": "$2b$12$NEW",
        });
        let changed = scrubbed_field_changes(&before, &after, "users");
        assert_eq!(changed, vec!["password_hash".to_string()]);

        // Full pipeline: the diff Value should reference "password_hash" by name
        // but never the hash value.
        let before_s = serialise_for_audit(&before, "users");
        let after_s = serialise_for_audit(&after, "users");
        let (mut b, mut a) = shallow_diff(&before_s, &after_s);
        if !changed.is_empty() {
            b.insert("_scrubbed_changed".into(), Value::Null);
            a.insert("_scrubbed_changed".into(), json!(changed));
        }
        let full = serde_json::json!({ "before": b, "after": a }).to_string();
        assert!(full.contains("password_hash"), "marker missing: {full}");
        assert!(!full.contains("$2b$12$OLD"), "before hash leaked: {full}");
        assert!(!full.contains("$2b$12$NEW"), "after hash leaked: {full}");
    }

    #[test]
    fn secret_suffix_scrubs_generic_fields() {
        // A row with `access_token` and `webhook_secret` — both must be stripped.
        let row = json!({
            "id": 1,
            "access_token": "abc",
            "webhook_secret": "xyz",
            "public_key": "visible",      // NOT scrubbed — field-name list is strict
        });
        let Value::Object(obj) = row else { panic!() };
        let cleaned = scrub_object(obj, "unknown_table");
        assert!(cleaned.get("access_token").is_none());
        assert!(cleaned.get("webhook_secret").is_none());
        assert_eq!(cleaned.get("public_key"), Some(&json!("visible")));
    }

    #[test]
    fn shallow_diff_only_returns_changed_keys() {
        let before = json!({ "a": 1, "b": "x", "c": true });
        let after = json!({ "a": 2, "b": "x", "c": false, "d": "new" });
        let (b, a) = shallow_diff(&before, &after);
        let mut keys: Vec<&String> = b.keys().collect();
        keys.sort();
        let mut keys_a: Vec<&String> = a.keys().collect();
        keys_a.sort();
        assert_eq!(keys, vec!["a", "c", "d"]);
        assert_eq!(keys_a, vec!["a", "c", "d"]);
        assert_eq!(b.get("d"), Some(&Value::Null));
        assert_eq!(a.get("d"), Some(&json!("new")));
    }
}
