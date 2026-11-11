// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

//! Integration tests for the maintenance handler. First test file for
//! this module; mirrors the in-module `#[sqlx::test]` scaffolding pattern
//! introduced in PR #76 (handlers/flights/commit.rs::tests).
//!
//! Wired into the binary crate via `#[cfg(test)] #[path = ...] mod tests`
//! in maintenance.rs so it can use `super::*;` without needing a library
//! target.

use super::*;
use crate::auth::middleware::Claims;
use axum::extract::{Query, State};
use axum::Json;
use sqlx::{PgPool, Row};

// ---- Shared test seed helpers ----

async fn default_org_id(pool: &PgPool) -> i32 {
    let row = sqlx::query("SELECT id FROM organisations ORDER BY id LIMIT 1")
        .fetch_one(pool)
        .await
        .expect("default org seeded by migration 002");
    row.get::<i32, _>("id")
}

async fn default_admin_user(pool: &PgPool) -> (i32, String) {
    let row = sqlx::query(
        "SELECT id, user_code FROM users WHERE role = 'admin' ORDER BY id LIMIT 1",
    )
    .fetch_one(pool)
    .await
    .expect("default admin seeded by migration 002");
    (row.get::<i32, _>("id"), row.get::<String, _>("user_code"))
}

async fn seed_controller(pool: &PgPool, org_id: i32, serial: &str, model: &str) -> i32 {
    let row = sqlx::query(
        "INSERT INTO remote_controllers (org_id, serial_number, model, status) \
         VALUES ($1, $2, $3, 'active') RETURNING id",
    )
    .bind(org_id)
    .bind(serial)
    .bind(model)
    .fetch_one(pool)
    .await
    .expect("controller insert");
    row.get::<i32, _>("id")
}

async fn seed_aircraft(pool: &PgPool, org_id: i32, serial: &str) -> i32 {
    let row = sqlx::query(
        "INSERT INTO aircraft (org_id, serial_number, status) \
         VALUES ($1, $2, 'active') RETURNING id",
    )
    .bind(org_id)
    .bind(serial)
    .fetch_one(pool)
    .await
    .expect("aircraft insert");
    row.get::<i32, _>("id")
}

/// Construct a Claims with admin role for the seeded org/admin user. Tests
/// call the handler functions directly with this — same as the production
/// path, just without going through the axum extractor stack.
fn admin_claims(org_id: i32, user_id: i32, user_code: &str) -> Claims {
    Claims {
        user_id,
        org_id,
        role: "admin".to_string(),
        roles: vec!["admin".to_string()],
        user_code: user_code.to_string(),
    }
}

/// Distinct 3-letter codes for created_by + sign_off so the SoD check
/// (created_by != sign_off) passes. Returns (created_by, sign_off).
fn distinct_sod_codes() -> (String, String) {
    ("AAA".to_string(), "BBB".to_string())
}

fn base_request() -> CreateMaintenanceRequest {
    let (created_by, sign_off) = distinct_sod_codes();
    CreateMaintenanceRequest {
        aircraft_id: None,
        controller_id: None,
        performed_date: "2026-05-18".to_string(),
        flight_id: None,
        created_by,
        remark: "Internal battery replaced (test scenario)".to_string(),
        action_taken: "Replaced and verified per service manual".to_string(),
        maint_category: "Unscheduled".to_string(),
        maint_type: "Repair".to_string(),
        sign_off,
    }
}

// ---- Unit tests for validate_exactly_one_target ----

#[test]
fn validate_target_accepts_aircraft_only() {
    assert!(validate_exactly_one_target(Some(7), None).is_ok());
}

#[test]
fn validate_target_accepts_controller_only() {
    assert!(validate_exactly_one_target(None, Some(7)).is_ok());
}

#[test]
fn validate_target_rejects_both_set() {
    let err = validate_exactly_one_target(Some(1), Some(2)).unwrap_err();
    match err {
        AppError::BadRequest(m) => assert!(m.contains("not both"), "got: {m}"),
        other => panic!("expected BadRequest, got {other:?}"),
    }
}

#[test]
fn validate_target_rejects_neither_set() {
    let err = validate_exactly_one_target(None, None).unwrap_err();
    match err {
        AppError::BadRequest(m) => assert!(m.contains("exactly one of"), "got: {m}"),
        other => panic!("expected BadRequest, got {other:?}"),
    }
}

#[test]
fn validate_target_rejects_zero_aircraft_id() {
    // A historical client may have sent aircraft_id: 0 as a sentinel; reject.
    let err = validate_exactly_one_target(Some(0), None).unwrap_err();
    matches!(err, AppError::BadRequest(_));
}

// ---- Integration tests ----

