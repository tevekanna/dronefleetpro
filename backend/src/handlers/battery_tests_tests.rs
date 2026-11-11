// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

//! Integration tests for the battery_tests handler — the append-only
//! battery health/capacity log surfaced on BatteryStatsPage.

use super::*;
use crate::auth::middleware::Claims;
use axum::extract::{Path, State};
use crate::models::battery_test::CreateBatteryTestRequest;
use chrono::NaiveDate;
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

async fn seed_battery(pool: &PgPool, org_id: i32, serial: &str) -> i32 {
    let row = sqlx::query(
        "INSERT INTO batteries (org_id, serial_number, status) \
         VALUES ($1, $2, 'active') RETURNING id",
    )
    .bind(org_id)
    .bind(serial)
    .fetch_one(pool)
    .await
    .expect("battery insert");
    row.get::<i32, _>("id")
}

async fn seed_secondary_org(pool: &PgPool) -> i32 {
    let org_row = sqlx::query(
        "INSERT INTO organisations (name, slug) VALUES ($1, $2) RETURNING id",
    )
    .bind("Battery Tests Org 2")
    .bind(format!("bt-org-{}", uuid::Uuid::new_v4()))
    .fetch_one(pool)
    .await
    .expect("secondary org");
    org_row.get::<i32, _>("id")
}

fn sample_request() -> CreateBatteryTestRequest {
    CreateBatteryTestRequest {
        test_date: NaiveDate::from_ymd_opt(2026, 5, 29).unwrap(),
        result: "pass".to_string(),
        swelling_status: Some("minor".to_string()),
        capacity_measured: Some(58.5),
        internal_resistance: Some(12.0),
        cell_voltages: Some(vec![4.18, 4.17, 4.19, 4.16]),
        notes: Some("routine check".to_string()),
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn test_create_and_list_battery_test(pool: PgPool) {
    let org_id = default_org_id(&pool).await;
    let (user_id, user_code) = default_admin_user(&pool).await;
    let battery_id = seed_battery(&pool, org_id, "TEST-BT-CREATE").await;

    let Json(created) = create_battery_test(
        admin_claims(org_id, user_id, &user_code),
        State(pool.clone()),
        Path(battery_id),
        Json(sample_request()),
    )
    .await
    .expect("create ok");

    assert_eq!(created.battery_id, battery_id);
    assert_eq!(created.result, "pass");
    assert_eq!(created.swelling_status, "minor");
    assert_eq!(created.test_type, "health", "test_type is server-set");
    assert_eq!(
        created.tested_by.as_deref(),
        Some(user_code.as_str()),
        "tested_by is server-set from claims.user_code"
    );

    let Json(rows) = list_battery_tests(
        admin_claims(org_id, user_id, &user_code),
        State(pool.clone()),
        Path(battery_id),
    )
    .await
    .expect("list ok");

    assert_eq!(rows.len(), 1, "expected exactly one test for the battery");
    assert_eq!(rows[0].id, created.id);
}

#[sqlx::test(migrations = "./migrations")]
async fn test_create_rejects_invalid_result(pool: PgPool) {
    let org_id = default_org_id(&pool).await;
    let (user_id, user_code) = default_admin_user(&pool).await;
    let battery_id = seed_battery(&pool, org_id, "TEST-BT-BADRESULT").await;

    let mut req = sample_request();
    req.result = "bogus".to_string();

    let err = create_battery_test(
        admin_claims(org_id, user_id, &user_code),
        State(pool.clone()),
        Path(battery_id),
        Json(req),
    )
    .await
    .expect_err("invalid result must be rejected");

    assert!(matches!(err, AppError::BadRequest(_)), "got: {err:?}");
}

#[sqlx::test(migrations = "./migrations")]
async fn test_battery_in_another_org_returns_404(pool: PgPool) {
    let org1 = default_org_id(&pool).await;
    let (admin1_id, admin1_code) = default_admin_user(&pool).await;

    let org2 = seed_secondary_org(&pool).await;
    let other_battery = seed_battery(&pool, org2, "TEST-BT-OTHERORG").await;

    let err = list_battery_tests(
        admin_claims(org1, admin1_id, &admin1_code),
        State(pool.clone()),
        Path(other_battery),
    )
    .await
    .expect_err("battery in another org must 404");

    assert!(matches!(err, AppError::NotFound(_)), "got: {err:?}");
}
