// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

//! Integration tests for the defects handler. Targets the
//! `?aircraft_id=N` filter added so AircraftDetailPage can render a
//! per-aircraft DefectHistory panel without pulling every defect
//! across the org.

use super::*;
use crate::auth::middleware::Claims;
use axum::extract::{Query, State};
use sqlx::{PgPool, Row};

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

async fn seed_defect(
    pool: &PgPool,
    org_id: i32,
    aircraft_id: i32,
    description: &str,
) -> i32 {
    let row = sqlx::query(
        "INSERT INTO defect_log (org_id, aircraft_id, defect_date, reported_by, description, status) \
         VALUES ($1, $2, CURRENT_DATE, 'LAR', $3, 'open') RETURNING id",
    )
    .bind(org_id)
    .bind(aircraft_id)
    .bind(description)
    .fetch_one(pool)
    .await
    .expect("defect insert");
    row.get::<i32, _>("id")
}

async fn seed_secondary_org(pool: &PgPool) -> (i32, i32, String) {
    let org_row = sqlx::query(
        "INSERT INTO organisations (name, slug) VALUES ($1, $2) RETURNING id",
    )
    .bind("Defects Test Org 2")
    .bind(format!("defects-org-{}", uuid::Uuid::new_v4()))
    .fetch_one(pool)
    .await
    .expect("secondary org");
    let org_id: i32 = org_row.get("id");

    let user_row = sqlx::query(
        "INSERT INTO users (org_id, name, email, user_code, password_hash, role) \
         VALUES ($1, $2, $3, $4, $5, 'admin') RETURNING id, user_code",
    )
    .bind(org_id)
    .bind("Admin Two")
    .bind(format!("admin2-{}@example.com", org_id))
    .bind(format!("A{:02}", org_id % 100))
    .bind("$2b$12$placeholder.hash.value.not.used.in.these.tests.xxxxxxxxxx")
    .fetch_one(pool)
    .await
    .expect("secondary admin");
    (
        org_id,
        user_row.get::<i32, _>("id"),
        user_row.get::<String, _>("user_code"),
    )
}

fn empty_query() -> Query<ListDefectsQuery> {
    Query(ListDefectsQuery { aircraft_id: None })
}

fn aircraft_query(id: i32) -> Query<ListDefectsQuery> {
    Query(ListDefectsQuery {
        aircraft_id: Some(id),
    })
}

#[sqlx::test(migrations = "./migrations")]
async fn test_list_defects_filters_by_aircraft_id(pool: PgPool) {
    let org_id = default_org_id(&pool).await;
    let (user_id, user_code) = default_admin_user(&pool).await;
    let ac1 = seed_aircraft(&pool, org_id, "TEST-DEF-AC1").await;
    let ac2 = seed_aircraft(&pool, org_id, "TEST-DEF-AC2").await;
    let _ = seed_defect(&pool, org_id, ac1, "defect on aircraft 1").await;
    let _ = seed_defect(&pool, org_id, ac2, "defect on aircraft 2").await;

    let Json(rows) = list_defects(
        admin_claims(org_id, user_id, &user_code),
        State(pool.clone()),
        aircraft_query(ac1),
    )
    .await
    .expect("list ok");

    assert_eq!(rows.len(), 1, "expected exactly one defect for aircraft {ac1}");
    assert_eq!(rows[0].aircraft_id, Some(ac1));
    assert_eq!(rows[0].description, "defect on aircraft 1");
}

#[sqlx::test(migrations = "./migrations")]
async fn test_list_defects_no_filter_returns_all(pool: PgPool) {
    let org_id = default_org_id(&pool).await;
    let (user_id, user_code) = default_admin_user(&pool).await;
    let ac1 = seed_aircraft(&pool, org_id, "TEST-DEF-NOFILT-1").await;
    let ac2 = seed_aircraft(&pool, org_id, "TEST-DEF-NOFILT-2").await;
    let _ = seed_defect(&pool, org_id, ac1, "first").await;
    let _ = seed_defect(&pool, org_id, ac2, "second").await;

    let Json(rows) = list_defects(
        admin_claims(org_id, user_id, &user_code),
        State(pool.clone()),
        empty_query(),
    )
    .await
    .expect("list ok");

    assert_eq!(rows.len(), 2, "expected both defects when no filter applied");
}

/// The controller_id filter requested in the original spec was dropped
/// because `defect_log` has no `controller_id` column. The third test
/// instead exercises the org-isolation boundary, which is the more
/// meaningful invariant on this endpoint.
#[sqlx::test(migrations = "./migrations")]
async fn test_list_defects_excludes_other_orgs(pool: PgPool) {
    let org1 = default_org_id(&pool).await;
    let (admin1_id, admin1_code) = default_admin_user(&pool).await;
    let ac1 = seed_aircraft(&pool, org1, "TEST-DEF-ORG1").await;
    let _ = seed_defect(&pool, org1, ac1, "org-1 defect").await;

    let (org2, _, _) = seed_secondary_org(&pool).await;
    let ac2 = seed_aircraft(&pool, org2, "TEST-DEF-ORG2").await;
    let _ = seed_defect(&pool, org2, ac2, "org-2 defect").await;

    let Json(rows) = list_defects(
        admin_claims(org1, admin1_id, &admin1_code),
        State(pool.clone()),
        empty_query(),
    )
    .await
    .expect("list ok");

    assert_eq!(rows.len(), 1, "org 1 must not see org 2's defects");
    assert_eq!(rows[0].description, "org-1 defect");
}
