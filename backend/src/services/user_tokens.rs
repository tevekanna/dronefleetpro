// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

//! Token generation, hashing and constant-time verification for invitation
//! and password-reset flows. Plaintext tokens are returned to admins exactly
//! once at issue time; the database only ever stores the bcrypt hash.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::{DateTime, Duration, Utc};
use rand::rngs::OsRng;
use rand::RngCore;
use sqlx::{Postgres, Row, Transaction};

use crate::errors::AppError;

/// Minimum length of any password a user sets for themselves — through
/// token redemption, self-service change, or the startup bootstrap
/// (`services::admin_bootstrap`). One constant so the rule cannot drift
/// between those paths.
pub const MIN_PASSWORD_LEN: usize = 12;

/// Token lifetime — hardcoded for v1 across both invitation and password-reset
/// purposes. Matches what an admin can reasonably hold open in a chat thread
/// while the user is unreachable.
pub const TOKEN_TTL_HOURS: i64 = 72;

const BCRYPT_COST: u32 = 10;

/// Generate a 32-byte CSPRNG token, URL-safe base64-encoded with no padding
/// (43 chars). Suitable for embedding directly in a redemption URL.
pub fn generate_token() -> String {
    let mut buf = [0u8; 32];
    OsRng.fill_bytes(&mut buf);
    URL_SAFE_NO_PAD.encode(buf)
}

/// bcrypt-hash a plaintext token at the same cost we use for passwords.
pub fn hash_token(plaintext: &str) -> Result<String, AppError> {
    bcrypt::hash(plaintext, BCRYPT_COST)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("token hashing failed: {e}")))
}

/// Verify a plaintext token against a stored bcrypt hash. Wraps bcrypt::verify
/// so callers don't need to map its error.
pub fn verify_token(plaintext: &str, stored_hash: &str) -> bool {
    bcrypt::verify(plaintext, stored_hash).unwrap_or(false)
}

/// A freshly issued password-reset token. `plaintext` exists only in memory
/// and is handed to the caller exactly once; the database holds `token_hash`.
pub struct IssuedResetToken {
    pub id: i64,
    pub plaintext: String,
    pub token_hash: String,
    pub expires_at: DateTime<Utc>,
}

/// Invalidate every unconsumed password-reset token for the user, then issue
/// a new one, inside the caller's transaction. Shared by the admin
/// `POST /users/:id/reset-token` endpoint and the startup bootstrap so both
/// follow the same "only the newest link works" rule.
///
/// `issued_by` is the admin who asked for the token, or `None` when the
/// system issues it (startup bootstrap).
pub async fn issue_password_reset_token(
    tx: &mut Transaction<'_, Postgres>,
    org_id: i32,
    user_id: i32,
    issued_by: Option<i32>,
) -> Result<IssuedResetToken, AppError> {
    let plaintext = generate_token();
    let token_hash = hash_token(&plaintext)?;
    let expires_at = Utc::now() + Duration::hours(TOKEN_TTL_HOURS);

    sqlx::query(
        "UPDATE user_tokens SET consumed_at = NOW() \
         WHERE org_id = $1 AND user_id = $2 \
           AND purpose = 'password_reset' AND consumed_at IS NULL",
    )
    .bind(org_id)
    .bind(user_id)
    .execute(&mut **tx)
    .await?;

    let id: i64 = sqlx::query(
        "INSERT INTO user_tokens (org_id, user_id, purpose, token_hash, issued_by, expires_at) \
         VALUES ($1, $2, 'password_reset', $3, $4, $5) RETURNING id",
    )
    .bind(org_id)
    .bind(user_id)
    .bind(&token_hash)
    .bind(issued_by)
    .bind(expires_at)
    .fetch_one(&mut **tx)
    .await?
    .get("id");

    Ok(IssuedResetToken {
        id,
        plaintext,
        token_hash,
        expires_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generated_tokens_are_unique_and_url_safe() {
        let a = generate_token();
        let b = generate_token();
        assert_ne!(a, b, "two generated tokens must differ");
        assert_eq!(a.len(), 43, "url-safe-no-pad length for 32 bytes");
        for c in a.chars() {
            assert!(
                c.is_ascii_alphanumeric() || c == '-' || c == '_',
                "non-url-safe char: {c}"
            );
        }
    }

    #[test]
    fn hash_and_verify_roundtrip() {
        let token = generate_token();
        let hash = hash_token(&token).expect("hash");
        assert!(verify_token(&token, &hash));
        assert!(!verify_token("wrong-token", &hash));
    }
}
