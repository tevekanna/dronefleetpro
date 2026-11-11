// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

//! Integration tests for the batteries handler — tri-state aircraft_id on
//! update: omitted keeps the current mount, explicit null clears it, a value
//! sets it.

use super::*;
use crate::auth::middleware::Claims;
use axum::extract::{Path, State};
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

async fn seed_battery(pool: &PgPool, org_id: i32, serial: &str, aircraft_id: i32) -> i32 {
    let row = sqlx::query(
        "INSERT INTO batteries (org_id, serial_number, aircraft_id, status) \
         VALUES ($1, $2, $3, 'active') RETURNING id",
    )
    .bind(org_id)
    .bind(serial)
    .bind(aircraft_id)
    .fetch_one(pool)
    .await
    .expect("battery insert");
    row.get::<i32, _>("id")
}

fn body(json: &str) -> UpdateBatteryRequest {
    serde_json::from_str(json).expect("valid update body")
}

#[sqlx::test(migrations = "./migrations")]
async fn test_update_battery_aircraft_id_tri_state(pool: PgPool) {
    let org_id = default_org_id(&pool).await;
    let (user_id, user_code) = default_admin_user(&pool).await;
    let aircraft1 = seed_aircraft(&pool, org_id, "TRI-BAT-AC-1").await;
    let aircraft2 = seed_aircraft(&pool, org_id, "TRI-BAT-AC-2").await;
    let battery_id = seed_battery(&pool, org_id, "TRI-BAT-1", aircraft1).await;

    // (b) omitted field keeps the current mount
    let Json(after) = update_battery(
        admin_claims(org_id, user_id, &user_code),
        State(pool.clone()),
        Path(battery_id),
        Json(body("{}")),
    )
    .await
    .expect("update with omitted aircraft_id ok");
    assert_eq!(after.aircraft_id, Some(aircraft1), "omitted must keep mount");

    // (a) explicit null clears the mount
    let Json(after) = update_battery(
        admin_claims(org_id, user_id, &user_code),
        State(pool.clone()),
        Path(battery_id),
        Json(body("{\"aircraft_id\": null}")),
    )
    .await
    .expect("update with explicit null ok");
    assert_eq!(after.aircraft_id, None, "explicit null must clear mount");

    // omitted again: stays NULL
    let Json(after) = update_battery(
        admin_claims(org_id, user_id, &user_code),
        State(pool.clone()),
        Path(battery_id),
        Json(body("{}")),
    )
    .await
    .expect("second omitted update ok");
    assert_eq!(after.aircraft_id, None, "omitted must not resurrect mount");

    // (c) a value sets the mount
    let Json(after) = update_battery(
        admin_claims(org_id, user_id, &user_code),
        State(pool.clone()),
        Path(battery_id),
        Json(body(&format!("{{\"aircraft_id\": {}}}", aircraft2))),
    )
    .await
    .expect("update with value ok");
    assert_eq!(after.aircraft_id, Some(aircraft2), "value must set mount");
}

async fn seed_aircraft_model(pool: &PgPool, org_id: i32, name: &str) -> i32 {
    let row = sqlx::query(
        "INSERT INTO aircraft_models (org_id, name) VALUES ($1, $2) RETURNING id",
    )
    .bind(org_id)
    .bind(name)
    .fetch_one(pool)
    .await
    .expect("aircraft model insert");
    row.get::<i32, _>("id")
}

#[sqlx::test(migrations = "./migrations")]
async fn test_update_battery_compatible_model_id_tri_state(pool: PgPool) {
    let org_id = default_org_id(&pool).await;
    let (user_id, user_code) = default_admin_user(&pool).await;
    let aircraft1 = seed_aircraft(&pool, org_id, "TRI-CMP-AC-1").await;
    let model1 = seed_aircraft_model(&pool, org_id, "Tri Compat Model 1").await;
    let model2 = seed_aircraft_model(&pool, org_id, "Tri Compat Model 2").await;
    let battery_id = seed_battery(&pool, org_id, "TRI-CMP-BAT-1", aircraft1).await;

    // Seed the structured compat directly (migration 052 column).
    sqlx::query("UPDATE batteries SET compatible_model_id = $1 WHERE id = $2")
        .bind(model1)
        .bind(battery_id)
        .execute(&pool)
        .await
        .expect("seed compatible_model_id");

    // (b) omitted field keeps the current value
    let Json(after) = update_battery(
        admin_claims(org_id, user_id, &user_code),
        State(pool.clone()),
        Path(battery_id),
        Json(body("{}")),
    )
    .await
    .expect("update with omitted compatible_model_id ok");
    assert_eq!(after.compatible_model_id, Some(model1), "omitted must keep value");

    // (a) explicit null clears it
    let Json(after) = update_battery(
        admin_claims(org_id, user_id, &user_code),
        State(pool.clone()),
        Path(battery_id),
        Json(body("{\"compatible_model_id\": null}")),
    )
    .await
    .expect("update with explicit null ok");
    assert_eq!(after.compatible_model_id, None, "explicit null must clear value");

    // omitted again: stays NULL
    let Json(after) = update_battery(
        admin_claims(org_id, user_id, &user_code),
        State(pool.clone()),
        Path(battery_id),
        Json(body("{}")),
    )
    .await
    .expect("second omitted update ok");
    assert_eq!(after.compatible_model_id, None, "omitted must not resurrect value");

    // (c) a value sets it
    let Json(after) = update_battery(
        admin_claims(org_id, user_id, &user_code),
        State(pool.clone()),
        Path(battery_id),
        Json(body(&format!("{{\"compatible_model_id\": {}}}", model2))),
    )
    .await
    .expect("update with value ok");
    assert_eq!(after.compatible_model_id, Some(model2), "value must set compat model");
}
