// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

//! Tests for the users handler — the single-role update path (PR 1 of the
//! multi-role work). Wired into the binary crate via
//! `#[cfg(test)] #[path = "users_tests.rs"] mod tests;` in users.rs.
//!
//! Focus: a PUT carrying the single `role` field (what the current
//! single-select Edit User modal sends) must REPLACE the user's entire
//! `user_roles` set with that one role AND keep the denormalised `users.role`
//! mirror in sync, while the lockout guards (last active admin, own admin
//! role) still hold and the change is audited with the `roles` array.
//!
//! Scaffolding mirrors the aircraft_models_tests.rs / maintenance_tests.rs
//! pattern: `#[sqlx::test(migrations = "./migrations")]` gives each test a
//! fresh DB with the seed org + admin (migration 002) and the user_roles
//! backfill (migration 045).

use super::*;
use crate::auth::middleware::Claims;
use crate::errors::AppError;
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

/// The seeded ADM admin (migration 002) — the org's only admin, with a
/// backfilled `user_roles` row (migration 045).
async fn default_admin_user(pool: &PgPool) -> (i32, String) {
    let row = sqlx::query("SELECT id, user_code FROM users WHERE role = 'admin' ORDER BY id LIMIT 1")
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

/// Insert a user carrying a single `role`, plus the matching `user_roles`
/// row (as migration 045's backfill would). Returns the new user id.
async fn seed_user(
    pool: &PgPool,
    org_id: i32,
    name: &str,
    email: &str,
    user_code: &str,
    role: &str,
) -> i32 {
    let row = sqlx::query(
        "INSERT INTO users (org_id, name, email, user_code, password_hash, role, is_active) \
         VALUES ($1, $2, $3, $4, 'placeholder', $5, TRUE) RETURNING id",
    )
    .bind(org_id)
    .bind(name)
    .bind(email)
    .bind(user_code)
    .bind(role)
    .fetch_one(pool)
    .await
    .expect("seed user insert");
    let id = row.get::<i32, _>("id");
    sqlx::query("INSERT INTO user_roles (org_id, user_id, role) VALUES ($1, $2, $3)")
        .bind(org_id)
        .bind(id)
        .bind(role)
        .execute(pool)
        .await
        .expect("seed user_roles insert");
    id
}

fn single_role_body(role: &str) -> UpdateUserRequest {
    UpdateUserRequest {
        name: None,
        email: None,
        user_code: None,
        role: Some(role.to_string()),
        roles: None,
        pilot_id: None,
    }
}

// ---- (a) single-role update replaces the set + mirror + list reflects it ----

#[sqlx::test(migrations = "./migrations")]
async fn test_single_role_update_replaces_set_and_mirror(pool: PgPool) {
    let org = default_org_id(&pool).await;
    let (admin_id, admin_code) = default_admin_user(&pool).await;
    let target = seed_user(&pool, org, "Vic Viewer", "vic@example.test", "VIC", "viewer").await;

    let resp = update(
        admin_claims(org, admin_id, &admin_code),
        State(pool.clone()),
        Path(target),
        Json(single_role_body("technician")),
    )
    .await
    .expect("single-role update should succeed")
    .0;

    // Response reflects the new role in both the mirror and the array.
    assert_eq!(resp.role, "technician");
    assert_eq!(resp.roles, vec!["technician".to_string()]);

    // users.role denormalised mirror updated.
    let mirror: (String,) = sqlx::query_as("SELECT role FROM users WHERE id = $1")
        .bind(target)
        .fetch_one(&pool)
        .await
        .expect("mirror role");
    assert_eq!(mirror.0, "technician");

    // user_roles set REPLACED — exactly one row, the new role (no stale viewer).
    let roles: Vec<(String,)> =
        sqlx::query_as("SELECT role FROM user_roles WHERE user_id = $1 ORDER BY role")
            .bind(target)
            .fetch_all(&pool)
            .await
            .expect("user_roles rows");
    assert_eq!(roles.len(), 1, "role set must be fully replaced, got {roles:?}");
    assert_eq!(roles[0].0, "technician");

    // List endpoint (the refetch the modal triggers) shows the new role.
    let listed = list(
        admin_claims(org, admin_id, &admin_code),
        State(pool.clone()),
        Query(ListQuery { include_inactive: false }),
    )
    .await
    .expect("list should succeed")
    .0;
    let found = listed
        .iter()
        .find(|u| u.id == target)
        .expect("target user present in list");
    assert_eq!(found.role, "technician");
    assert_eq!(found.roles, vec!["technician".to_string()]);
}

// ---- (b) last active admin cannot be demoted ----

#[sqlx::test(migrations = "./migrations")]
async fn test_last_active_admin_role_demotion_refused(pool: PgPool) {
    let org = default_org_id(&pool).await;
    let (admin_id, admin_code) = default_admin_user(&pool).await;

    // ADM is the org's ONLY admin. A different admin actor (crafted claims,
    // id != target) attempting to strip ADM's admin role must be refused by
    // the last-admin guard. A non-target actor id keeps the self-guard from
    // shadowing the last-admin guard we want to exercise here.
    let actor = admin_claims(org, admin_id + 100_000, &admin_code);
    let err = update(
        actor,
        State(pool.clone()),
        Path(admin_id),
        Json(single_role_body("viewer")),
    )
    .await
    .expect_err("demoting the last active admin must fail");

    match err {
        AppError::Conflict(msg) => {
            assert!(
                msg.to_lowercase().contains("last active admin"),
                "expected last-admin conflict, got: {msg}"
            );
        }
        other => panic!("expected Conflict, got {other:?}"),
    }

    // ADM is untouched in both the mirror and the join table.
    let mirror: (String,) = sqlx::query_as("SELECT role FROM users WHERE id = $1")
        .bind(admin_id)
        .fetch_one(&pool)
        .await
        .expect("mirror role");
    assert_eq!(mirror.0, "admin");
    let admin_rows: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM user_roles WHERE user_id = $1 AND role = 'admin'")
            .bind(admin_id)
            .fetch_one(&pool)
            .await
            .expect("count admin rows");
    assert_eq!(admin_rows.0, 1);
}

// ---- (c) an admin cannot remove their own admin role ----

#[sqlx::test(migrations = "./migrations")]
async fn test_own_admin_role_removal_refused(pool: PgPool) {
    let org = default_org_id(&pool).await;
    let (admin_id, admin_code) = default_admin_user(&pool).await;

    // Actor targets themselves and tries to drop admin -> self-guard fires
    // first (409) with a message mentioning "your own role".
    let err = update(
        admin_claims(org, admin_id, &admin_code),
        State(pool.clone()),
        Path(admin_id),
        Json(single_role_body("viewer")),
    )
    .await
    .expect_err("removing your own admin role must fail");

    match err {
        AppError::Conflict(msg) => {
            assert!(
                msg.to_lowercase().contains("your own role"),
                "expected self-role conflict, got: {msg}"
            );
        }
        other => panic!("expected Conflict, got {other:?}"),
    }

    // Still admin.
    let mirror: (String,) = sqlx::query_as("SELECT role FROM users WHERE id = $1")
        .bind(admin_id)
        .fetch_one(&pool)
        .await
        .expect("mirror role");
    assert_eq!(mirror.0, "admin");
}

// ---- (d) the role change is audited with the roles array ----

#[sqlx::test(migrations = "./migrations")]
async fn test_role_change_audited_with_roles_array(pool: PgPool) {
    let org = default_org_id(&pool).await;
    let (admin_id, admin_code) = default_admin_user(&pool).await;
    let target = seed_user(&pool, org, "Val Viewer", "val@example.test", "VAL", "viewer").await;

    let _ = update(
        admin_claims(org, admin_id, &admin_code),
        State(pool.clone()),
        Path(target),
        Json(single_role_body("technician")),
    )
    .await
    .expect("update should succeed");

    let diff: (serde_json::Value,) = sqlx::query_as(
        "SELECT diff_jsonb FROM audit_log \
         WHERE table_name = 'users' AND action = 'UPDATE' AND record_id = $1 \
         ORDER BY id DESC LIMIT 1",
    )
    .bind(target)
    .fetch_one(&pool)
    .await
    .expect("audit row for the role change");

    let d = &diff.0;
    // The audit snapshot represents roles as the `roles` array (per PR 1),
    // and shallow_diff includes it because it changed.
    assert_eq!(
        d.pointer("/before/roles/0").and_then(|v| v.as_str()),
        Some("viewer"),
        "before.roles should be [viewer], diff: {d}"
    );
    assert_eq!(
        d.pointer("/after/roles/0").and_then(|v| v.as_str()),
        Some("technician"),
        "after.roles should be [technician], diff: {d}"
    );
}

// ---- create-path coverage (singular `role`, multi-role, missing, audit) ----

/// Body carrying only the singular `role` field — what the current
/// single-select Add User modal sends.
fn create_single_role(name: &str, email: &str, user_code: &str, role: &str) -> CreateUserRequest {
    CreateUserRequest {
        name: name.to_string(),
        email: email.to_string(),
        user_code: user_code.to_string(),
        role: Some(role.to_string()),
        roles: None,
        pilot_id: None,
    }
}

// ---- (a) create with a single role persists the set + mirror + invitation ----

#[sqlx::test(migrations = "./migrations")]
async fn test_create_with_single_role_persists_set_and_mirror(pool: PgPool) {
    let org = default_org_id(&pool).await;
    let (admin_id, admin_code) = default_admin_user(&pool).await;

    let resp = create(
        admin_claims(org, admin_id, &admin_code),
        State(pool.clone()),
        Json(create_single_role(
            "New Tech",
            "newtech@example.test",
            "NTX",
            "technician",
        )),
    )
    .await
    .expect("single-role create should succeed")
    .0;

    // Response mirrors the role in both fields.
    assert_eq!(resp.user.role, "technician");
    assert_eq!(resp.user.roles, vec!["technician".to_string()]);
    // Invitation flow intact — 43-char url-safe plaintext token returned once.
    assert_eq!(resp.invitation_token.len(), 43);

    let uid = resp.user.id;
    let mirror: (String,) = sqlx::query_as("SELECT role FROM users WHERE id = $1")
        .bind(uid)
        .fetch_one(&pool)
        .await
        .expect("mirror role");
    assert_eq!(mirror.0, "technician");

    let roles: Vec<(String,)> =
        sqlx::query_as("SELECT role FROM user_roles WHERE user_id = $1 ORDER BY role")
            .bind(uid)
            .fetch_all(&pool)
            .await
            .expect("user_roles rows");
    assert_eq!(roles.len(), 1, "exactly one role expected, got {roles:?}");
    assert_eq!(roles[0].0, "technician");

    // An invitation token row was written in the same transaction.
    let tokens: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM user_tokens WHERE user_id = $1 AND purpose = 'invitation'",
    )
    .bind(uid)
    .fetch_one(&pool)
    .await
    .expect("invitation token count");
    assert_eq!(tokens.0, 1);
}

