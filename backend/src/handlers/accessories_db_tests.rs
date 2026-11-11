// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

//! Integration tests for the accessories handler — tri-state aircraft_id on
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

async fn seed_accessory(pool: &PgPool, org_id: i32, name: &str, aircraft_id: i32) -> i32 {
    let row = sqlx::query(
        "INSERT INTO accessories (org_id, name, aircraft_id, status) \
         VALUES ($1, $2, $3, 'active') RETURNING id",
    )
    .bind(org_id)
    .bind(name)
    .bind(aircraft_id)
    .fetch_one(pool)
    .await
    .expect("accessory insert");
    row.get::<i32, _>("id")
}

fn body(json: &str) -> UpdateAccessoryRequest {
    serde_json::from_str(json).expect("valid update body")
}

#[sqlx::test(migrations = "./migrations")]
async fn test_update_accessory_aircraft_id_tri_state(pool: PgPool) {
    let org_id = default_org_id(&pool).await;
    let (user_id, user_code) = default_admin_user(&pool).await;
    let aircraft1 = seed_aircraft(&pool, org_id, "TRI-ACC-AC-1").await;
    let aircraft2 = seed_aircraft(&pool, org_id, "TRI-ACC-AC-2").await;
    let accessory_id = seed_accessory(&pool, org_id, "Tri-state gimbal", aircraft1).await;

    // (b) omitted field keeps the current mount
    let Json(after) = update_accessory(
        admin_claims(org_id, user_id, &user_code),
        State(pool.clone()),
        Path(accessory_id),
        Json(body("{}")),
    )
    .await
    .expect("update with omitted aircraft_id ok");
    assert_eq!(after.aircraft_id, Some(aircraft1), "omitted must keep mount");

    // (a) explicit null clears the mount
    let Json(after) = update_accessory(
        admin_claims(org_id, user_id, &user_code),
        State(pool.clone()),
        Path(accessory_id),
        Json(body("{\"aircraft_id\": null}")),
    )
    .await
    .expect("update with explicit null ok");
    assert_eq!(after.aircraft_id, None, "explicit null must clear mount");

    // omitted again: stays NULL
    let Json(after) = update_accessory(
        admin_claims(org_id, user_id, &user_code),
        State(pool.clone()),
        Path(accessory_id),
        Json(body("{}")),
    )
    .await
    .expect("second omitted update ok");
    assert_eq!(after.aircraft_id, None, "omitted must not resurrect mount");

    // (c) a value sets the mount
    let Json(after) = update_accessory(
        admin_claims(org_id, user_id, &user_code),
        State(pool.clone()),
        Path(accessory_id),
        Json(body(&format!("{{\"aircraft_id\": {}}}", aircraft2))),
    )
    .await
    .expect("update with value ok");
    assert_eq!(after.aircraft_id, Some(aircraft2), "value must set mount");
}
