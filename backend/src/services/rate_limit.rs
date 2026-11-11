// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

//! Per-username rate limiting for credential-handling auth endpoints.
//!
//! # Layering
//!
//! This module is the SECOND layer of auth rate limiting. The FIRST layer is
//! `tower_governor` in `main.rs`, configured per-IP via `SmartIpKeyExtractor`.
//! Both layers stay active:
//!
//!   incoming POST /auth/{login,redeem-token,change-password}
//!     -> tower_governor (per-IP, RAM-only, 1 token / 12 s, burst 6)
//!     -> handler body
//!     -> rate_limit::check_and_record(...)  ← THIS MODULE (per-identifier, Postgres-backed, 10 / 15 min)
//!     -> bcrypt verification
//!     -> downstream logic
//!
//! The per-IP layer guards against generic flood/DoS from one source.
//! Per-username defends against a credential-spam attack that comes from many
//! sources or — critically — through a NAT that collapses many sources into
//! one IP. The latter is exactly the situation on a Docker Desktop for
//! Windows deployment: it NATs every external connection to the bridge
//! gateway IP, so there the per-IP layer alone protects nothing per-user. Per-username
//! survives that collapse because the identifier is the credential itself,
//! not the network address.
//!
//! # Behaviour
//!
//! - 10 attempts per 15-minute rolling window per (scope, identifier).
//! - The 11th attempt within the window returns `Locked`.
//! - The locked attempt is NOT inserted (otherwise the window would never
//!   drain).
//! - Successful auth does NOT clear the counter — only time drains it.
//!
//! # Identifier hygiene
//!
//! - login: lowercase + trim email
//! - redeem_token: caller MUST hash the raw token (sha256 hex) before passing
//!   it in. The raw token must never enter this table.
//! - change_password: `user:<id>` — keyed on the JWT-authenticated user.
//!
//! Threshold and window were chosen on 2026-05-03.

use sha2::{Digest, Sha256};
use sqlx::PgPool;

use crate::services::audit;

const MAX_ATTEMPTS: i64 = 10;
const WINDOW_MINUTES: i64 = 15;

#[derive(Debug, Clone, Copy)]
pub enum RateLimitScope {
    Login,
    RedeemToken,
    ChangePassword,
}

impl RateLimitScope {
    pub fn as_str(self) -> &'static str {
        match self {
            RateLimitScope::Login => "login",
            RateLimitScope::RedeemToken => "redeem_token",
            RateLimitScope::ChangePassword => "change_password",
        }
    }
}

impl std::fmt::Display for RateLimitScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug)]
pub enum RateLimitError {
    Locked { retry_after_seconds: u64 },
    Database(sqlx::Error),
}

/// Check the rolling-window count for `(scope, identifier)`. If under the
/// threshold, insert a new attempt row and return `Ok(())`. If at or over
/// the threshold, return `Err(Locked { retry_after_seconds })` WITHOUT
/// inserting — locked attempts must not extend the window.
pub async fn check_and_record(
    pool: &PgPool,
    scope: RateLimitScope,
    identifier: &str,
) -> Result<(), RateLimitError> {
    let scope_str = scope.as_str();
    let mut tx = pool.begin().await.map_err(RateLimitError::Database)?;

    // Count attempts in the rolling window plus the oldest one (so we can
    // tell the caller how long until the window drains by one slot).
    let row: (i64, Option<chrono::DateTime<chrono::Utc>>) = sqlx::query_as(
        "SELECT COUNT(*)::BIGINT AS cnt, MIN(attempt_at) AS oldest \
         FROM auth_attempts \
         WHERE scope = $1 AND identifier = $2 \
           AND attempt_at > NOW() - ($3::TEXT || ' minutes')::INTERVAL",
    )
    .bind(scope_str)
    .bind(identifier)
    .bind(WINDOW_MINUTES.to_string())
    .fetch_one(&mut *tx)
    .await
    .map_err(RateLimitError::Database)?;

    let (cnt, oldest) = row;

    if cnt >= MAX_ATTEMPTS {
        // Locked: do NOT insert. retry_after = (oldest + window) - now.
        let now = chrono::Utc::now();
        let secs = oldest
            .map(|o| {
                let drain_at = o + chrono::Duration::minutes(WINDOW_MINUTES);
                let delta = drain_at.signed_duration_since(now).num_seconds();
                delta.max(1) as u64
            })
            .unwrap_or(1);
        // Drop the txn (no writes done; rollback is implicit on drop).
        return Err(RateLimitError::Locked {
            retry_after_seconds: secs,
        });
    }

    sqlx::query(
        "INSERT INTO auth_attempts (scope, identifier) VALUES ($1, $2)",
    )
    .bind(scope_str)
    .bind(identifier)
    .execute(&mut *tx)
    .await
    .map_err(RateLimitError::Database)?;

    tx.commit().await.map_err(RateLimitError::Database)?;
    Ok(())
}

