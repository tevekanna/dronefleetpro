// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

//! Startup bootstrap that neutralises the publicly known seed admin password.
//!
//! Migration 002 seeds `admin@dronefleetpro.local` with a fixed bcrypt hash
//! whose plaintext is public (the repository is open source). The migration
//! itself cannot be edited — sqlx checksums would break every existing
//! database — so this step runs on every start, straight after migrations and
//! before the server listens, and makes sure no account keeps that hash:
//!
//! * `DFP_BOOTSTRAP_ADMIN_PASSWORD` set and valid: every account still on the
//!   seed hash gets that password instead.
//! * Otherwise: every such account is locked with a marker hash that no
//!   password can match, and a one-time set-password link is written to the
//!   backend log. The link uses the existing password-reset token flow
//!   (`/redeem?token=…`). Each start issues a fresh link and invalidates the
//!   previous one, until the account has set a password.
//!
//! Once no account carries the seed or marker hash the step does nothing.

use serde_json::json;
use sqlx::PgPool;

use crate::errors::AppError;
use crate::services::{audit, user_tokens as token_service};

#[cfg(test)]
#[path = "admin_bootstrap_tests.rs"]
mod tests;

/// The bcrypt hash that migration 002 seeds for the default admin. It is
/// public by design: the migration is part of the open-source repository, and
/// so is its plaintext. Accounts are matched against it by plain string
/// equality rather than by verifying a password, because the point is to find
/// rows that still hold this exact, published value — a bcrypt hash embeds its
/// own salt, so only a row copied from the seed can be equal to it.
pub const SEED_PASSWORD_HASH: &str = "$2b$12$7T3ErfyTki4vQSnnFP6Spuw39JbFFRzMsrdsH/ZIFHgQrs0mJ13h.";

/// Marker stored in `users.password_hash` while an account waits for its
/// owner to set a password. Deliberately not a valid bcrypt string, so no
/// password can ever verify against it.
pub const BOOTSTRAP_PENDING_HASH: &str = "!bootstrap-pending";

/// The plaintext behind `SEED_PASSWORD_HASH`. Local development and the
/// Bruno/Playwright suites still use it (via `docker-compose.dev.yml`), but a
/// release build refuses it as a bootstrap password: accepting it would put
/// the published password straight back.
const PUBLIC_SEED_PASSWORD: &str = "dronefleet2025";

/// bcrypt cost for user passwords — the same cost the redeem and
/// change-password handlers use.
const PASSWORD_BCRYPT_COST: u32 = 12;

/// User code recorded on audit rows written by the system rather than by a
/// person (same convention as migration 011).
const SYSTEM_USER_CODE: &str = "SYS";

/// A one-time set-password link issued for a locked account. Collected in
/// test builds only, so that tests can redeem the token; production code
/// writes the link to the log and keeps no other copy.
#[cfg(test)]
pub struct PendingLink {
    pub email: String,
    pub token: String,
}

/// What a bootstrap run did. `replaced` counts accounts whose seed password
/// was replaced from the environment variable; `pending` counts accounts that
/// are locked and were issued a set-password link in this run.
#[derive(Default)]
pub struct BootstrapReport {
    pub replaced: usize,
    pub pending: usize,
    #[cfg(test)]
    pub pending_links: Vec<PendingLink>,
}

#[derive(sqlx::FromRow)]
struct AccountRow {
    id: i32,
    org_id: i32,
    email: String,
    is_active: bool,
}

/// Why a supplied bootstrap password cannot be used, or `Ok` if it can.
/// `release_build` is a parameter so both build profiles can be unit-tested.
fn validate_bootstrap_password(password: &str, release_build: bool) -> Result<(), String> {
    if password.len() < token_service::MIN_PASSWORD_LEN {
        return Err(format!(
            "it is shorter than {} characters",
            token_service::MIN_PASSWORD_LEN
        ));
    }
    if release_build && password == PUBLIC_SEED_PASSWORD {
        return Err("it is the publicly known seed password, which release builds reject".into());
    }
    Ok(())
}

