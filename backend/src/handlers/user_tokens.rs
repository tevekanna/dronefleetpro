// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

//! Public redemption endpoint for invitation and password-reset tokens.
//! Both purposes share the same flow — submit token + new_password, the server
//! verifies the bcrypt hash, sets the user's password, and marks the token
//! consumed. On any failure the response is a generic 400 so the caller can't
//! enumerate which condition failed.

use axum::{extract::State, http::StatusCode, response::IntoResponse, Json};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use sqlx::Row;

use crate::db::DbPool;
use crate::errors::AppError;
use crate::services::{
    audit,
    rate_limit::{self, RateLimitError, RateLimitScope},
    user_tokens::{self as token_service, MIN_PASSWORD_LEN},
};

#[derive(Debug, Deserialize)]
pub struct RedeemRequest {
    pub token: String,
    pub new_password: String,
}

fn invalid_token_error() -> AppError {
    AppError::BadRequest("invalid or expired token".into())
}

/// POST /api/v1/auth/redeem-token — public, no JWT.
pub async fn redeem(
    State(pool): State<DbPool>,
    Json(body): Json<RedeemRequest>,
) -> Result<impl IntoResponse, AppError> {
    if body.new_password.len() < MIN_PASSWORD_LEN {
        return Err(AppError::BadRequest(format!(
            "Password must be at least {MIN_PASSWORD_LEN} characters"
        )));
    }
    let token_input = body.token.trim();
    if token_input.is_empty() {
        return Err(invalid_token_error());
    }

    // Per-username rate limit, keyed on sha256(token) hex. Critical here
    // because the redeem path bcrypt-verifies against EVERY active token row
    // — every bogus attempt costs O(N) bcrypt work, the worst-case spam
    // target of the three auth endpoints. The raw token never enters the
    // counter; only its irreversible hash does.
    let identifier = rate_limit::sha256_hex(token_input);
    match rate_limit::check_and_record(&pool, RateLimitScope::RedeemToken, &identifier).await {
        Ok(()) => {}
        Err(RateLimitError::Locked {
            retry_after_seconds,
        }) => {
            rate_limit::audit_lockout(
                &pool,
                RateLimitScope::RedeemToken,
                &identifier,
                retry_after_seconds,
            )
            .await;
            return Err(AppError::RateLimited {
                retry_after_seconds,
            });
        }
        Err(RateLimitError::Database(e)) => return Err(AppError::Database(e)),
    }

    // Pull every unconsumed, unexpired token; bcrypt check against each.
    // bcrypt verification is the only safe match — token_hash is a salted
    // bcrypt of the plaintext, so we can't index-lookup. Cardinality is small
    // (active token rows per system) so a linear scan is fine.
    let candidates = sqlx::query(
        "SELECT id, org_id, user_id, purpose, token_hash, expires_at \
         FROM user_tokens \
         WHERE consumed_at IS NULL AND expires_at > NOW()",
    )
    .fetch_all(&pool)
    .await?;

    let mut matched: Option<(i64, i32, i32, String, String, DateTime<Utc>)> = None;
    for row in candidates {
        let stored_hash: String = row.try_get("token_hash").map_err(AppError::Database)?;
        if token_service::verify_token(token_input, &stored_hash) {
            matched = Some((
                row.try_get("id").map_err(AppError::Database)?,
                row.try_get("org_id").map_err(AppError::Database)?,
                row.try_get("user_id").map_err(AppError::Database)?,
                row.try_get("purpose").map_err(AppError::Database)?,
                stored_hash,
                row.try_get("expires_at").map_err(AppError::Database)?,
            ));
            break;
        }
    }

    let (token_id, org_id, user_id, purpose, _stored_hash, _expires_at) =
        matched.ok_or_else(invalid_token_error)?;

    // Defence-in-depth: confirm the user exists and is active. Also pull
    // the user_code so the audit row can carry it.
    let user_row: Option<(bool, String)> =
        sqlx::query_as("SELECT is_active, user_code FROM users WHERE id = $1 AND org_id = $2")
            .bind(user_id)
            .bind(org_id)
            .fetch_optional(&pool)
            .await?;
    let (is_active, user_code) = user_row.ok_or_else(invalid_token_error)?;
    if !is_active {
        return Err(invalid_token_error());
    }

    let new_hash = bcrypt::hash(&body.new_password, 12)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Password hashing failed: {e}")))?;

    let mut tx = pool.begin().await?;
    sqlx::query("UPDATE users SET password_hash = $1 WHERE id = $2 AND org_id = $3")
        .bind(&new_hash)
        .bind(user_id)
        .bind(org_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query("UPDATE user_tokens SET consumed_at = NOW() WHERE id = $1")
        .bind(token_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;

    // Audit: TOKEN_REDEEMED — public endpoint, no JWT, but the redemption
    // is tied to a specific user (whose password just changed). Carry the
    // user's id + user_code so the row surfaces in the user's tenant audit
    // viewer. Purpose distinguishes invitation vs. password reset.
    audit::log_auth_event(
        &pool,
        Some(org_id),
        Some(user_id),
        &user_code,
        "TOKEN_REDEEMED",
        "user_tokens",
        Some(token_id as i32),
        Some(serde_json::json!({ "purpose": purpose })),
    )
    .await;

    Ok(StatusCode::NO_CONTENT)
}