/// SHA-256 hex of the input, full 64-char. Used by the redeem-token handler
/// to derive the rate-limit identifier from the raw token without ever
/// storing the token itself. Audit logs use a 16-char prefix
/// (see `audit_lockout`).
pub fn sha256_hex(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    let digest = hasher.finalize();
    digest.iter().fold(String::with_capacity(64), |mut acc, b| {
        use std::fmt::Write;
        let _ = write!(acc, "{:02x}", b);
        acc
    })
}

/// Write a single `RATE_LIMITED` row to `audit_log`. Best-effort: caller
/// should treat the response as fire-and-forget so a transient audit
/// failure doesn't change the user-facing 429 response.
pub async fn audit_lockout(
    pool: &PgPool,
    scope: RateLimitScope,
    identifier: &str,
    retry_after_seconds: u64,
) {
    // 16-char prefix is enough to correlate repeated lockouts on the same
    // identifier without leaking the raw email / token / user ID.
    let full = sha256_hex(identifier);
    let identifier_hash = full[..16].to_string();
    audit::log_auth_event(
        pool,
        None,
        None,
        "",
        "RATE_LIMITED",
        "auth_attempts",
        None,
        Some(serde_json::json!({
            "scope": scope.as_str(),
            "identifier_hash": identifier_hash,
            "retry_after_seconds": retry_after_seconds,
        })),
    )
    .await;
}

