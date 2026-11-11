// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

//! Tests for the startup bootstrap. Wired into the binary crate via
//! `#[cfg(test)] #[path = "admin_bootstrap_tests.rs"] mod tests;` in
//! admin_bootstrap.rs.
//!
//! `#[sqlx::test(migrations = "./migrations")]` gives each test a fresh
//! database in which migration 002 has just seeded the default admin with the
//! public seed hash — exactly the state of a new installation.

use super::*;
use crate::handlers::auth::login;
use crate::handlers::user_tokens::{redeem, RedeemRequest};
use crate::models::user::LoginRequest;
use axum::extract::State;
use axum::Json;
use sqlx::Row;

const SEED_ADMIN_EMAIL: &str = "admin@dronefleetpro.local";
const GOOD_PASSWORD: &str = "a-strong-test-password";

async fn seed_admin(pool: &PgPool) -> (i32, String) {
    let row = sqlx::query("SELECT id, password_hash FROM users WHERE email = $1")
        .bind(SEED_ADMIN_EMAIL)
        .fetch_one(pool)
        .await
        .expect("default admin seeded by migration 002");
    (row.get("id"), row.get("password_hash"))
}

async fn unconsumed_reset_tokens(pool: &PgPool, user_id: i32) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM user_tokens \
         WHERE user_id = $1 AND purpose = 'password_reset' AND consumed_at IS NULL",
    )
    .bind(user_id)
    .fetch_one(pool)
    .await
    .expect("count tokens")
}

async fn total_tokens(pool: &PgPool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM user_tokens")
        .fetch_one(pool)
        .await
        .expect("count tokens")
}

async fn try_login(pool: &PgPool, password: &str) -> Result<Json<serde_json::Value>, AppError> {
    login(
        State(pool.clone()),
        Json(LoginRequest {
            email: SEED_ADMIN_EMAIL.to_string(),
            password: password.to_string(),
        }),
    )
    .await
}

#[sqlx::test(migrations = "./migrations")]
async fn migration_seeds_the_public_hash(pool: PgPool) {
    // Guards the constant: if it ever drifts from migration 002 the bootstrap
    // would silently match nothing.
    let (_, hash) = seed_admin(&pool).await;
    assert_eq!(hash, SEED_PASSWORD_HASH);
}

#[sqlx::test(migrations = "./migrations")]
async fn env_password_replaces_seed_hash_without_tokens(pool: PgPool) {
    let report = run(&pool, Some(GOOD_PASSWORD.to_string()))
        .await
        .expect("bootstrap");
    assert_eq!(report.replaced, 1);
    assert_eq!(report.pending, 0);
    assert!(report.pending_links.is_empty());

    let (_, hash) = seed_admin(&pool).await;
    assert_ne!(hash, SEED_PASSWORD_HASH);
    assert!(bcrypt::verify(GOOD_PASSWORD, &hash).expect("valid bcrypt hash"));
    assert!(!bcrypt::verify("dronefleet2025", &hash).expect("valid bcrypt hash"));
    assert_eq!(total_tokens(&pool).await, 0);

    // Audited as a system action, with no password or hash in the row.
    let row = sqlx::query(
        "SELECT user_id, user_code, diff_jsonb::text AS diff FROM audit_log \
         WHERE action = 'BOOTSTRAP_PASSWORD_REPLACED'",
    )
    .fetch_one(&pool)
    .await
    .expect("one audit row");
    assert_eq!(row.get::<Option<i32>, _>("user_id"), None);
    assert_eq!(row.get::<String, _>("user_code"), "SYS");
    let diff: String = row.get("diff");
    assert!(!diff.contains(GOOD_PASSWORD));
    assert!(!diff.contains("$2b$"));
}

#[sqlx::test(migrations = "./migrations")]
async fn no_env_locks_account_and_issues_one_redeemable_token(pool: PgPool) {
    let report = run(&pool, None).await.expect("bootstrap");
    assert_eq!(report.replaced, 0);
    assert_eq!(report.pending, 1);
    assert_eq!(report.pending_links.len(), 1);
    assert_eq!(report.pending_links[0].email, SEED_ADMIN_EMAIL);

    let (admin_id, hash) = seed_admin(&pool).await;
    assert_eq!(hash, BOOTSTRAP_PENDING_HASH);
    assert_eq!(unconsumed_reset_tokens(&pool, admin_id).await, 1);

    // The public seed password no longer works...
    assert!(matches!(
        try_login(&pool, "dronefleet2025").await,
        Err(AppError::Unauthorized(_))
    ));

    // ...the token redeems through the existing redeem handler...
    redeem(
        State(pool.clone()),
        Json(RedeemRequest {
            token: report.pending_links[0].token.clone(),
            new_password: GOOD_PASSWORD.to_string(),
        }),
    )
    .await
    .expect("redeem succeeds");
    assert_eq!(unconsumed_reset_tokens(&pool, admin_id).await, 0);

    // ...and login then succeeds with the new password.
    let body = try_login(&pool, GOOD_PASSWORD)
        .await
        .expect("login succeeds");
    assert!(body.0["token"].is_string());

    // The audit rows never contain the token.
    let leaked: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_log WHERE diff_jsonb::text LIKE '%' || $1 || '%'",
    )
    .bind(&report.pending_links[0].token)
    .fetch_one(&pool)
    .await
    .expect("count");
    assert_eq!(leaked, 0);
}

