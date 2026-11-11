// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

//! Tests for the aircraft_models handler — list / get_stats / bare get
//! permission boundaries plus aggregate correctness. Wired into the binary
//! crate via `#[cfg(test)] #[path = ...] mod tests` in aircraft_models.rs.
//!
//! Scaffolding mirrors the PR #76/#77 pattern (see commit.rs and
//! maintenance_tests.rs).

use super::*;
use crate::auth::middleware::Claims;
use axum::extract::{Path, Query, State};
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

fn admin_claims(org_id: i32, user_id: i32, user_code: &str) -> Claims {
    Claims {
        user_id,
        org_id,
        role: "admin".to_string(),
        roles: vec!["admin".to_string()],
        user_code: user_code.to_string(),
    }
}

/// A pilot-role Claims for the same default user. Useful for verifying
/// that view endpoints accept non-admin roles and that the bare CRUD `get`
/// continues to reject them.
fn pilot_claims(org_id: i32, user_id: i32, user_code: &str) -> Claims {
    Claims {
        user_id,
        org_id,
        role: "pilot".to_string(),
        roles: vec!["pilot".to_string()],
        user_code: user_code.to_string(),
    }
}

async fn seed_model(pool: &PgPool, org_id: i32, name: &str) -> i32 {
    let row = sqlx::query(
        "INSERT INTO aircraft_models (org_id, name) VALUES ($1, $2) RETURNING id",
    )
    .bind(org_id)
    .bind(name)
    .fetch_one(pool)
    .await
    .expect("aircraft_models insert");
    row.get::<i32, _>("id")
}

async fn seed_aircraft_with_stats(
    pool: &PgPool,
    org_id: i32,
    model_id: i32,
    serial: &str,
    equipment_id: &str,
    status: &str,
    total_flights: i32,
    total_seconds: i32,
) -> i32 {
    let row = sqlx::query(
        "INSERT INTO aircraft (org_id, serial_number, equipment_id, model_id, status, \
                               total_flights, total_seconds) \
         VALUES ($1, $2, $3, $4, $5, $6, $7) RETURNING id",
    )
    .bind(org_id)
    .bind(serial)
    .bind(equipment_id)
    .bind(model_id)
    .bind(status)
    .bind(total_flights)
    .bind(total_seconds)
    .fetch_one(pool)
    .await
    .expect("aircraft insert");
    row.get::<i32, _>("id")
}

/// Inserts a second organisation row so cross-org isolation can be tested.
/// Returns the new org id.
async fn seed_secondary_org(pool: &PgPool) -> i32 {
    let row = sqlx::query(
        "INSERT INTO organisations (name, slug) VALUES ($1, $2) RETURNING id",
    )
    .bind("Test Secondary Org")
    .bind(format!("test-org-{}", uuid::Uuid::new_v4()))
    .fetch_one(pool)
    .await
    .expect("secondary org insert");
    row.get::<i32, _>("id")
}

// ---- Tests ----

/// Aggregate counts and SUMs must reflect the seeded individuals — both
/// status counts (active / maintenance / retired) and the per-model
/// total_flights sum.
#[sqlx::test(migrations = "./migrations")]
async fn test_list_returns_aggregated_stats(pool: PgPool) {
    let org_id = default_org_id(&pool).await;
    let (user_id, user_code) = default_admin_user(&pool).await;

    let model_a = seed_model(&pool, org_id, "TestModel_StatsA").await;
    let model_b = seed_model(&pool, org_id, "TestModel_StatsB_NoAircraft").await;

    // Model A: 3 aircraft (2 active with 10 + 20 flights, 1 retired with 5).
    seed_aircraft_with_stats(
        &pool, org_id, model_a, "TSA_SN_1", "UAV-TSA1", "active", 10, 600,
    )
    .await;
    seed_aircraft_with_stats(
        &pool, org_id, model_a, "TSA_SN_2", "UAV-TSA2", "active", 20, 1200,
    )
    .await;
    seed_aircraft_with_stats(
        &pool, org_id, model_a, "TSA_SN_3", "UAV-TSA3", "retired", 5, 300,
    )
    .await;

    let Json(rows) = list(
        admin_claims(org_id, user_id, &user_code),
        State(pool.clone()),
        Query(ListQuery { is_active: None }),
    )
    .await
    .expect("list ok");

    let a = rows
        .iter()
        .find(|r| r.id == model_a)
        .expect("model_a in list");
    assert_eq!(a.aircraft_count, 3);
    assert_eq!(a.active_count, 2);
    assert_eq!(a.maintenance_count, 0);
    assert_eq!(a.retired_count, 1);
    assert_eq!(a.total_flights, 35);
    assert_eq!(a.total_flight_seconds, 2100);

    let b = rows
        .iter()
        .find(|r| r.id == model_b)
        .expect("model_b in list");
    assert_eq!(b.aircraft_count, 0);
    assert_eq!(b.total_flights, 0);
    assert_eq!(b.total_flight_seconds, 0);
    assert!(b.last_flight.is_none());
}

