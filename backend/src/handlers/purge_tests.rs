// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

//! Integration tests for the admin purge retention guard
//! (EASA UAS.SPEC.050(1)(g)). Mirrors the in-module `#[sqlx::test]`
//! scaffolding pattern from maintenance_tests.rs; wired into the binary
//! crate via `#[cfg(test)] #[path = ...] mod tests` in purge.rs.

use super::*;
use crate::services::retention;
use sqlx::PgPool;

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

fn admin_claims(org_id: i32, user_id: i32, user_code: &str) -> Claims {
    Claims {
        user_id,
        org_id,
        role: "admin".to_string(),
        roles: vec!["admin".to_string()],
        user_code: user_code.to_string(),
    }
}

/// Seed a flight whose flight_start is `years_ago` years in the past.
/// `deleted_at` set = soft-deleted.
async fn seed_flight(
    pool: &PgPool,
    org_id: i32,
    years_ago: i32,
    aircraft_id: Option<i32>,
    soft_deleted: bool,
) -> i32 {
    let row = sqlx::query(
        "INSERT INTO flights (org_id, flight_start, aircraft_id, deleted_at) \
         VALUES ($1, NOW() - make_interval(years => $2), $3, \
                 CASE WHEN $4 THEN NOW() ELSE NULL END) \
         RETURNING id",
    )
    .bind(org_id)
    .bind(years_ago)
    .bind(aircraft_id)
    .bind(soft_deleted)
    .fetch_one(pool)
    .await
    .expect("flight insert");
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

async fn flight_count(pool: &PgPool, org_id: i32) -> i64 {
    sqlx::query("SELECT COUNT(*) FROM flights WHERE org_id = $1")
        .bind(org_id)
        .fetch_one(pool)
        .await
        .expect("count query")
        .get::<i64, _>(0)
}

fn valid_confirmation(acknowledged: i64) -> Option<Json<PurgeConfirmation>> {
    Some(Json(PurgeConfirmation {
        acknowledged,
        reason: "Test data reset after import rehearsal".to_string(),
    }))
}

// ---- Tests ----

#[sqlx::test(migrations = "./migrations")]
async fn purge_flights_outside_window_succeeds_without_body(pool: PgPool) {
    let org_id = default_org_id(&pool).await;
    let (user_id, user_code) = default_admin_user(&pool).await;
    seed_flight(&pool, org_id, 4, None, false).await;
    seed_flight(&pool, org_id, 5, None, false).await;

    let claims = admin_claims(org_id, user_id, &user_code);
    let Json(result) = purge_flights(claims, State(pool.clone()), None)
        .await
        .expect("purge with only out-of-window rows must succeed without a body");

    assert_eq!(result.deleted, 2);
    assert_eq!(flight_count(&pool, org_id).await, 0);
}

#[sqlx::test(migrations = "./migrations")]
async fn purge_flights_within_window_no_body_returns_409_and_deletes_nothing(pool: PgPool) {
    let org_id = default_org_id(&pool).await;
    let (user_id, user_code) = default_admin_user(&pool).await;
    seed_flight(&pool, org_id, 1, None, false).await;
    seed_flight(&pool, org_id, 4, None, false).await;

    let claims = admin_claims(org_id, user_id, &user_code);
    let err = purge_flights(claims, State(pool.clone()), None)
        .await
        .expect_err("purge with retained rows and no body must be refused");

    match err {
        AppError::Conflict(m) => {
            assert!(m.contains("preflight"), "message should point at preflight: {m}");
        }
        other => panic!("expected Conflict, got {other:?}"),
    }
    assert_eq!(flight_count(&pool, org_id).await, 2, "nothing may be deleted");
}

#[sqlx::test(migrations = "./migrations")]
async fn purge_flights_wrong_acknowledged_returns_409_and_deletes_nothing(pool: PgPool) {
    let org_id = default_org_id(&pool).await;
    let (user_id, user_code) = default_admin_user(&pool).await;
    seed_flight(&pool, org_id, 1, None, false).await;

    let claims = admin_claims(org_id, user_id, &user_code);
    let err = purge_flights(claims, State(pool.clone()), valid_confirmation(999))
        .await
        .expect_err("mismatched acknowledged count must be refused");

    match err {
        AppError::Conflict(m) => {
            assert!(m.contains("mismatch"), "got: {m}");
            assert!(m.contains('1'), "message should state the current protected count: {m}");
        }
        other => panic!("expected Conflict, got {other:?}"),
    }
    assert_eq!(flight_count(&pool, org_id).await, 1, "nothing may be deleted");
}

#[sqlx::test(migrations = "./migrations")]
async fn purge_flights_short_reason_returns_400(pool: PgPool) {
    let org_id = default_org_id(&pool).await;
    let (user_id, user_code) = default_admin_user(&pool).await;
    seed_flight(&pool, org_id, 1, None, false).await;

    let claims = admin_claims(org_id, user_id, &user_code);
    let body = Some(Json(PurgeConfirmation {
        acknowledged: 1,
        reason: "  short  ".to_string(),
    }));
    let err = purge_flights(claims, State(pool.clone()), body)
        .await
        .expect_err("a sub-10-character reason must be refused");

    match err {
        AppError::BadRequest(m) => assert!(m.contains("10 characters"), "got: {m}"),
        other => panic!("expected BadRequest, got {other:?}"),
    }
    assert_eq!(flight_count(&pool, org_id).await, 1, "nothing may be deleted");
}

#[sqlx::test(migrations = "./migrations")]
async fn purge_flights_correct_confirmation_succeeds_and_writes_purge_audit(pool: PgPool) {
    let org_id = default_org_id(&pool).await;
    let (user_id, user_code) = default_admin_user(&pool).await;
    seed_flight(&pool, org_id, 1, None, false).await;
    seed_flight(&pool, org_id, 4, None, false).await;

    let claims = admin_claims(org_id, user_id, &user_code);
    let Json(result) = purge_flights(claims, State(pool.clone()), valid_confirmation(1))
        .await
        .expect("confirmed purge must succeed");

    assert_eq!(result.deleted, 2);
    assert_eq!(flight_count(&pool, org_id).await, 0);

    let audit_rows: Vec<(String, serde_json::Value)> = sqlx::query_as(
        "SELECT table_name, diff_jsonb FROM audit_log \
         WHERE org_id = $1 AND action = 'PURGE'",
    )
    .bind(org_id)
    .fetch_all(&pool)
    .await
    .expect("audit query");

    assert_eq!(audit_rows.len(), 1, "expected exactly one PURGE audit entry");
    let (table_name, diff) = &audit_rows[0];
    assert_eq!(table_name, "flights");
    let purge = diff.get("purge").expect("diff should carry a 'purge' object");
    assert_eq!(purge.get("rows_deleted").and_then(|v| v.as_i64()), Some(2));
    assert_eq!(purge.get("within_retention").and_then(|v| v.as_i64()), Some(1));
    assert_eq!(purge.get("degraded_records").and_then(|v| v.as_i64()), Some(0));
    assert_eq!(
        purge.get("reason").and_then(|v| v.as_str()),
        Some("Test data reset after import rehearsal")
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn purge_aircraft_with_retained_flight_reports_degraded_and_refuses(pool: PgPool) {
    let org_id = default_org_id(&pool).await;
    let (user_id, user_code) = default_admin_user(&pool).await;
    let aircraft_id = seed_aircraft(&pool, org_id, "TEST_AC_RETENTION").await;
    seed_flight(&pool, org_id, 1, Some(aircraft_id), false).await;

    let mut conn = pool.acquire().await.expect("acquire");
    let impact = retention::impact_for_target(&mut conn, org_id, "aircraft")
        .await
        .expect("impact");
    drop(conn);
    assert_eq!(impact.within_retention, 0);
    assert!(
        impact.degraded_records > 0,
        "retained flight referencing the aircraft must count as degraded"
    );

    let claims = admin_claims(org_id, user_id, &user_code);
    let err = purge_aircraft(claims, State(pool.clone()), None)
        .await
        .expect_err("purge_aircraft must refuse without confirmation");
    match err {
        AppError::Conflict(_) => {}
        other => panic!("expected Conflict, got {other:?}"),
    }

    let aircraft_left: i64 = sqlx::query("SELECT COUNT(*) FROM aircraft WHERE org_id = $1")
        .bind(org_id)
        .fetch_one(&pool)
        .await
        .expect("count")
        .get(0);
    assert!(aircraft_left >= 1, "aircraft must not be deleted");
    let linked: i64 = sqlx::query(
        "SELECT COUNT(*) FROM flights WHERE org_id = $1 AND aircraft_id IS NOT NULL",
    )
    .bind(org_id)
    .fetch_one(&pool)
    .await
    .expect("count")
    .get(0);
    assert_eq!(linked, 1, "flights.aircraft_id must not be nullified");
}

#[sqlx::test(migrations = "./migrations")]
async fn soft_deleted_flight_within_window_counts_toward_retention(pool: PgPool) {
    let org_id = default_org_id(&pool).await;
    let (user_id, user_code) = default_admin_user(&pool).await;
    seed_flight(&pool, org_id, 1, None, true).await; // soft-deleted, in window

    let mut conn = pool.acquire().await.expect("acquire");
    let impact = retention::impact_for_target(&mut conn, org_id, "flights")
        .await
        .expect("impact");
    drop(conn);
    assert_eq!(
        impact.within_retention, 1,
        "a soft-deleted flight is still a retained record"
    );

    let claims = admin_claims(org_id, user_id, &user_code);
    let err = purge_flights(claims, State(pool.clone()), None)
        .await
        .expect_err("soft-deleted retained flight must still block an unconfirmed purge");
    match err {
        AppError::Conflict(_) => {}
        other => panic!("expected Conflict, got {other:?}"),
    }
    assert_eq!(flight_count(&pool, org_id).await, 1);
}
