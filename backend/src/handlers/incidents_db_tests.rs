// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

//! Integration tests for occurrence classification and the 72-hour
//! reporting clock on the incidents handlers (migration 054).

use super::*;
use crate::auth::middleware::Claims;
use crate::models::incident::CreateIncidentRequest;
use axum::extract::{Path, State};
use axum::Json;
use chrono::{DateTime, Duration, Utc};
use sqlx::PgPool;

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

/// Seed an aircraft model with the given design_approval_required value
/// (None = not determined).
async fn seed_model(pool: &PgPool, org_id: i32, name: &str, dar: Option<&str>) -> i32 {
    let row = sqlx::query(
        "INSERT INTO aircraft_models (org_id, name, design_approval_required) \
         VALUES ($1, $2, $3) RETURNING id",
    )
    .bind(org_id)
    .bind(name)
    .bind(dar)
    .fetch_one(pool)
    .await
    .expect("model insert");
    row.get::<i32, _>("id")
}

async fn seed_aircraft(pool: &PgPool, org_id: i32, serial: &str, model_id: Option<i32>) -> i32 {
    let row = sqlx::query(
        "INSERT INTO aircraft (org_id, serial_number, status, model_id) \
         VALUES ($1, $2, 'active', $3) RETURNING id",
    )
    .bind(org_id)
    .bind(serial)
    .bind(model_id)
    .fetch_one(pool)
    .await
    .expect("aircraft insert");
    row.get::<i32, _>("id")
}

fn create_req(
    aircraft_id: Option<i32>,
    occurrence_code: Option<&str>,
    became_aware_at: Option<DateTime<Utc>>,
    reported_at: Option<DateTime<Utc>>,
) -> CreateIncidentRequest {
    CreateIncidentRequest {
        occurred_at: "2026-07-01T12:00:00Z".to_string(),
        severity: "medium".to_string(),
        category: "technical".to_string(),
        description: "occurrence classification test incident".to_string(),
        flight_id: None,
        pilot_id: None,
        aircraft_id,
        location_description: None,
        immediate_action: None,
        reported_to_authority: None,
        occurrence_code: occurrence_code.map(str::to_string),
        became_aware_at,
        reported_at,
        reporting_delay_reason: None,
    }
}

/// Create an incident through the handler and return the detail response
/// (incident + computed reporting status).
async fn create_and_get(
    pool: &PgPool,
    claims: &Claims,
    req: CreateIncidentRequest,
) -> IncidentDetailResponse {
    let Json(created) = create_incident(claims.clone(), State(pool.clone()), Json(req))
        .await
        .expect("create ok");
    let Json(detail) = get_incident(claims.clone(), State(pool.clone()), Path(created.id))
        .await
        .expect("get ok");
    detail
}

#[sqlx::test(migrations = "./migrations")]
async fn test_create_incident_with_valid_code(pool: PgPool) {
    let org_id = default_org_id(&pool).await;
    let (user_id, user_code) = default_admin_user(&pool).await;
    let claims = admin_claims(org_id, user_id, &user_code);

    let Json(created) = create_incident(
        claims,
        State(pool.clone()),
        Json(create_req(None, Some("VI.1.4.1"), None, None)),
    )
    .await
    .expect("valid Annex VI code must be accepted");

    assert_eq!(created.occurrence_code.as_deref(), Some("VI.1.4.1"));
}

#[sqlx::test(migrations = "./migrations")]
async fn test_create_incident_rejects_invalid_code(pool: PgPool) {
    let org_id = default_org_id(&pool).await;
    let (user_id, user_code) = default_admin_user(&pool).await;
    let claims = admin_claims(org_id, user_id, &user_code);

    let err = create_incident(
        claims,
        State(pool.clone()),
        Json(create_req(None, Some("VI.9.99"), None, None)),
    )
    .await
    .expect_err("invalid code must be rejected");

    match err {
        AppError::BadRequest(msg) => {
            assert!(msg.contains("VI.9.99"), "error must name the invalid code: {msg}");
        }
        other => panic!("expected BadRequest, got {other:?}"),
    }
}

#[sqlx::test(migrations = "./migrations")]
async fn test_not_required_model_yields_not_applicable_without_deadline(pool: PgPool) {
    let org_id = default_org_id(&pool).await;
    let (user_id, user_code) = default_admin_user(&pool).await;
    let claims = admin_claims(org_id, user_id, &user_code);
    let model = seed_model(&pool, org_id, "OCC-NotReq Model", Some("not_required")).await;
    let ac = seed_aircraft(&pool, org_id, "OCC-NOTREQ-1", Some(model)).await;

    // Even classified, aware and long past 72h: no obligation, no deadline.
    let detail = create_and_get(
        &pool,
        &claims,
        create_req(
            Some(ac),
            Some("VI.4.1"),
            Some(Utc::now() - Duration::hours(200)),
            None,
        ),
    )
    .await;

    assert_eq!(detail.reporting.state, "not_applicable");
    assert_eq!(detail.reporting.applicable, Some(false));
    assert_eq!(detail.reporting.deadline_at, None);
    assert_eq!(detail.reporting.hours_remaining, None);
}