/// View endpoints must be reachable by non-admin roles — list is now
/// authenticated-any, not require_admin.
#[sqlx::test(migrations = "./migrations")]
async fn test_list_accessible_to_non_admin(pool: PgPool) {
    let org_id = default_org_id(&pool).await;
    let (user_id, user_code) = default_admin_user(&pool).await;

    let result = list(
        pilot_claims(org_id, user_id, &user_code),
        State(pool.clone()),
        Query(ListQuery { is_active: None }),
    )
    .await;

    assert!(
        result.is_ok(),
        "list must accept pilot role; got: {:?}",
        result.err()
    );
}

/// `/stats` returns aggregate row + individuals array of the right shape.
#[sqlx::test(migrations = "./migrations")]
async fn test_stats_endpoint_returns_individuals(pool: PgPool) {
    let org_id = default_org_id(&pool).await;
    let (user_id, user_code) = default_admin_user(&pool).await;

    let model_id = seed_model(&pool, org_id, "TestModel_StatsIndividuals").await;
    seed_aircraft_with_stats(
        &pool, org_id, model_id, "TSI_SN_1", "UAV-TSI1", "active", 7, 420,
    )
    .await;
    seed_aircraft_with_stats(
        &pool, org_id, model_id, "TSI_SN_2", "UAV-TSI2", "active", 3, 180,
    )
    .await;

    let Json(detail) = get_stats(
        admin_claims(org_id, user_id, &user_code),
        State(pool.clone()),
        Path(model_id),
    )
    .await
    .expect("get_stats ok");

    assert_eq!(detail.model.id, model_id);
    assert_eq!(detail.model.aircraft_count, 2);
    assert_eq!(detail.model.total_flights, 10);
    assert_eq!(detail.individuals.len(), 2);

    let eq_ids: Vec<&str> = detail
        .individuals
        .iter()
        .filter_map(|i| i.equipment_id.as_deref())
        .collect();
    assert!(eq_ids.contains(&"UAV-TSI1"));
    assert!(eq_ids.contains(&"UAV-TSI2"));
}

/// Unknown id → 404, not a panic or 500.
#[sqlx::test(migrations = "./migrations")]
async fn test_stats_endpoint_404_for_nonexistent_model(pool: PgPool) {
    let org_id = default_org_id(&pool).await;
    let (user_id, user_code) = default_admin_user(&pool).await;

    let err = get_stats(
        admin_claims(org_id, user_id, &user_code),
        State(pool.clone()),
        Path(999_999),
    )
    .await
    .expect_err("nonexistent model should 404");

    match err {
        AppError::NotFound(_) => {}
        other => panic!("expected NotFound, got {other:?}"),
    }
}

/// Cross-org probe → 404, not 403 — we don't leak that a model exists in
/// some other org.
#[sqlx::test(migrations = "./migrations")]
async fn test_stats_endpoint_404_for_other_org_model(pool: PgPool) {
    let org_id_1 = default_org_id(&pool).await;
    let (user_id, user_code) = default_admin_user(&pool).await;
    let org_id_2 = seed_secondary_org(&pool).await;

    let foreign_model_id = seed_model(&pool, org_id_2, "ForeignOrgModel").await;

    let err = get_stats(
        admin_claims(org_id_1, user_id, &user_code),
        State(pool.clone()),
        Path(foreign_model_id),
    )
    .await
    .expect_err("foreign-org model should 404");

    match err {
        AppError::NotFound(_) => {}
        other => panic!("expected NotFound (no existence leak), got {other:?}"),
    }
}

/// `/stats` is reachable by non-admin roles.
#[sqlx::test(migrations = "./migrations")]
async fn test_stats_endpoint_accessible_to_non_admin(pool: PgPool) {
    let org_id = default_org_id(&pool).await;
    let (user_id, user_code) = default_admin_user(&pool).await;

    let model_id = seed_model(&pool, org_id, "TestModel_StatsPilotAccess").await;

    let result = get_stats(
        pilot_claims(org_id, user_id, &user_code),
        State(pool.clone()),
        Path(model_id),
    )
    .await;

    assert!(
        result.is_ok(),
        "get_stats must accept pilot role; got: {:?}",
        result.err()
    );
}

/// The bare `GET /aircraft-models/:id` (used by ModelFormModal for editing)
/// must continue to require admin — only the new stats endpoints opened up.
#[sqlx::test(migrations = "./migrations")]
async fn test_bare_get_endpoint_still_admin_only(pool: PgPool) {
    let org_id = default_org_id(&pool).await;
    let (user_id, user_code) = default_admin_user(&pool).await;

    let model_id = seed_model(&pool, org_id, "TestModel_BareGetAdmin").await;

    let err = get(
        pilot_claims(org_id, user_id, &user_code),
        State(pool.clone()),
        Path(model_id),
    )
    .await
    .expect_err("bare get must reject pilot role");

    match err {
        AppError::Forbidden(_) => {}
        other => panic!("expected Forbidden, got {other:?}"),
    }
}