// =====================================================================
// Tests — sqlx::test gives each test its own ephemeral database; we point
// it at the local Postgres via DATABASE_URL and let it apply migrations
// from ./migrations automatically.
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::PgPool;

    fn fresh_id(prefix: &str) -> String {
        // Unique-per-test identifier so parallel tests don't interfere even
        // if sqlx::test fixture isolation breaks down.
        format!(
            "{}-{}-{}",
            prefix,
            std::process::id(),
            uuid::Uuid::new_v4()
        )
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn under_threshold_allows_attempts(pool: PgPool) {
        let id = fresh_id("under");
        for i in 0..MAX_ATTEMPTS {
            let r = check_and_record(&pool, RateLimitScope::Login, &id).await;
            assert!(r.is_ok(), "attempt {} should succeed: {:?}", i + 1, r);
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn eleventh_attempt_returns_locked(pool: PgPool) {
        let id = fresh_id("eleventh");
        for _ in 0..MAX_ATTEMPTS {
            check_and_record(&pool, RateLimitScope::Login, &id)
                .await
                .expect("first 10 should succeed");
        }
        let r = check_and_record(&pool, RateLimitScope::Login, &id).await;
        match r {
            Err(RateLimitError::Locked {
                retry_after_seconds,
            }) => {
                assert!(
                    retry_after_seconds >= 1
                        && retry_after_seconds <= (WINDOW_MINUTES as u64) * 60,
                    "retry_after_seconds out of range: {}",
                    retry_after_seconds
                );
            }
            other => panic!("expected Locked, got {:?}", other),
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn locked_attempts_dont_extend_window(pool: PgPool) {
        let id = fresh_id("noextend");
        for _ in 0..MAX_ATTEMPTS {
            check_and_record(&pool, RateLimitScope::Login, &id)
                .await
                .expect("first 10 should succeed");
        }
        // Snapshot the oldest attempt_at after the 10 successful inserts.
        let oldest_before: chrono::DateTime<chrono::Utc> = sqlx::query_scalar(
            "SELECT MIN(attempt_at) FROM auth_attempts WHERE scope = $1 AND identifier = $2",
        )
        .bind("login")
        .bind(&id)
        .fetch_one(&pool)
        .await
        .expect("query");

        // Hammer 5 more times — all must lock.
        for _ in 0..5 {
            let r = check_and_record(&pool, RateLimitScope::Login, &id).await;
            assert!(matches!(r, Err(RateLimitError::Locked { .. })));
        }

        // The locked branch must NOT have inserted any new rows.
        let cnt: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM auth_attempts WHERE scope = $1 AND identifier = $2",
        )
        .bind("login")
        .bind(&id)
        .fetch_one(&pool)
        .await
        .expect("query");
        assert_eq!(cnt, MAX_ATTEMPTS, "no new rows should be inserted while locked");

        // Oldest should be unchanged.
        let oldest_after: chrono::DateTime<chrono::Utc> = sqlx::query_scalar(
            "SELECT MIN(attempt_at) FROM auth_attempts WHERE scope = $1 AND identifier = $2",
        )
        .bind("login")
        .bind(&id)
        .fetch_one(&pool)
        .await
        .expect("query");
        assert_eq!(oldest_before, oldest_after);
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn different_identifiers_are_isolated(pool: PgPool) {
        let alice = fresh_id("alice");
        let bob = fresh_id("bob");
        for _ in 0..MAX_ATTEMPTS {
            check_and_record(&pool, RateLimitScope::Login, &alice)
                .await
                .expect("alice ok");
        }
        // Alice should now lock.
        assert!(matches!(
            check_and_record(&pool, RateLimitScope::Login, &alice).await,
            Err(RateLimitError::Locked { .. })
        ));
        // Bob should still be free.
        for _ in 0..MAX_ATTEMPTS {
            assert!(
                check_and_record(&pool, RateLimitScope::Login, &bob)
                    .await
                    .is_ok()
            );
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn different_scopes_are_isolated(pool: PgPool) {
        let id = fresh_id("scopes");
        for _ in 0..MAX_ATTEMPTS {
            check_and_record(&pool, RateLimitScope::Login, &id)
                .await
                .expect("login ok");
        }
        assert!(matches!(
            check_and_record(&pool, RateLimitScope::Login, &id).await,
            Err(RateLimitError::Locked { .. })
        ));
        // Same identifier under a different scope must be unaffected.
        for _ in 0..MAX_ATTEMPTS {
            assert!(
                check_and_record(&pool, RateLimitScope::RedeemToken, &id)
                    .await
                    .is_ok()
            );
        }
    }

    #[sqlx::test(migrations = "./migrations")]
    async fn old_attempts_outside_window_dont_count(pool: PgPool) {
        let id = fresh_id("aged");
        // Manually insert 10 attempts dated 20 minutes ago — outside the
        // 15-minute window — so the next check should succeed.
        for _ in 0..MAX_ATTEMPTS {
            sqlx::query(
                "INSERT INTO auth_attempts (scope, identifier, attempt_at) \
                 VALUES ($1, $2, NOW() - INTERVAL '20 minutes')",
            )
            .bind("login")
            .bind(&id)
            .execute(&pool)
            .await
            .expect("manual insert");
        }
        let r = check_and_record(&pool, RateLimitScope::Login, &id).await;
        assert!(r.is_ok(), "old attempts should not count: {:?}", r);
    }

    #[test]
    fn sha256_hex_is_deterministic_and_full_width() {
        let h1 = sha256_hex("alice@example.com");
        let h2 = sha256_hex("alice@example.com");
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 64, "full SHA-256 hex is 64 chars");
        // Different inputs produce different hashes.
        let h3 = sha256_hex("bob@example.com");
        assert_ne!(h1, h3);
        // Known SHA-256 vector — cheap regression guard against algorithm swap.
        assert_eq!(
            sha256_hex(""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
}