#[sqlx::test(migrations = "./migrations")]
async fn test_required_model_within_window_is_due(pool: PgPool) {
    let org_id = default_org_id(&pool).await;
    let (user_id, user_code) = default_admin_user(&pool).await;
    let claims = admin_claims(org_id, user_id, &user_code);
    let model = seed_model(&pool, org_id, "OCC-Req Model Due", Some("required")).await;
    let ac = seed_aircraft(&pool, org_id, "OCC-DUE-1", Some(model)).await;

    let aware = Utc::now() - Duration::hours(1);
    let detail = create_and_get(
        &pool,
        &claims,
        create_req(Some(ac), Some("VI.4.1"), Some(aware), None),
    )
    .await;

    assert_eq!(detail.reporting.state, "due");
    assert_eq!(detail.reporting.applicable, Some(true));
    // Compare against the DB-roundtripped timestamp — PostgreSQL stores
    // TIMESTAMPTZ at microsecond precision, Utc::now() has nanoseconds.
    let stored_aware = detail.incident.became_aware_at.expect("aware stored");
    assert_eq!(
        detail.reporting.deadline_at,
        Some(stored_aware + Duration::hours(72))
    );
    assert!(detail.reporting.hours_remaining.is_some());
}

#[sqlx::test(migrations = "./migrations")]
async fn test_required_model_past_window_is_overdue(pool: PgPool) {
    let org_id = default_org_id(&pool).await;
    let (user_id, user_code) = default_admin_user(&pool).await;
    let claims = admin_claims(org_id, user_id, &user_code);
    let model = seed_model(&pool, org_id, "OCC-Req Model Overdue", Some("required")).await;
    let ac = seed_aircraft(&pool, org_id, "OCC-OVERDUE-1", Some(model)).await;

    let aware = Utc::now() - Duration::hours(73);
    let detail = create_and_get(
        &pool,
        &claims,
        create_req(Some(ac), Some("VI.4.1"), Some(aware), None),
    )
    .await;

    assert_eq!(detail.reporting.state, "overdue");
    let stored_aware = detail.incident.became_aware_at.expect("aware stored");
    assert_eq!(
        detail.reporting.deadline_at,
        Some(stored_aware + Duration::hours(72))
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn test_reported_at_yields_reported(pool: PgPool) {
    let org_id = default_org_id(&pool).await;
    let (user_id, user_code) = default_admin_user(&pool).await;
    let claims = admin_claims(org_id, user_id, &user_code);
    let model = seed_model(&pool, org_id, "OCC-Req Model Reported", Some("required")).await;
    let ac = seed_aircraft(&pool, org_id, "OCC-REPORTED-1", Some(model)).await;

    let aware = Utc::now() - Duration::hours(100);
    let detail = create_and_get(
        &pool,
        &claims,
        create_req(
            Some(ac),
            Some("VI.4.1"),
            Some(aware),
            Some(aware + Duration::hours(10)),
        ),
    )
    .await;

    assert_eq!(detail.reporting.state, "reported");
    assert_eq!(detail.reporting.hours_remaining, None);
}

#[sqlx::test(migrations = "./migrations")]
async fn test_aircraft_without_model_is_undetermined(pool: PgPool) {
    let org_id = default_org_id(&pool).await;
    let (user_id, user_code) = default_admin_user(&pool).await;
    let claims = admin_claims(org_id, user_id, &user_code);
    let ac = seed_aircraft(&pool, org_id, "OCC-NOMODEL-1", None).await;

    let detail = create_and_get(
        &pool,
        &claims,
        create_req(
            Some(ac),
            Some("VI.4.1"),
            Some(Utc::now() - Duration::hours(200)),
            None,
        ),
    )
    .await;

    assert_eq!(detail.reporting.state, "undetermined");
    assert_eq!(detail.reporting.applicable, None);
    assert_eq!(
        detail.reporting.deadline_at, None,
        "a deadline must never be shown while applicability is undetermined"
    );
}

#[sqlx::test(migrations = "./migrations")]
async fn test_list_incidents_carries_reporting_status(pool: PgPool) {
    let org_id = default_org_id(&pool).await;
    let (user_id, user_code) = default_admin_user(&pool).await;
    let claims = admin_claims(org_id, user_id, &user_code);
    let model = seed_model(&pool, org_id, "OCC-List Model", Some("required")).await;
    let ac = seed_aircraft(&pool, org_id, "OCC-LIST-1", Some(model)).await;

    // Applicable but unclassified: state is "unclassified" and, critically,
    // no deadline even though became_aware_at is set.
    let Json(created) = create_incident(
        claims.clone(),
        State(pool.clone()),
        Json(create_req(Some(ac), None, Some(Utc::now()), None)),
    )
    .await
    .expect("create ok");

    let Json(items) = list_incidents(claims, State(pool.clone()))
        .await
        .expect("list ok");
    let item = items
        .iter()
        .find(|i| i.incident.id == created.id)
        .expect("created incident present in list");

    assert_eq!(item.reporting.state, "unclassified");
    assert_eq!(item.reporting.applicable, Some(true));
    assert_eq!(item.reporting.deadline_at, None);
}