// ---- (b) create with a roles array sets the highest-precedence mirror ----

#[sqlx::test(migrations = "./migrations")]
async fn test_create_with_roles_array_sets_highest_precedence_mirror(pool: PgPool) {
    let org = default_org_id(&pool).await;
    let (admin_id, admin_code) = default_admin_user(&pool).await;

    let resp = create(
        admin_claims(org, admin_id, &admin_code),
        State(pool.clone()),
        Json(CreateUserRequest {
            name: "Multi Role".to_string(),
            email: "multi@example.test".to_string(),
            user_code: "MUL".to_string(),
            role: None,
            roles: Some(vec![
                "viewer".to_string(),
                "technician".to_string(),
                "safety_officer".to_string(),
            ]),
            pilot_id: None,
        }),
    )
    .await
    .expect("multi-role create should succeed")
    .0;

    // Precedence admin > technician > safety_officer > pilot > viewer, so the
    // primary mirror for {viewer, technician, safety_officer} is technician.
    assert_eq!(resp.user.role, "technician");
    // roles is array_agg(... ORDER BY role) — alphabetical.
    assert_eq!(
        resp.user.roles,
        vec![
            "safety_officer".to_string(),
            "technician".to_string(),
            "viewer".to_string()
        ]
    );

    let uid = resp.user.id;
    let mirror: (String,) = sqlx::query_as("SELECT role FROM users WHERE id = $1")
        .bind(uid)
        .fetch_one(&pool)
        .await
        .expect("mirror role");
    assert_eq!(mirror.0, "technician");

    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM user_roles WHERE user_id = $1")
        .bind(uid)
        .fetch_one(&pool)
        .await
        .expect("role count");
    assert_eq!(count.0, 3);
}

