// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

use crate::errors::AppError;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct JwtClaims {
    pub sub: i32,          // user_id
    pub org_id: i32,
    pub role: String,      // primary role (legacy single-role claim)
    /// Full set of RBAC roles. `#[serde(default)]` so OLD tokens minted before
    /// multi-role (which lack this field) still decode; `decode_jwt`
    /// normalises an empty set to `vec![role]` so a legacy token behaves as
    /// single-role.
    #[serde(default)]
    pub roles: Vec<String>,
    pub user_code: String,
    pub exp: i64,
}

pub fn encode_jwt(claims: &JwtClaims, secret: &str) -> Result<String, AppError> {
    encode(
        &Header::default(),
        claims,
        &EncodingKey::from_secret(secret.as_bytes()),
    )
    .map_err(|e| AppError::Internal(anyhow::anyhow!("JWT encode error: {}", e)))
}

pub fn decode_jwt(token: &str, secret: &str) -> Result<JwtClaims, AppError> {
    let token_data = decode::<JwtClaims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )
    .map_err(|e| AppError::Unauthorized(format!("Invalid token: {}", e)))?;

    // Normalisation point: a legacy token carries no `roles`, so fall back to
    // treating its single `role` as the whole set. Every downstream consumer
    // (Claims, guards) can then rely on `roles` being non-empty.
    let mut claims = token_data.claims;
    if claims.roles.is_empty() {
        claims.roles = vec![claims.role.clone()];
    }

    Ok(claims)
}

impl From<JwtClaims> for crate::auth::middleware::Claims {
    fn from(j: JwtClaims) -> Self {
        // `decode_jwt` already normalised `roles` to be non-empty; guard here
        // too so a hand-built JwtClaims never yields an empty role set.
        let roles = if j.roles.is_empty() {
            vec![j.role.clone()]
        } else {
            j.roles
        };
        Self {
            user_id: j.sub,
            org_id: j.org_id,
            role: j.role,
            roles,
            user_code: j.user_code,
        }
    }
}