#[sqlx::test(migrations = "./migrations")]
async fn test_create_maintenance_with_controller_id_succeeds(pool: PgPool) {
    let org_id = default_org_id(&pool).await;
    let (user_id, user_code) = default_admin_user(&pool).await;
    let controller_id =
        seed_controller(&pool, org_id, "TEST_CTL_SN_001", "DJI Matrice 600").await;

    let mut body = base_request();
    body.controller_id = Some(controller_id);

    let claims = admin_claims(org_id, user_id, &user_code);
    let Json(record) = create_maintenance(claims, State(pool.clone()), Json(body))
        .await
        .expect("create_maintenance should succeed");

    assert_eq!(record.controller_id, Some(controller_id));
    assert_eq!(record.aircraft_id, None);
    assert!(
        record.controller_name.is_some(),
        "controller_name should be populated via JOIN"
    );
    assert!(
        record.aircraft_name.is_none(),
        "aircraft_name should be None when aircraft_id is None"
    );
    // COALESCE precedence: rc.model wins because nickname is NULL.
    assert_eq!(
        record.controller_name.as_deref(),
        Some("DJI Matrice 600"),
        "controller_name should fall back to rc.model when nickname is NULL"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn test_create_maintenance_rejects_when_both_target_ids_set(pool: PgPool) {
    let org_id = default_org_id(&pool).await;
    let (user_id, user_code) = default_admin_user(&pool).await;
    let aircraft_id = seed_aircraft(&pool, org_id, "TEST_AC_SN_BOTH").await;
    let controller_id =
        seed_controller(&pool, org_id, "TEST_CTL_SN_BOTH", "DJI Mavic Air 2").await;

    let mut body = base_request();
    body.aircraft_id = Some(aircraft_id);
    body.controller_id = Some(controller_id);

    let claims = admin_claims(org_id, user_id, &user_code);
    let err = create_maintenance(claims, State(pool.clone()), Json(body))
        .await
        .expect_err("create_maintenance should reject both-set");

    match err {
        AppError::BadRequest(m) => assert!(
            m.contains("not both"),
            "expected 'not both' message, got: {m}"
        ),
        other => panic!("expected BadRequest, got {other:?}"),
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn test_create_maintenance_rejects_when_neither_target_id_set(pool: PgPool) {
    let org_id = default_org_id(&pool).await;
    let (user_id, user_code) = default_admin_user(&pool).await;

    let body = base_request(); // both fields default to None

    let claims = admin_claims(org_id, user_id, &user_code);
    let err = create_maintenance(claims, State(pool.clone()), Json(body))
        .await
        .expect_err("create_maintenance should reject neither-set");

    match err {
        AppError::BadRequest(m) => assert!(
            m.contains("exactly one of"),
            "expected 'exactly one of' message, got: {m}"
        ),
        other => panic!("expected BadRequest, got {other:?}"),
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn test_audit_log_records_controller_maintenance_creation(pool: PgPool) {
    let org_id = default_org_id(&pool).await;
    let (user_id, user_code) = default_admin_user(&pool).await;
    let controller_id =
        seed_controller(&pool, org_id, "TEST_CTL_SN_AUDIT", "DJI Inspire 1").await;

    let mut body = base_request();
    body.controller_id = Some(controller_id);

    let claims = admin_claims(org_id, user_id, &user_code);
    let Json(record) = create_maintenance(claims, State(pool.clone()), Json(body))
        .await
        .expect("create_maintenance should succeed");

    let audit_rows: Vec<(String, String, Option<i32>, serde_json::Value)> = sqlx::query_as(
        "SELECT action, table_name, record_id, diff_jsonb \
         FROM audit_log \
         WHERE org_id = $1 AND table_name = 'maintenance' AND record_id = $2",
    )
    .bind(org_id)
    .bind(record.id)
    .fetch_all(&pool)
    .await
    .expect("audit query");

    assert_eq!(audit_rows.len(), 1, "expected exactly one audit row");
    let (action, table_name, record_id, diff) = &audit_rows[0];
    assert_eq!(action, "INSERT");
    assert_eq!(table_name, "maintenance");
    assert_eq!(*record_id, Some(record.id));

    // diff_jsonb has shape { "after": { ...row fields... } }
    let after = diff
        .get("after")
        .expect("diff should have an 'after' key for INSERT");
    assert_eq!(
        after.get("controller_id").and_then(|v| v.as_i64()),
        Some(controller_id as i64),
        "audit diff should record controller_id"
    );
    assert_eq!(
        after.get("controller_name").and_then(|v| v.as_str()),
        Some("DJI Inspire 1"),
        "audit diff should record the joined controller_name"
    );
    assert!(
        after.get("aircraft_id").is_some_and(|v| v.is_null()),
        "audit diff should record aircraft_id as null"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn test_list_maintenance_filters_by_controller_id(pool: PgPool) {
    let org_id = default_org_id(&pool).await;
    let (user_id, user_code) = default_admin_user(&pool).await;
    let claims = admin_claims(org_id, user_id, &user_code);

    let aircraft_a = seed_aircraft(&pool, org_id, "TEST_AC_FILTER_A").await;
    let aircraft_b = seed_aircraft(&pool, org_id, "TEST_AC_FILTER_B").await;
    let controller_a =
        seed_controller(&pool, org_id, "TEST_CTL_FILTER_A", "DJI Matrice 600").await;
    let controller_b =
        seed_controller(&pool, org_id, "TEST_CTL_FILTER_B", "DJI Mavic Air 2").await;

    // 4 events: 2 per kind. SoD on this org may include pre-existing fixtures,
    // so we count *new* rows by their target id rather than asserting on the
    // grand total.
    for &(aircraft_id, controller_id) in &[
        (Some(aircraft_a), None),
        (Some(aircraft_a), None),
        (Some(aircraft_b), None),
        (None, Some(controller_a)),
        (None, Some(controller_a)),
        (None, Some(controller_b)),
    ] {
        let mut body = base_request();
        body.aircraft_id = aircraft_id;
        body.controller_id = controller_id;
        let _ = create_maintenance(claims.clone(), State(pool.clone()), Json(body))
            .await
            .expect("seed create_maintenance");
    }

    // Filter: controller_a → exactly the 2 controller_a events.
    let Json(filtered) = list_maintenance(
        claims.clone(),
        State(pool.clone()),
        Query(ListMaintenanceQuery {
            aircraft_id: None,
            controller_id: Some(controller_a),
        }),
    )
    .await
    .expect("list_maintenance with controller filter");
    assert_eq!(filtered.len(), 2, "expected 2 rows for controller_a");
    for row in &filtered {
        assert_eq!(row.controller_id, Some(controller_a));
        assert_eq!(row.aircraft_id, None);
    }

    // Filter: aircraft_a → exactly the 2 aircraft_a events.
    let Json(filtered) = list_maintenance(
        claims.clone(),
        State(pool.clone()),
        Query(ListMaintenanceQuery {
            aircraft_id: Some(aircraft_a),
            controller_id: None,
        }),
    )
    .await
    .expect("list_maintenance with aircraft filter");
    assert_eq!(filtered.len(), 2, "expected 2 rows for aircraft_a");
    for row in &filtered {
        assert_eq!(row.aircraft_id, Some(aircraft_a));
        assert_eq!(row.controller_id, None);
    }

    // No filter → at least all 6 seeded rows (may include fixtures).
    let Json(all_rows) = list_maintenance(
        claims,
        State(pool),
        Query(ListMaintenanceQuery {
            aircraft_id: None,
            controller_id: None,
        }),
    )
    .await
    .expect("list_maintenance no filter");
    let mine: Vec<_> = all_rows
        .iter()
        .filter(|r| {
            r.aircraft_id == Some(aircraft_a)
                || r.aircraft_id == Some(aircraft_b)
                || r.controller_id == Some(controller_a)
                || r.controller_id == Some(controller_b)
        })
        .collect();
    assert_eq!(mine.len(), 6, "expected 6 seeded rows in unfiltered list");
}

/// Optional fifth test: the loosened DB CHECK (<=1) must permit direct
/// inserts with both target ids NULL, because `ON DELETE SET NULL` on
/// either FK relies on this — retiring an aircraft must not block on
/// orphaning its historical maintenance rows. The handler still rejects
/// NULL/NULL on the API surface; this only exercises the DB layer.
#[sqlx::test(migrations = "./migrations")]
async fn test_db_check_allows_both_null_target_ids(pool: PgPool) {
    let org_id = default_org_id(&pool).await;

    let inserted_id: i32 = sqlx::query(
        "INSERT INTO maintenance (org_id, performed_date, created_by, sign_off, \
                                  remark, action_taken, maint_category, maint_type) \
         VALUES ($1, '2026-05-18'::date, 'AAA', 'BBB', \
                 'orphan row', 'orphan row', 'Unscheduled', 'Repair') \
         RETURNING id",
    )
    .bind(org_id)
    .fetch_one(&pool)
    .await
    .expect("DB CHECK should allow NULL/NULL target ids")
    .get("id");

    let (aircraft_id, controller_id): (Option<i32>, Option<i32>) = sqlx::query_as(
        "SELECT aircraft_id, controller_id FROM maintenance WHERE id = $1",
    )
    .bind(inserted_id)
    .fetch_one(&pool)
    .await
    .expect("row should be readable");

    assert_eq!(aircraft_id, None);
    assert_eq!(controller_id, None);
}
