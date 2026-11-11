// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde::Deserialize;
use serde_json::json;

use crate::auth::jwt::{encode_jwt, JwtClaims};
use crate::auth::middleware::primary_role;
use crate::db::DbPool;
use crate::errors::AppError;
use crate::models::user::LoginRequest;
use crate::services::{
    audit,
    rate_limit::{self, RateLimitError, RateLimitScope},
    user_tokens::MIN_PASSWORD_LEN,
};

/// POST /api/v1/auth/login — Authenticate user and return JWT.
///
/// Writes a LOGIN_SUCCESS or LOGIN_FAILURE audit row on every attempt
/// before returning. The audit row carries the originating client IP via
/// the task-local set by `services::client_ip::capture_client_ip_middleware`
/// so brute-force or compromised-credential investigations can trace
/// attempts by source.
pub async fn login(
    State(pool): State<DbPool>,
    Json(body): Json<LoginRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    // Per-username rate limit (second layer; per-IP via tower_governor runs
    // first in main.rs). Identifier is the lowercase trimmed email so case
    // and whitespace can't be used to dodge the counter.
    let identifier = body.email.trim().to_lowercase();
    match rate_limit::check_and_record(&pool, RateLimitScope::Login, &identifier).await {
        Ok(()) => {}
        Err(RateLimitError::Locked {
            retry_after_seconds,
        }) => {
            rate_limit::audit_lockout(
                &pool,
                RateLimitScope::Login,
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

    // Look up user by email
    let maybe_user = sqlx::query_as::<_, UserRow>(
        "SELECT id, org_id, name, email, user_code, password_hash, role, is_active FROM users WHERE email = $1"
    )
    .bind(&body.email)
    .fetch_optional(&pool)
    .await
    .map_err(|e| {
        tracing::error!("DB query error: {:?}", e);
        AppError::Database(e)
    })?;

    let user = match maybe_user {
        Some(u) => u,
        None => {
            // Unknown email: record with NULL org/user so the row exists for
            // forensic inspection but doesn't surface in any tenant's audit
            // viewer. Email is recorded so brute-force enumeration attempts
            // can be detected at the system level.
            audit::log_auth_event(
                &pool,
                None,
                None,
                "?",
                "LOGIN_FAILURE",
                "users",
                None,
                Some(json!({
                    "attempted_email": body.email,
                    "reason": "unknown_email",
                })),
            )
            .await;
            return Err(AppError::Unauthorized("Invalid email or password".into()));
        }
    };

    // Deactivated account: same generic 401 to the caller, but record the
    // real reason for the audit log so admins can spot lockouts.
    if !user.is_active {
        audit::log_auth_event(
            &pool,
            Some(user.org_id),
            Some(user.id),
            &user.user_code,
            "LOGIN_FAILURE",
            "users",
            Some(user.id),
            Some(json!({ "reason": "deactivated" })),
        )
        .await;
        return Err(AppError::Unauthorized("Account is disabled".into()));
    }

    // Verify password. A stored hash that is not a valid bcrypt string —
    // notably the `!bootstrap-pending` marker written by
    // `services::admin_bootstrap` — can never match any password, so a
    // bcrypt error is a failed match and takes the same generic 401 and
    // LOGIN_FAILURE audit path as a wrong password. It must never surface
    // as a 500 or a distinguishable message.
    let password_ok = match bcrypt::verify(&body.password, &user.password_hash) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!(
                "stored password hash for user {} is not verifiable ({:?}); treating as a failed match",
                user.id,
                e
            );
            false
        }
    };

    if !password_ok {
        audit::log_auth_event(
            &pool,
            Some(user.org_id),
            Some(user.id),
            &user.user_code,
            "LOGIN_FAILURE",
            "users",
            Some(user.id),
            Some(json!({ "reason": "bad_password" })),
        )
        .await;
        return Err(AppError::Unauthorized("Invalid email or password".into()));
    }

    // Load the user's full role set. Defensive fallback to the denormalised
    // primary role on the users row if (somehow) no user_roles rows exist.
    let mut roles: Vec<String> = sqlx::query_scalar::<_, String>(
        "SELECT role FROM user_roles WHERE user_id = $1 ORDER BY role",
    )
    .bind(user.id)
    .fetch_all(&pool)
    .await
    .map_err(AppError::Database)?;
    if roles.is_empty() {
        roles = vec![user.role.clone()];
    }
    let primary = primary_role(&roles);

    // Build JWT claims (8 hour expiry). `role` carries the primary role for
    // backwards compatibility; `roles` carries the full set.
    let exp = chrono::Utc::now().timestamp() + (8 * 3600);
    let claims = JwtClaims {
        sub: user.id,
        org_id: user.org_id,
        role: primary.clone(),
        roles: roles.clone(),
        user_code: user.user_code.clone(),
        exp,
    };

    let jwt_secret = std::env::var("JWT_SECRET")
        .unwrap_or_else(|_| "dev_secret".into());

    let token = encode_jwt(&claims, &jwt_secret).map_err(|e| {
        tracing::error!("JWT encode error: {:?}", e);
        AppError::Internal(anyhow::anyhow!("Token generation failed"))
    })?;

    // Best-effort: stamp last_login_at. Failure here mustn't block login.
    if let Err(e) =
        sqlx::query("UPDATE users SET last_login_at = NOW() WHERE id = $1")
            .bind(user.id)
            .execute(&pool)
            .await
    {
        tracing::warn!("failed to stamp last_login_at for user {}: {:?}", user.id, e);
    }

    audit::log_auth_event(
        &pool,
        Some(user.org_id),
        Some(user.id),
        &user.user_code,
        "LOGIN_SUCCESS",
        "users",
        Some(user.id),
        None,
    )
    .await;

    Ok(Json(json!({
        "token": token,
        "user": {
            "id": user.id,
            "name": user.name,
            "email": user.email,
            "role": primary,
            "roles": roles,
            "user_code": user.user_code,
            "org_id": user.org_id
        }
    })))
}

/// Internal struct for the login query (not exposed via API).
#[derive(Debug, sqlx::FromRow)]
struct UserRow {
    pub id: i32,
    pub org_id: i32,
    pub name: String,
    pub email: String,
    pub user_code: String,
    pub password_hash: String,
    pub role: String,
    pub is_active: bool,
}

pub async fn refresh() -> Result<Json<serde_json::Value>, AppError> {
    Ok(Json(json!({ "message": "Refresh endpoint — not yet implemented" })))
}

/// POST /api/v1/auth/logout — Authenticated.
///
/// JWTs are stateless, so logout is just "the client deletes the token".
/// We still take the request to write a LOGOUT audit row so admins can see
/// session lifecycle. Best-effort: never fails the response — even if the
/// audit insert fails the client still gets 204 and clears its token.
pub async fn logout(
    claims: crate::auth::middleware::Claims,
    State(pool): State<DbPool>,
) -> Result<impl IntoResponse, AppError> {
    audit::log_auth_event(
        &pool,
        Some(claims.org_id),
        Some(claims.user_id),
        &claims.user_code,
        "LOGOUT",
        "users",
        Some(claims.user_id),
        None,
    )
    .await;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn me(
    claims: crate::auth::middleware::Claims,
    State(pool): State<DbPool>,
) -> Result<Json<serde_json::Value>, AppError> {
    let user = sqlx::query_as::<_, MeRow>(
        "SELECT u.id, u.org_id, u.name, u.email, u.user_code, u.role, u.is_active, u.pilot_id, u.created_at, \
                COALESCE((SELECT array_agg(ur.role ORDER BY ur.role) \
                          FROM user_roles ur WHERE ur.user_id = u.id), ARRAY[]::text[]) AS roles \
         FROM users u WHERE u.id = $1 AND u.org_id = $2"
    )
    .bind(claims.user_id)
    .bind(claims.org_id)
    .fetch_optional(&pool)
    .await?
    .ok_or_else(|| AppError::NotFound("User not found".into()))?;

    // Defensive: surface the primary role even if the role set is somehow empty.
    let roles = if user.roles.is_empty() {
        vec![user.role.clone()]
    } else {
        user.roles.clone()
    };
    let primary = primary_role(&roles);

    Ok(Json(json!({
        "id": user.id,
        "org_id": user.org_id,
        "name": user.name,
        "email": user.email,
        "user_code": user.user_code,
        "role": primary,
        "roles": roles,
        "is_active": user.is_active,
        "pilot_id": user.pilot_id,
        "created_at": user.created_at
    })))
}

#[derive(Debug, sqlx::FromRow)]
struct MeRow {
    pub id: i32,
    pub org_id: i32,
    pub name: String,
    pub email: String,
    pub user_code: String,
    pub role: String,
    pub roles: Vec<String>,
    pub is_active: bool,
    pub pilot_id: Option<i32>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize)]
pub struct ChangePasswordRequest {
    pub current_password: String,
    pub new_password: String,
}

/// Snapshot a user row including password_hash so the audit scrubber can emit
/// `_scrubbed_changed: ["password_hash"]` when the hash differs.
async fn user_audit_snapshot_self(
    pool: &DbPool,
    org_id: i32,
    user_id: i32,
) -> Option<serde_json::Value> {
    use sqlx::Row;
    let row = sqlx::query(
        "SELECT u.id, u.org_id, u.name, u.email, u.user_code, u.password_hash, u.is_active, \
                COALESCE((SELECT array_agg(ur.role ORDER BY ur.role) \
                          FROM user_roles ur WHERE ur.user_id = u.id), ARRAY[]::text[]) AS roles \
         FROM users u WHERE u.id = $1 AND u.org_id = $2",
    )
    .bind(user_id)
    .bind(org_id)
    .fetch_optional(pool)
    .await
    .ok()
    .flatten()?;
    let mut v = serde_json::Map::new();
    v.insert("id".into(), serde_json::json!(row.try_get::<i32, _>("id").ok()?));
    v.insert("org_id".into(), serde_json::json!(row.try_get::<i32, _>("org_id").ok()?));
    v.insert("name".into(), serde_json::json!(row.try_get::<String, _>("name").ok()?));
    v.insert("email".into(), serde_json::json!(row.try_get::<String, _>("email").ok()?));
    v.insert("user_code".into(), serde_json::json!(row.try_get::<String, _>("user_code").ok()?));
    v.insert("password_hash".into(), serde_json::json!(row.try_get::<String, _>("password_hash").ok()?));
    v.insert("roles".into(), serde_json::json!(row.try_get::<Vec<String>, _>("roles").ok()?));
    v.insert("is_active".into(), serde_json::json!(row.try_get::<bool, _>("is_active").ok()?));
    Some(serde_json::Value::Object(v))
}

/// POST /api/v1/auth/change-password — any logged-in user.
/// Requires the user's current password. Generic "incorrect password" if it
/// doesn't match, so callers can't probe for the right password via timing.
pub async fn change_password(
    claims: crate::auth::middleware::Claims,
    State(pool): State<DbPool>,
    Json(body): Json<ChangePasswordRequest>,
) -> Result<impl IntoResponse, AppError> {
    // Per-username rate limit before any bcrypt work. Keyed on the
    // authenticated user, not the password — so a stolen JWT can't be used
    // to brute-force the current password.
    let identifier = format!("user:{}", claims.user_id);
    match rate_limit::check_and_record(&pool, RateLimitScope::ChangePassword, &identifier).await {
        Ok(()) => {}
        Err(RateLimitError::Locked {
            retry_after_seconds,
        }) => {
            rate_limit::audit_lockout(
                &pool,
                RateLimitScope::ChangePassword,
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

    if body.new_password.len() < MIN_PASSWORD_LEN {
        return Err(AppError::BadRequest(format!(
            "Password must be at least {MIN_PASSWORD_LEN} characters"
        )));
    }

    let user = sqlx::query_as::<_, UserRow>(
        "SELECT id, org_id, name, email, user_code, password_hash, role, is_active \
         FROM users WHERE id = $1 AND org_id = $2",
    )
    .bind(claims.user_id)
    .bind(claims.org_id)
    .fetch_optional(&pool)
    .await?
    .ok_or_else(|| AppError::Unauthorized("Invalid session".into()))?;

    if !user.is_active {
        return Err(AppError::Unauthorized("Account is disabled".into()));
    }

    // As in `login`: an unverifiable stored hash is a failed match, not a 500.
    match bcrypt::verify(&body.current_password, &user.password_hash) {
        Ok(true) => {}
        Ok(false) => return Err(AppError::BadRequest("Incorrect current password".into())),
        Err(e) => {
            tracing::warn!(
                "stored password hash for user {} is not verifiable ({:?}); treating as a failed match",
                user.id,
                e
            );
            return Err(AppError::BadRequest("Incorrect current password".into()));
        }
    }

    let new_hash = bcrypt::hash(&body.new_password, 12)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Password hashing failed: {e}")))?;

    let before = user_audit_snapshot_self(&pool, claims.org_id, claims.user_id).await;

    sqlx::query("UPDATE users SET password_hash = $1 WHERE id = $2 AND org_id = $3")
        .bind(&new_hash)
        .bind(claims.user_id)
        .bind(claims.org_id)
        .execute(&pool)
        .await?;

    let after = user_audit_snapshot_self(&pool, claims.org_id, claims.user_id).await;
    if let (Some(b), Some(a)) = (before, after) {
        audit::log_update_diff(
            &pool,
            claims.org_id,
            claims.user_id,
            &claims.user_code,
            "users",
            claims.user_id,
            &b,
            &a,
        )
        .await;
    }

    Ok(StatusCode::NO_CONTENT)
}