// ---- (c) create with neither role nor roles is a clean 4xx ----

#[sqlx::test(migrations = "./migrations")]
async fn test_create_without_any_role_rejected(pool: PgPool) {
    let org = default_org_id(&pool).await;
    let (admin_id, admin_code) = default_admin_user(&pool).await;

    let err = create(
        admin_claims(org, admin_id, &admin_code),
        State(pool.clone()),
        Json(CreateUserRequest {
            name: "No Role".to_string(),
            email: "norole@example.test".to_string(),
            user_code: "NRL".to_string(),
            role: None,
            roles: None,
            pilot_id: None,
        }),
    )
    .await
    .expect_err("create with no role must fail");

    match err {
        AppError::BadRequest(msg) => {
            assert!(
                msg.to_lowercase().contains("at least one role"),
                "expected a descriptive role error, got: {msg}"
            );
        }
        other => panic!("expected BadRequest, got {other:?}"),
    }

    // Nothing was written.
    let created: (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM users WHERE user_code = 'NRL' AND org_id = $1")
            .bind(org)
            .fetch_one(&pool)
            .await
            .expect("count");
    assert_eq!(created.0, 0);
}

// ---- (d) the create is audited with the roles array ----

#[sqlx::test(migrations = "./migrations")]
async fn test_create_audited_with_roles_array(pool: PgPool) {
    let org = default_org_id(&pool).await;
    let (admin_id, admin_code) = default_admin_user(&pool).await;

    let resp = create(
        admin_claims(org, admin_id, &admin_code),
        State(pool.clone()),
        Json(create_single_role("Audited", "aud@example.test", "AUD", "pilot")),
    )
    .await
    .expect("create should succeed")
    .0;

    let diff: (serde_json::Value,) = sqlx::query_as(
        "SELECT diff_jsonb FROM audit_log \
         WHERE table_name = 'users' AND action = 'INSERT' AND record_id = $1 \
         ORDER BY id DESC LIMIT 1",
    )
    .bind(resp.user.id)
    .fetch_one(&pool)
    .await
    .expect("audit row for the create");

    // log_insert_full stores { "after": { ...fields incl roles array... } }.
    let d = &diff.0;
    assert_eq!(
        d.pointer("/after/roles/0").and_then(|v| v.as_str()),
        Some("pilot"),
        "after.roles should be [pilot], diff: {d}"
    );
}
