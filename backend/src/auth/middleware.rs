// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

use async_trait::async_trait;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use crate::errors::AppError;

#[derive(Debug, Clone)]
pub struct Claims {
    pub user_id: i32,
    pub org_id: i32,
    /// Primary role — used only for the legacy single-role JWT claim and for
    /// display. Authorisation decisions go through `roles` / `has_role`.
    pub role: String,
    /// Full set of RBAC roles held by the user. Always non-empty (normalised
    /// at decode time from a legacy single `role` when needed).
    pub roles: Vec<String>,
    pub user_code: String,
}

impl Claims {
    /// True if the user holds the given role.
    pub fn has_role(&self, r: &str) -> bool {
        self.roles.iter().any(|x| x == r)
    }

    /// True if the user holds any of the given roles.
    pub fn has_any(&self, allowed: &[&str]) -> bool {
        allowed.iter().any(|r| self.has_role(r))
    }
}

/// Role precedence for picking the single "primary" role from a multi-role
/// set: admin > technician > safety_officer > pilot > viewer. Used to mint the
/// legacy `role` JWT claim and to keep `users.role` in sync. Shared by the auth
/// and users handlers so the choice stays consistent.
pub fn primary_role(roles: &[String]) -> String {
    const PRECEDENCE: &[&str] = &["admin", "technician", "safety_officer", "pilot", "viewer"];
    for candidate in PRECEDENCE {
        if roles.iter().any(|r| r == candidate) {
            return (*candidate).to_string();
        }
    }
    // Defensive: an unrecognised or empty set falls back to the first entry,
    // or to the most-restrictive role if the set is empty.
    roles
        .first()
        .cloned()
        .unwrap_or_else(|| "viewer".to_string())
}

/// Require the caller to hold the `admin` role. Returns `Forbidden` otherwise.
pub fn require_admin(claims: &Claims) -> Result<(), AppError> {
    if !claims.has_role("admin") {
        return Err(AppError::Forbidden("Admin role required".into()));
    }
    Ok(())
}

/// Require the caller to hold either the `admin` or `technician` role.
/// Returns `Forbidden` otherwise.
pub fn require_admin_or_technician(claims: &Claims) -> Result<(), AppError> {
    if !claims.has_any(&["admin", "technician"]) {
        return Err(AppError::Forbidden(
            "Admin or technician role required".into(),
        ));
    }
    Ok(())
}

/// Allow if the caller holds any of the provided roles. 403 otherwise.
pub fn require_role(claims: &Claims, allowed: &[&str]) -> Result<(), AppError> {
    if claims.has_any(allowed) {
        Ok(())
    } else {
        Err(AppError::Forbidden(format!(
            "Requires one of roles: {} (your roles: {})",
            allowed.join(", "),
            claims.roles.join(", ")
        )))
    }
}

/// Allow if the caller holds any of the provided roles, OR if
/// `claims.user_id == owner_user_id`. Used for "own only" patterns where
/// pilots/technicians can edit their own data but not others'.
pub fn require_self_or_role(
    claims: &Claims,
    owner_user_id: Option<i32>,
    allowed: &[&str],
) -> Result<(), AppError> {
    if claims.has_any(allowed) {
        return Ok(());
    }
    if let Some(owner) = owner_user_id {
        if claims.user_id == owner {
            return Ok(());
        }
    }
    Err(AppError::Forbidden(format!(
        "Requires ownership or one of roles: {} (your roles: {})",
        allowed.join(", "),
        claims.roles.join(", ")
    )))
}

#[async_trait]
impl<S> FromRequestParts<S> for Claims
where
    S: Send + Sync,
{
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        use axum::http::header::AUTHORIZATION;

        let auth_header = parts
            .headers
            .get(AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| AppError::Unauthorized("Missing Authorization header".into()))?;

        let token = auth_header
            .strip_prefix("Bearer ")
            .ok_or_else(|| AppError::Unauthorized("Invalid Authorization header format".into()))?;

        let jwt_secret = std::env::var("JWT_SECRET")
            .unwrap_or_else(|_| "dev_secret".into());

        let jwt_claims = crate::auth::jwt::decode_jwt(token, &jwt_secret)
            .map_err(|_| AppError::Unauthorized("Invalid or expired token".into()))?;

        // `decode_jwt` guarantees `roles` is non-empty (legacy tokens are
        // normalised to `vec![role]`); guard again for safety.
        let roles = if jwt_claims.roles.is_empty() {
            vec![jwt_claims.role.clone()]
        } else {
            jwt_claims.roles
        };

        Ok(Claims {
            user_id: jwt_claims.sub,
            org_id: jwt_claims.org_id,
            role: jwt_claims.role,
            roles,
            user_code: jwt_claims.user_code,
        })
    }
}
