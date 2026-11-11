// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

//! Integration tests for the aircraft-breakdowns endpoint added in PR #7
//! Commit 3. Covers happy path, not-found, org isolation, and zero-flight
//! aircraft. Also covers the `mounted` equipment block on
//! `get_aircraft_detail` (active-only, aircraft-scoped).

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

async fn seed_secondary_org_with_admin(pool: &PgPool) -> (i32, i32, String) {
    let org_row = sqlx::query(
        "INSERT INTO organisations (name, slug) VALUES ($1, $2) RETURNING id",
    )
    .bind("Aircraft Breakdowns Test Org 2")
    .bind(format!("ac-bd-org-{}", uuid::Uuid::new_v4()))
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

/// Insert a flight directly. Bypasses the commit handler; uses only the
/// columns the breakdown queries care about.
async fn seed_flight(
    pool: &PgPool,
    org_id: i32,
    aircraft_id: i32,
    easa_category: &str,
    mission_type: &str,
    duration_seconds: i32,
) {
    sqlx::query(
        "INSERT INTO flights \
           (org_id, aircraft_id, flight_start, duration_seconds, easa_category, mission_type, \
            home_lat, home_lon) \
         VALUES ($1, $2, NOW() - INTERVAL '1 day', $3, $4, $5, 59.33, 18.07)",
    )
    .bind(org_id)
    .bind(aircraft_id)
    .bind(duration_seconds)
    .bind(easa_category)
    .bind(mission_type)
    .execute(pool)
    .await
    .expect("flight insert");
}

/// Seed one accessory. `aircraft_id` is nullable to mirror the schema, but
/// these tests always mount it somewhere.
async fn seed_accessory(
    pool: &PgPool,
    org_id: i32,
    aircraft_id: i32,
    name: &str,
    status: &str,
) -> i32 {
    let row = sqlx::query(
        "INSERT INTO accessories (org_id, aircraft_id, name, status) \
         VALUES ($1, $2, $3, $4) RETURNING id",
    )
    .bind(org_id)
    .bind(aircraft_id)
    .bind(name)
    .bind(status)
    .fetch_one(pool)
    .await
    .expect("accessory insert");
    row.get::<i32, _>("id")
}

async fn seed_battery(
    pool: &PgPool,
    org_id: i32,
    aircraft_id: i32,
    serial: &str,
    status: &str,
) -> i32 {
    let row = sqlx::query(
        "INSERT INTO batteries (org_id, aircraft_id, serial_number, status) \
         VALUES ($1, $2, $3, $4) RETURNING id",
    )
    .bind(org_id)
    .bind(aircraft_id)
    .bind(serial)
    .bind(status)
    .fetch_one(pool)
    .await
    .expect("battery insert");
    row.get::<i32, _>("id")
}

/// Seed an aircraft model carrying the weight fields the takeoff-mass
/// computation reads (mtom_kg, empty_weight_kg — migration 049).
async fn seed_model_with_weights(
    pool: &PgPool,
    org_id: i32,
    name: &str,
    mtom_kg: Option<f32>,
    empty_weight_kg: Option<f32>,
) -> i32 {
    let row = sqlx::query(
        "INSERT INTO aircraft_models (org_id, name, mtom_kg, empty_weight_kg) \
         VALUES ($1, $2, $3, $4) RETURNING id",
    )
    .bind(org_id)
    .bind(name)
    .bind(mtom_kg)
    .bind(empty_weight_kg)
    .fetch_one(pool)
    .await
    .expect("aircraft_models insert");
    row.get::<i32, _>("id")
}

async fn set_aircraft_model(pool: &PgPool, aircraft_id: i32, model_id: i32) {
    sqlx::query("UPDATE aircraft SET model_id = $1 WHERE id = $2")
        .bind(model_id)
        .bind(aircraft_id)
        .execute(pool)
        .await
        .expect("aircraft model_id update");
}

async fn set_battery_weight(pool: &PgPool, battery_id: i32, weight_kg: Option<f32>) {
    sqlx::query("UPDATE batteries SET weight_kg = $1 WHERE id = $2")
        .bind(weight_kg)
        .bind(battery_id)
        .execute(pool)
        .await
        .expect("battery weight update");
}

async fn set_accessory_weight(pool: &PgPool, accessory_id: i32, weight_kg: Option<f32>) {
    sqlx::query("UPDATE accessories SET weight_kg = $1 WHERE id = $2")
        .bind(weight_kg)
        .bind(accessory_id)
        .execute(pool)
        .await
        .expect("accessory weight update");
}

async fn seed_controller(
    pool: &PgPool,
    org_id: i32,
    aircraft_id: i32,
    serial: &str,
    status: &str,
) -> i32 {
    let row = sqlx::query(
        "INSERT INTO remote_controllers (org_id, aircraft_id, serial_number, status) \
         VALUES ($1, $2, $3, $4) RETURNING id",
    )
    .bind(org_id)
    .bind(aircraft_id)
    .bind(serial)
    .bind(status)
    .fetch_one(pool)
    .await
    .expect("controller insert");
    row.get::<i32, _>("id")
}

#[sqlx::test(migrations = "./migrations")]
async fn test_detail_mounted_equipment_active_and_scoped(pool: PgPool) {
    let org_id = default_org_id(&pool).await;
    let (user_id, user_code) = default_admin_user(&pool).await;
    let aircraft_a = seed_aircraft(&pool, org_id, "TEST-MNT-A").await;
    let aircraft_b = seed_aircraft(&pool, org_id, "TEST-MNT-B").await;

    // Mounted on A: one active accessory, one active battery, one active
    // controller — these must all appear.
    let acc_a = seed_accessory(&pool, org_id, aircraft_a, "Zenmuse X5", "active").await;
    let bat_a = seed_battery(&pool, org_id, aircraft_a, "TEST-MNT-BAT-A", "active").await;
    let ctl_a = seed_controller(&pool, org_id, aircraft_a, "TEST-MNT-CTL-A", "active").await;

    // Noise: an accessory mounted on B, and a RETIRED battery still pointing
    // at A — neither may appear in A's mounted block.
    seed_accessory(&pool, org_id, aircraft_b, "Zenmuse X3", "active").await;
    seed_battery(&pool, org_id, aircraft_a, "TEST-MNT-BAT-RET", "retired").await;

    let Json(resp) = get_aircraft_detail(
        admin_claims(org_id, user_id, &user_code),
        State(pool.clone()),
        Path(aircraft_a),
    )
    .await
    .expect("detail ok");

    assert_eq!(resp.mounted.accessories.len(), 1, "exactly one active accessory on A");
    assert_eq!(resp.mounted.accessories[0].id, acc_a);
    assert_eq!(resp.mounted.accessories[0].name, "Zenmuse X5");

    assert_eq!(
        resp.mounted.batteries.len(),
        1,
        "retired battery must be excluded even though it points at A"
    );
    assert_eq!(resp.mounted.batteries[0].id, bat_a);
    assert_eq!(resp.mounted.batteries[0].serial_number, "TEST-MNT-BAT-A");

    assert_eq!(resp.mounted.controllers.len(), 1, "exactly one active controller on A");
    assert_eq!(resp.mounted.controllers[0].id, ctl_a);
    assert_eq!(resp.mounted.controllers[0].serial_number, "TEST-MNT-CTL-A");
}

#[sqlx::test(migrations = "./migrations")]
async fn test_detail_mounted_equipment_empty(pool: PgPool) {
    let org_id = default_org_id(&pool).await;
    let (user_id, user_code) = default_admin_user(&pool).await;
    let aircraft_id = seed_aircraft(&pool, org_id, "TEST-MNT-EMPTY").await;
    sqlx::query("UPDATE aircraft SET empty_weight_kg = 10.0 WHERE id = $1")
        .bind(aircraft_id)
        .execute(&pool)
        .await
        .expect("empty weight update");

    let Json(resp) = get_aircraft_detail(
        admin_claims(org_id, user_id, &user_code),
        State(pool.clone()),
        Path(aircraft_id),
    )
    .await
    .expect("detail ok");

    assert!(resp.mounted.accessories.is_empty());
    assert!(resp.mounted.batteries.is_empty());
    assert!(resp.mounted.controllers.is_empty());

    // Nothing mounted is a complete configuration: takeoff mass is the
    // empty weight alone, with zero missing-weight counts.
    let tm = &resp.takeoff_mass;
    assert_eq!(tm.total_kg, Some(10.0));
    assert_eq!(tm.batteries_kg, Some(0.0));
    assert_eq!(tm.accessories_kg, Some(0.0));
    assert_eq!(tm.batteries_missing_weight, 0);
    assert_eq!(tm.accessories_missing_weight, 0);
    assert!(!tm.empty_weight_missing);
}

#[sqlx::test(migrations = "./migrations")]
async fn test_breakdowns_returns_all_four_categories(pool: PgPool) {
    let org_id = default_org_id(&pool).await;
    let (user_id, user_code) = default_admin_user(&pool).await;
    let aircraft_id = seed_aircraft(&pool, org_id, "TEST-BD-ALL").await;

    // 2 Open A2 / Photo flights, 1 Specific SAIL-I / Survey flight.
    seed_flight(&pool, org_id, aircraft_id, "Open-A2", "Photo/Film", 600).await;
    seed_flight(&pool, org_id, aircraft_id, "Open-A2", "Photo/Film", 900).await;
    seed_flight(&pool, org_id, aircraft_id, "Specific-SAIL-I", "Survey", 1200).await;

    let Json(resp) = get_aircraft_breakdowns(
        admin_claims(org_id, user_id, &user_code),
        State(pool.clone()),
        Path(aircraft_id),
    )
    .await
    .expect("breakdowns ok");

    // EASA category: two buckets.
    assert_eq!(resp.by_easa_category.len(), 2);
    let easa_total: i64 = resp.by_easa_category.iter().map(|c| c.flights).sum();
    assert_eq!(easa_total, 3);

    // Mission type: two buckets.
    assert_eq!(resp.by_mission_type.len(), 2);
    let mission_total: i64 = resp.by_mission_type.iter().map(|c| c.flights).sum();
    assert_eq!(mission_total, 3);

    // Visual regime: 2 VLOS (Open-A2) + 1 BVLOS (Specific-SAIL-I).
    let vlos = resp
        .by_visual_regime
        .iter()
        .find(|c| c.label == "VLOS")
        .expect("VLOS bucket present");
    assert_eq!(vlos.flights, 2);
    let bvlos = resp
        .by_visual_regime
        .iter()
        .find(|c| c.label == "BVLOS")
        .expect("BVLOS bucket present");
    assert_eq!(bvlos.flights, 1);

    // Day/night: every test flight has home_lat/home_lon and flight_start,
    // so the Unknown bucket should be empty (filtered out). At least one
    // bucket must be non-empty given the three seeded flights.
    let dn_total: i64 = resp.by_daylight.iter().map(|c| c.flights).sum();
    assert!(dn_total >= 3, "day/night classification covers all flights");
    assert!(
        !resp.by_daylight.iter().any(|c| c.label == "Unknown"),
        "Unknown bucket must be filtered out when all flights have lat/lon"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn test_breakdowns_aircraft_not_found(pool: PgPool) {
    let org_id = default_org_id(&pool).await;
    let (user_id, user_code) = default_admin_user(&pool).await;

    let err = get_aircraft_breakdowns(
        admin_claims(org_id, user_id, &user_code),
        State(pool.clone()),
        Path(999_999),
    )
    .await
    .expect_err("nonexistent id must 404");

    assert!(matches!(err, AppError::NotFound(_)), "got {err:?}");
}

#[sqlx::test(migrations = "./migrations")]
async fn test_breakdowns_org_isolation(pool: PgPool) {
    let org1 = default_org_id(&pool).await;
    let (admin1_id, admin1_code) = default_admin_user(&pool).await;
    let ac_in_org1 = seed_aircraft(&pool, org1, "TEST-BD-ORG1").await;
    seed_flight(&pool, org1, ac_in_org1, "Open-A1", "Photo/Film", 300).await;

    let (org2, admin2_id, admin2_code) = seed_secondary_org_with_admin(&pool).await;

    let err = get_aircraft_breakdowns(
        admin_claims(org2, admin2_id, &admin2_code),
        State(pool.clone()),
        Path(ac_in_org1),
    )
    .await
    .expect_err("cross-org request must 404, not 403");

    assert!(matches!(err, AppError::NotFound(_)), "got {err:?}");

    // Sanity check: from org 1, the same id still resolves.
    let Json(resp) = get_aircraft_breakdowns(
        admin_claims(org1, admin1_id, &admin1_code),
        State(pool.clone()),
        Path(ac_in_org1),
    )
    .await
    .expect("same org must succeed");
    assert!(!resp.by_easa_category.is_empty());
}

// =====================================================================
// compute_takeoff_mass — pure unit tests (no DB).
// =====================================================================

fn battery(weight_kg: Option<f32>) -> MountedBattery {
    MountedBattery {
        id: 1,
        equipment_id: None,
        serial_number: "UNIT-BAT".to_string(),
        compatible_model: None,
        weight_kg,
        status: "active".to_string(),
    }
}

fn accessory(weight_kg: Option<f32>) -> MountedAccessory {
    MountedAccessory {
        id: 1,
        name: "Unit Payload".to_string(),
        category: None,
        serial_number: None,
        equipment_id: None,
        weight_kg,
        status: "active".to_string(),
    }
}

#[test]
fn takeoff_mass_complete_configuration_sums() {
    let batteries = vec![battery(Some(0.6)), battery(Some(0.59))];
    let accessories = vec![accessory(Some(0.83))];
    let tm = compute_takeoff_mass(Some(10.0), Some(9.5), Some(15.5), &batteries, &accessories);

    assert_eq!(tm.empty_weight_kg, Some(10.0));
    assert_eq!(tm.empty_weight_source.as_deref(), Some("aircraft"));
    assert!((tm.batteries_kg.unwrap() - 1.19).abs() < 1e-5);
    assert!((tm.accessories_kg.unwrap() - 0.83).abs() < 1e-5);
    assert!((tm.total_kg.unwrap() - 12.02).abs() < 1e-5);
    assert_eq!(tm.mtom_kg, Some(15.5));
    assert!(!tm.empty_weight_missing);
    assert_eq!(tm.batteries_missing_weight, 0);
    assert_eq!(tm.accessories_missing_weight, 0);
}

#[test]
fn takeoff_mass_falls_back_to_model_empty_weight() {
    let tm = compute_takeoff_mass(None, Some(9.5), None, &[], &[]);
    assert_eq!(tm.empty_weight_kg, Some(9.5));
    assert_eq!(tm.empty_weight_source.as_deref(), Some("model"));
    assert_eq!(tm.total_kg, Some(9.5));
}

#[test]
fn takeoff_mass_no_empty_weight_anywhere() {
    let tm = compute_takeoff_mass(None, None, Some(15.5), &[battery(Some(0.6))], &[]);
    assert_eq!(tm.empty_weight_kg, None);
    assert_eq!(tm.empty_weight_source, None);
    assert!(tm.empty_weight_missing);
    // Battery component is still complete on its own…
    assert_eq!(tm.batteries_kg, Some(0.6));
    // …but the total must never be emitted without the empty weight.
    assert_eq!(tm.total_kg, None);
}

#[test]
fn takeoff_mass_missing_battery_weight_blocks_total() {
    let batteries = vec![battery(Some(0.6)), battery(None)];
    let accessories = vec![accessory(Some(0.83))];
    let tm = compute_takeoff_mass(Some(10.0), None, Some(15.5), &batteries, &accessories);

    assert_eq!(tm.batteries_kg, None, "one weightless battery poisons the component");
    assert_eq!(tm.batteries_missing_weight, 1);
    assert_eq!(tm.accessories_kg, Some(0.83), "accessory component unaffected");
    assert_eq!(tm.accessories_missing_weight, 0);
    assert_eq!(tm.total_kg, None, "total never emitted with missing inputs");
}

#[test]
fn takeoff_mass_zero_mounted_items_is_complete() {
    let tm = compute_takeoff_mass(Some(10.0), None, Some(15.5), &[], &[]);
    // No mounted equipment is an accurate configuration, not missing data.
    assert_eq!(tm.batteries_kg, Some(0.0));
    assert_eq!(tm.accessories_kg, Some(0.0));
    assert_eq!(tm.total_kg, Some(10.0));
    assert_eq!(tm.batteries_missing_weight, 0);
    assert_eq!(tm.accessories_missing_weight, 0);
}

// =====================================================================
// takeoff_mass — DB integration tests on get_aircraft_detail.
// =====================================================================

#[sqlx::test(migrations = "./migrations")]
async fn test_detail_takeoff_mass_complete(pool: PgPool) {
    let org_id = default_org_id(&pool).await;
    let (user_id, user_code) = default_admin_user(&pool).await;
    let aircraft_id = seed_aircraft(&pool, org_id, "TEST-TOM-FULL").await;
    let model_id =
        seed_model_with_weights(&pool, org_id, "TOM Test Model", Some(15.5), Some(9.5)).await;
    set_aircraft_model(&pool, aircraft_id, model_id).await;

    let bat1 = seed_battery(&pool, org_id, aircraft_id, "TEST-TOM-BAT-1", "active").await;
    let bat2 = seed_battery(&pool, org_id, aircraft_id, "TEST-TOM-BAT-2", "active").await;
    set_battery_weight(&pool, bat1, Some(0.6)).await;
    set_battery_weight(&pool, bat2, Some(0.59)).await;
    let acc = seed_accessory(&pool, org_id, aircraft_id, "Zenmuse X5", "active").await;
    set_accessory_weight(&pool, acc, Some(0.83)).await;

    let Json(resp) = get_aircraft_detail(
        admin_claims(org_id, user_id, &user_code),
        State(pool.clone()),
        Path(aircraft_id),
    )
    .await
    .expect("detail ok");

    let tm = &resp.takeoff_mass;
    // No airframe-level empty weight recorded — model catalogue value applies.
    assert_eq!(tm.empty_weight_kg, Some(9.5));
    assert_eq!(tm.empty_weight_source.as_deref(), Some("model"));
    assert!((tm.batteries_kg.unwrap() - 1.19).abs() < 1e-5);
    assert!((tm.accessories_kg.unwrap() - 0.83).abs() < 1e-5);
    assert!((tm.total_kg.unwrap() - 11.52).abs() < 1e-5);
    assert_eq!(tm.mtom_kg, Some(15.5));
    assert_eq!(tm.batteries_missing_weight, 0);
    assert_eq!(tm.accessories_missing_weight, 0);
}

#[sqlx::test(migrations = "./migrations")]
async fn test_detail_takeoff_mass_null_battery_weight_blocks_total(pool: PgPool) {
    let org_id = default_org_id(&pool).await;
    let (user_id, user_code) = default_admin_user(&pool).await;
    let aircraft_id = seed_aircraft(&pool, org_id, "TEST-TOM-NULL").await;
    sqlx::query("UPDATE aircraft SET empty_weight_kg = 10.0 WHERE id = $1")
        .bind(aircraft_id)
        .execute(&pool)
        .await
        .expect("empty weight update");

    let bat1 = seed_battery(&pool, org_id, aircraft_id, "TEST-TOM-N-BAT-1", "active").await;
    set_battery_weight(&pool, bat1, Some(0.6)).await;
    // Second battery mounted with NO weight — must poison the total.
    seed_battery(&pool, org_id, aircraft_id, "TEST-TOM-N-BAT-2", "active").await;

    let Json(resp) = get_aircraft_detail(
        admin_claims(org_id, user_id, &user_code),
        State(pool.clone()),
        Path(aircraft_id),
    )
    .await
    .expect("detail ok");

    let tm = &resp.takeoff_mass;
    assert_eq!(tm.empty_weight_kg, Some(10.0));
    assert_eq!(tm.empty_weight_source.as_deref(), Some("aircraft"));
    assert_eq!(tm.batteries_kg, None);
    assert_eq!(tm.batteries_missing_weight, 1);
    assert_eq!(tm.total_kg, None, "total_kg must be null with a weightless mounted battery");
}

#[sqlx::test(migrations = "./migrations")]
async fn test_breakdowns_zero_flights(pool: PgPool) {
    let org_id = default_org_id(&pool).await;
    let (user_id, user_code) = default_admin_user(&pool).await;
    let aircraft_id = seed_aircraft(&pool, org_id, "TEST-BD-EMPTY").await;

    let Json(resp) = get_aircraft_breakdowns(
        admin_claims(org_id, user_id, &user_code),
        State(pool.clone()),
        Path(aircraft_id),
    )
    .await
    .expect("zero-flight aircraft must still succeed");

    assert!(resp.by_easa_category.is_empty());
    assert!(resp.by_mission_type.is_empty());
    // visual_regime breakdown doesn't filter NULLs at the SQL level — but
    // with zero rows in the source, it returns an empty Vec anyway.
    assert!(resp.by_visual_regime.is_empty());
    // by_daylight retains only non-zero buckets, so an aircraft with no
    // flights yields an empty vec.
    assert!(resp.by_daylight.is_empty());
}