#[sqlx::test(migrations = "./migrations")]
async fn second_run_reissues_and_invalidates_previous_token(pool: PgPool) {
    let first = run(&pool, None).await.expect("first run");
    let second = run(&pool, None).await.expect("second run");
    assert_eq!(second.replaced, 0);
    assert_eq!(second.pending, 1);
    assert_ne!(first.pending_links[0].token, second.pending_links[0].token);

    let (admin_id, _) = seed_admin(&pool).await;
    assert_eq!(unconsumed_reset_tokens(&pool, admin_id).await, 1);
    assert_eq!(total_tokens(&pool).await, 2);

    // The first link is dead; the second works.
    let stale = redeem(
        State(pool.clone()),
        Json(RedeemRequest {
            token: first.pending_links[0].token.clone(),
            new_password: GOOD_PASSWORD.to_string(),
        }),
    )
    .await;
    assert!(matches!(stale, Err(AppError::BadRequest(_))));

    redeem(
        State(pool.clone()),
        Json(RedeemRequest {
            token: second.pending_links[0].token.clone(),
            new_password: GOOD_PASSWORD.to_string(),
        }),
    )
    .await
    .expect("newest token redeems");
}

#[sqlx::test(migrations = "./migrations")]
async fn idempotent_once_the_account_has_a_password(pool: PgPool) {
    let first = run(&pool, None).await.expect("first run");
    redeem(
        State(pool.clone()),
        Json(RedeemRequest {
            token: first.pending_links[0].token.clone(),
            new_password: GOOD_PASSWORD.to_string(),
        }),
    )
    .await
    .expect("redeem succeeds");

    let (_, hash_before) = seed_admin(&pool).await;
    let tokens_before = total_tokens(&pool).await;
    let audit_before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM audit_log")
        .fetch_one(&pool)
        .await
        .expect("count");

    // With or without the environment variable, nothing is left to do.
    for password in [None, Some("another-strong-password".to_string())] {
        let report = run(&pool, password).await.expect("later run");
        assert_eq!(report.replaced, 0);
        assert_eq!(report.pending, 0);
    }

    let (_, hash_after) = seed_admin(&pool).await;
    assert_eq!(hash_after, hash_before);
    assert_eq!(total_tokens(&pool).await, tokens_before);
    let audit_after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM audit_log")
        .fetch_one(&pool)
        .await
        .expect("count");
    assert_eq!(audit_after, audit_before);
}

#[sqlx::test(migrations = "./migrations")]
async fn short_env_password_falls_back_to_pending(pool: PgPool) {
    let report = run(&pool, Some("too-short".to_string()))
        .await
        .expect("bootstrap");
    assert_eq!(report.replaced, 0);
    assert_eq!(report.pending, 1);

    let (admin_id, hash) = seed_admin(&pool).await;
    assert_eq!(hash, BOOTSTRAP_PENDING_HASH);
    assert_eq!(unconsumed_reset_tokens(&pool, admin_id).await, 1);
}

#[sqlx::test(migrations = "./migrations")]
async fn login_against_pending_account_is_generic_invalid_credentials(pool: PgPool) {
    run(&pool, None).await.expect("bootstrap");

    // Same error, same message, as a wrong password on a normal account —
    // never a 500 and never a hint that the account is locked.
    for attempt in [
        "dronefleet2025",
        BOOTSTRAP_PENDING_HASH,
        "anything-else-at-all",
    ] {
        match try_login(&pool, attempt).await {
            Err(AppError::Unauthorized(msg)) => assert_eq!(msg, "Invalid email or password"),
            other => panic!("expected generic 401, got {:?}", other.map(|j| j.0)),
        }
    }

    // And it is audited like any other bad password.
    let failures: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM audit_log \
         WHERE action = 'LOGIN_FAILURE' AND diff_jsonb->>'reason' = 'bad_password'",
    )
    .fetch_one(&pool)
    .await
    .expect("count");
    assert_eq!(failures, 3);
}

#[test]
fn password_validation_rules() {
    // Too short in any build.
    assert!(validate_bootstrap_password("short", false).is_err());
    assert!(validate_bootstrap_password("short", true).is_err());
    // The public seed password: allowed in debug builds (local development),
    // rejected in release builds.
    assert!(validate_bootstrap_password(PUBLIC_SEED_PASSWORD, false).is_ok());
    assert!(validate_bootstrap_password(PUBLIC_SEED_PASSWORD, true).is_err());
    // Anything else long enough is fine in both.
    assert!(validate_bootstrap_password(GOOD_PASSWORD, false).is_ok());
    assert!(validate_bootstrap_password(GOOD_PASSWORD, true).is_ok());
}

#[test]
fn pending_marker_is_not_a_bcrypt_hash() {
    assert!(bcrypt::verify("anything", BOOTSTRAP_PENDING_HASH).is_err());
    assert!(bcrypt::verify("dronefleet2025", SEED_PASSWORD_HASH).expect("seed hash is bcrypt"));
}