/// Run the bootstrap. Called from `main.rs` after migrations succeed and
/// before the server starts listening. `bootstrap_password` is the value of
/// `DFP_BOOTSTRAP_ADMIN_PASSWORD` (`None` when unset or empty); it is passed
/// in so that this function never reads the process environment.
pub async fn run(
    pool: &PgPool,
    bootstrap_password: Option<String>,
) -> Result<BootstrapReport, AppError> {
    let mut report = BootstrapReport::default();

    // ---- Step A: accounts still carrying the public seed hash ----
    let seed_accounts: Vec<AccountRow> = sqlx::query_as(
        "SELECT id, org_id, email, is_active FROM users WHERE password_hash = $1 ORDER BY id",
    )
    .bind(SEED_PASSWORD_HASH)
    .fetch_all(pool)
    .await?;

    if !seed_accounts.is_empty() {
        let usable_password = match bootstrap_password.as_deref() {
            Some(password) => {
                match validate_bootstrap_password(password, cfg!(not(debug_assertions))) {
                    Ok(()) => Some(password),
                    Err(reason) => {
                        tracing::warn!(
                            "DFP_BOOTSTRAP_ADMIN_PASSWORD is set but will not be used because {}. \
                             Accounts with the seed password are locked instead and receive a \
                             one-time set-password link.",
                            reason
                        );
                        None
                    }
                }
            }
            None => None,
        };

        for account in &seed_accounts {
            let (new_hash, action, details) = match usable_password {
                Some(password) => (
                    bcrypt::hash(password, PASSWORD_BCRYPT_COST).map_err(|e| {
                        AppError::Internal(anyhow::anyhow!("Password hashing failed: {e}"))
                    })?,
                    "BOOTSTRAP_PASSWORD_REPLACED",
                    json!({
                        "reason": "seed_password_replaced",
                        "source": "DFP_BOOTSTRAP_ADMIN_PASSWORD",
                    }),
                ),
                None => (
                    BOOTSTRAP_PENDING_HASH.to_string(),
                    "BOOTSTRAP_PASSWORD_LOCKED",
                    json!({ "reason": "seed_password_locked" }),
                ),
            };

            // Guarded on the seed hash so a concurrent change is never
            // overwritten.
            let updated = sqlx::query(
                "UPDATE users SET password_hash = $1 WHERE id = $2 AND password_hash = $3",
            )
            .bind(&new_hash)
            .bind(account.id)
            .bind(SEED_PASSWORD_HASH)
            .execute(pool)
            .await?
            .rows_affected();
            if updated == 0 {
                continue;
            }

            audit::log_auth_event(
                pool,
                Some(account.org_id),
                None,
                SYSTEM_USER_CODE,
                action,
                "users",
                Some(account.id),
                Some(details),
            )
            .await;

            if usable_password.is_some() {
                report.replaced += 1;
                tracing::info!(
                    "Seed password replaced from DFP_BOOTSTRAP_ADMIN_PASSWORD for {}",
                    account.email
                );
            }
        }
    }

    // ---- Step B: locked accounts waiting for a password ----
    // Includes the accounts locked just above and any left from an earlier
    // start.
    let pending_accounts: Vec<AccountRow> = sqlx::query_as(
        "SELECT id, org_id, email, is_active FROM users WHERE password_hash = $1 ORDER BY id",
    )
    .bind(BOOTSTRAP_PENDING_HASH)
    .fetch_all(pool)
    .await?;

    for account in pending_accounts {
        let mut tx = pool.begin().await?;
        let issued =
            token_service::issue_password_reset_token(&mut tx, account.org_id, account.id, None)
                .await?;
        tx.commit().await?;

        audit::log_auth_event(
            pool,
            Some(account.org_id),
            None,
            SYSTEM_USER_CODE,
            "BOOTSTRAP_LINK_ISSUED",
            "user_tokens",
            Some(issued.id as i32),
            Some(json!({
                "user_id": account.id,
                "purpose": "password_reset",
                "expires_at": issued.expires_at,
            })),
        )
        .await;

        let deactivated_note = if account.is_active {
            ""
        } else {
            "\n  Note:    this account is deactivated; reactivate it before using the link."
        };
        // The only place a plaintext token is ever logged.
        tracing::warn!(
            "\n\
             ================ ACTION REQUIRED: SET A PASSWORD ================\n\
             \x20 The account below had the public seed password and is locked\n\
             \x20 until a new password is set through this one-time link.\n\
             \x20 Account: {email}\n\
             \x20 Open:    /redeem?token={token}\n\
             \x20          (append this path to the address of your DroneFleetPro site)\n\
             \x20 Expires: {expires} UTC{note}\n\
             \x20 Restarting the backend issues a new link and invalidates this one.\n\
             =================================================================",
            email = account.email,
            token = issued.plaintext,
            expires = issued.expires_at.format("%Y-%m-%d %H:%M"),
            note = deactivated_note,
        );

        report.pending += 1;
        #[cfg(test)]
        report.pending_links.push(PendingLink {
            email: account.email,
            token: issued.plaintext,
        });
    }

    if report.replaced == 0 && report.pending == 0 {
        tracing::debug!("Admin bootstrap: no account carries the seed or pending hash.");
    }

    Ok(report)
}
