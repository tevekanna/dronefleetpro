// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

use axum::{
    extract::{Path, Query, State},
    Json,
};
use chrono::{DateTime, Duration, Utc};
use serde::Deserialize;
use sqlx::Row;

use crate::auth::middleware::{primary_role, require_admin, Claims};
use crate::db::DbPool;
use crate::errors::AppError;
use crate::services::{audit, user_tokens as token_service};

#[cfg(test)]
#[path = "users_tests.rs"]
mod tests;

const VALID_ROLES: &[&str] = &["admin", "pilot", "technician", "safety_officer", "viewer"];

#[derive(Debug, serde::Serialize, sqlx::FromRow)]
pub struct UserResponse {
    pub id: i32,
    pub org_id: i32,
    pub name: String,
    pub email: String,
    pub user_code: String,
    /// Primary role — denormalised mirror kept in sync with `roles` for the
    /// legacy single-role JWT claim and display.
    pub role: String,
    /// Full set of RBAC roles (source of truth lives in `user_roles`).
    pub roles: Vec<String>,
    pub is_active: bool,
    pub deactivated_at: Option<DateTime<Utc>>,
    pub pilot_id: Option<i32>,
    pub pilot_name: Option<String>,
    pub last_login_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    #[serde(default)]
    pub include_inactive: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateUserRequest {
    pub name: String,
    pub email: String,
    pub user_code: String,
    /// Single-role create — what the current single-select Add User modal
    /// sends. Promoted to a one-element role set. Kept until the frontend
    /// adopts the multi-role `roles` array in PR 2.
    pub role: Option<String>,
    /// Multi-role create (PR 2+). Takes precedence over `role` when both are
    /// present; both funnel through the same validation.
    pub roles: Option<Vec<String>>,
    pub pilot_id: Option<i32>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateUserRequest {
    pub name: Option<String>,
    pub email: Option<String>,
    pub user_code: Option<String>,
    /// Single-role update — what the current single-select Edit User modal
    /// sends. Replaces the user's entire role set with exactly this one role.
    /// Kept until the frontend adopts the multi-role `roles` array in PR 2.
    pub role: Option<String>,
    /// Multi-role update (PR 2+). Takes precedence over `role` when both are
    /// present; both funnel through the same validate-and-replace path.
    pub roles: Option<Vec<String>>,
    pub pilot_id: Option<Option<i32>>,
}

#[derive(Debug, serde::Serialize)]
pub struct CreateUserResponse {
    pub user: UserResponse,
    pub invitation_token: String,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, serde::Serialize)]
pub struct ResetTokenResponse {
    pub token: String,
    pub expires_at: DateTime<Utc>,
}

const USER_SELECT_WITH_PILOT: &str =
    "SELECT u.id, u.org_id, u.name, u.email, u.user_code, u.role, \
            COALESCE((SELECT array_agg(ur.role ORDER BY ur.role) \
                      FROM user_roles ur WHERE ur.user_id = u.id), ARRAY[]::text[]) AS roles, \
            u.is_active, u.deactivated_at, u.pilot_id, p.name AS pilot_name, \
            u.last_login_at, u.created_at \
     FROM users u LEFT JOIN pilots p ON p.id = u.pilot_id";

/// Returns the count of active admins in `org_id` other than `exclude_user_id`.
/// Admin status is determined by the `user_roles` join table (source of truth),
/// not the denormalised `users.role` mirror.
async fn count_other_active_admins(
    pool: &DbPool,
    org_id: i32,
    exclude_user_id: i32,
) -> Result<i64, AppError> {
    let row: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM users \
         WHERE org_id = $1 AND is_active = TRUE AND id != $2 \
           AND EXISTS (SELECT 1 FROM user_roles ur \
                       WHERE ur.user_id = users.id AND ur.role = 'admin')",
    )
    .bind(org_id)
    .bind(exclude_user_id)
    .fetch_one(pool)
    .await?;
    Ok(row.0)
}

/// Build an audit-friendly JSON of a user row INCLUDING password_hash, so the
/// scrubber can detect hash changes and surface them as `_scrubbed_changed`.
/// The hash is always stripped before storage.
async fn user_audit_snapshot(
    pool: &DbPool,
    org_id: i32,
    user_id: i32,
) -> Option<serde_json::Value> {
    let row = sqlx::query(
        "SELECT u.id, u.org_id, u.name, u.email, u.user_code, u.password_hash, \
                u.is_active, u.deactivated_at, u.pilot_id, u.last_login_at, u.created_at, \
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
    v.insert("deactivated_at".into(), serde_json::json!(row.try_get::<Option<DateTime<Utc>>, _>("deactivated_at").ok()?));
    v.insert("pilot_id".into(), serde_json::json!(row.try_get::<Option<i32>, _>("pilot_id").ok()?));
    v.insert("last_login_at".into(), serde_json::json!(row.try_get::<Option<DateTime<Utc>>, _>("last_login_at").ok()?));
    v.insert("created_at".into(), serde_json::json!(row.try_get::<DateTime<Utc>, _>("created_at").ok()?));
    Some(serde_json::Value::Object(v))
}

async fn fetch_user_response(
    pool: &DbPool,
    org_id: i32,
    user_id: i32,
) -> Result<UserResponse, AppError> {
    sqlx::query_as::<_, UserResponse>(
        &format!("{USER_SELECT_WITH_PILOT} WHERE u.id = $1 AND u.org_id = $2"),
    )
    .bind(user_id)
    .bind(org_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("User {user_id} not found")))
}

/// GET /api/v1/users — admin-only.
/// Default: only active users. `?include_inactive=true` returns all users.
pub async fn list(
    claims: Claims,
    State(pool): State<DbPool>,
    Query(q): Query<ListQuery>,
) -> Result<Json<Vec<UserResponse>>, AppError> {
    require_admin(&claims)?;

    let sql = if q.include_inactive {
        format!(
            "{USER_SELECT_WITH_PILOT} WHERE u.org_id = $1 \
             ORDER BY u.is_active DESC, u.name ASC"
        )
    } else {
        format!(
            "{USER_SELECT_WITH_PILOT} WHERE u.org_id = $1 AND u.is_active = TRUE \
             ORDER BY u.name ASC"
        )
    };

    let rows = sqlx::query_as::<_, UserResponse>(&sql)
        .bind(claims.org_id)
        .fetch_all(&pool)
        .await?;

    Ok(Json(rows))
}

fn validate_user_code(raw: &str) -> Result<String, AppError> {
    let uc = raw.trim().to_uppercase();
    if uc.len() != 3 || !uc.chars().all(|c| c.is_ascii_uppercase()) {
        return Err(AppError::BadRequest(
            "User code must be exactly 3 uppercase letters (A-Z)".into(),
        ));
    }
    Ok(uc)
}

/// Validate and normalise a requested role set: must be non-empty, every entry
/// must be a known role, and duplicates are silently de-duplicated (preserving
/// first-seen order). Returns the cleaned set.
fn validate_roles(roles: &[String]) -> Result<Vec<String>, AppError> {
    if roles.is_empty() {
        return Err(AppError::BadRequest(
            "At least one role is required".into(),
        ));
    }
    let mut cleaned: Vec<String> = Vec::with_capacity(roles.len());
    for raw in roles {
        let role = raw.trim();
        if !VALID_ROLES.contains(&role) {
            return Err(AppError::BadRequest(format!(
                "Invalid role '{role}'. Must be one of: admin, pilot, technician, safety_officer, viewer"
            )));
        }
        if !cleaned.iter().any(|r| r == role) {
            cleaned.push(role.to_string());
        }
    }
    Ok(cleaned)
}

/// POST /api/v1/users — admin-only.
/// Creates a deactivated-style account (technically active, but with a random
/// throwaway password_hash) and issues an `invitation` token returned once.
pub async fn create(
    claims: Claims,
    State(pool): State<DbPool>,
    Json(body): Json<CreateUserRequest>,
) -> Result<Json<CreateUserResponse>, AppError> {
    require_admin(&claims)?;

    if body.name.trim().is_empty() {
        return Err(AppError::BadRequest("Name is required".into()));
    }
    if !body.email.contains('@') {
        return Err(AppError::BadRequest("Invalid email address".into()));
    }
    let user_code = validate_user_code(&body.user_code)?;

    // Accept either the single-role `role` field (current single-select modal)
    // or the multi-role `roles` array (PR 2+). `roles` wins when both present;
    // a lone `role` is promoted to a one-element set. Neither present is a clean
    // 400 rather than an opaque serde-level failure.
    let requested_roles: Vec<String> = match (&body.roles, &body.role) {
        (Some(r), _) => r.clone(),
        (None, Some(single)) => vec![single.clone()],
        (None, None) => {
            return Err(AppError::BadRequest(
                "A user must be created with at least one role".into(),
            ));
        }
    };
    let roles = validate_roles(&requested_roles)?;
    let primary = primary_role(&roles);

    // Throwaway password — the user must redeem the invitation token to set a
    // real one. We hash a random 32-byte value so login attempts before
    // redemption simply fail.
    let placeholder = token_service::generate_token();
    let placeholder_hash = bcrypt::hash(&placeholder, 12)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("Password hashing failed: {e}")))?;

    let mut tx = pool.begin().await?;

    // `users.role` is kept populated with the primary role for backwards
    // compatibility; `user_roles` (inserted below) is the source of truth.
    let user_id: i32 = sqlx::query(
        "INSERT INTO users (org_id, name, email, user_code, role, password_hash, is_active, pilot_id) \
         VALUES ($1, $2, $3, $4, $5, $6, TRUE, $7) RETURNING id",
    )
    .bind(claims.org_id)
    .bind(body.name.trim())
    .bind(body.email.trim())
    .bind(&user_code)
    .bind(&primary)
    .bind(&placeholder_hash)
    .bind(body.pilot_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| {
        let msg = e.to_string();
        if msg.contains("email") {
            AppError::Conflict(format!(
                "A user with email '{}' already exists in this organisation",
                body.email
            ))
        } else if msg.contains("user_code") {
            AppError::Conflict(format!(
                "User code '{user_code}' is already taken in this organisation"
            ))
        } else {
            AppError::Database(e)
        }
    })?
    .get("id");

    // Insert the full role set in the same transaction.
    for role in &roles {
        sqlx::query(
            "INSERT INTO user_roles (org_id, user_id, role, granted_by) \
             VALUES ($1, $2, $3, $4)",
        )
        .bind(claims.org_id)
        .bind(user_id)
        .bind(role)
        .bind(claims.user_id)
        .execute(&mut *tx)
        .await?;
    }

    let token_plain = token_service::generate_token();
    let token_hash = token_service::hash_token(&token_plain)?;
    let expires_at = Utc::now() + Duration::hours(token_service::TOKEN_TTL_HOURS);

    let token_id: i64 = sqlx::query(
        "INSERT INTO user_tokens (org_id, user_id, purpose, token_hash, issued_by, expires_at) \
         VALUES ($1, $2, 'invitation', $3, $4, $5) RETURNING id",
    )
    .bind(claims.org_id)
    .bind(user_id)
    .bind(&token_hash)
    .bind(claims.user_id)
    .bind(expires_at)
    .fetch_one(&mut *tx)
    .await?
    .get("id");

    tx.commit().await?;

    let user = fetch_user_response(&pool, claims.org_id, user_id).await?;

    audit::log_insert_full(
        &pool,
        claims.org_id,
        claims.user_id,
        &claims.user_code,
        "users",
        user.id,
        &user,
    )
    .await;
    audit::log_insert_full(
        &pool,
        claims.org_id,
        claims.user_id,
        &claims.user_code,
        "user_tokens",
        token_id as i32,
        &serde_json::json!({
            "user_id": user_id,
            "purpose": "invitation",
            "expires_at": expires_at,
            "token_hash": token_hash,
        }),
    )
    .await;

    Ok(Json(CreateUserResponse {
        user,
        invitation_token: token_plain,
        expires_at,
    }))
}

/// PUT /api/v1/users/:id — admin-only.
pub async fn update(
    claims: Claims,
    State(pool): State<DbPool>,
    Path(id): Path<i32>,
    Json(body): Json<UpdateUserRequest>,
) -> Result<Json<UserResponse>, AppError> {
    require_admin(&claims)?;

    // Accept either the single-role `role` field (current single-select modal)
    // or the multi-role `roles` array (PR 2+). `roles` wins when both are
    // present; a lone `role` is promoted to a one-element set. Both then flow
    // through the same validation and full-replacement path below, so a
    // single-role edit REPLACES the user's entire role set with that one role.
    let requested_roles: Option<Vec<String>> = match (&body.roles, &body.role) {
        (Some(r), _) => Some(r.clone()),
        (None, Some(single)) => Some(vec![single.clone()]),
        (None, None) => None,
    };

    // Validate the requested role set (if any) up front and compute the new
    // primary role for the denormalised `users.role` mirror.
    let roles_update: Option<Vec<String>> = match &requested_roles {
        Some(r) => Some(validate_roles(r)?),
        None => None,
    };
    let new_primary: Option<String> = roles_update.as_ref().map(|r| primary_role(r));

    // Self-protection: an admin may freely change their other roles, but may
    // not strip their own admin role (that's how you'd lock yourself out).
    if claims.user_id == id {
        if let Some(ref new_roles) = roles_update {
            if claims.has_role("admin") && !new_roles.iter().any(|r| r == "admin") {
                return Err(AppError::Conflict(
                    "You cannot remove admin from your own role set".into(),
                ));
            }
        }
    }

    // Last-admin guard: removing admin from the role set is rejected if the
    // target is the org's only remaining active admin.
    if let Some(ref new_roles) = roles_update {
        if !new_roles.iter().any(|r| r == "admin") {
            let target: Option<(bool, bool)> = sqlx::query_as(
                "SELECT u.is_active, \
                        EXISTS (SELECT 1 FROM user_roles ur \
                                WHERE ur.user_id = u.id AND ur.role = 'admin') \
                 FROM users u WHERE u.id = $1 AND u.org_id = $2",
            )
            .bind(id)
            .bind(claims.org_id)
            .fetch_optional(&pool)
            .await?;
            if let Some((is_active, has_admin)) = target {
                if has_admin
                    && is_active
                    && count_other_active_admins(&pool, claims.org_id, id).await? < 1
                {
                    return Err(AppError::Conflict(
                        "Cannot demote the last active admin in this organization.".into(),
                    ));
                }
            }
        }
    }

    if let Some(ref name) = body.name {
        if name.trim().is_empty() {
            return Err(AppError::BadRequest("Name cannot be empty".into()));
        }
    }
    if let Some(ref email) = body.email {
        if !email.contains('@') {
            return Err(AppError::BadRequest("Invalid email address".into()));
        }
    }
    let user_code_upper = if let Some(ref code) = body.user_code {
        Some(validate_user_code(code)?)
    } else {
        None
    };

    let pilot_id_provided = body.pilot_id.is_some();
    let pilot_id_value = body.pilot_id.unwrap_or(None);

    let before_audit = user_audit_snapshot(&pool, claims.org_id, id).await;

    let mut tx = pool.begin().await?;

    // `users.role` is updated to the new primary only when a role set was
    // provided; otherwise COALESCE($4, role) leaves it untouched.
    sqlx::query(
        "UPDATE users SET \
            name      = COALESCE($1, name), \
            email     = COALESCE($2, email), \
            user_code = COALESCE($3, user_code), \
            role      = COALESCE($4, role), \
            pilot_id  = CASE WHEN $5 THEN $6 ELSE pilot_id END \
         WHERE id = $7 AND org_id = $8",
    )
    .bind(body.name.as_ref().map(|s| s.trim().to_string()))
    .bind(body.email.as_ref().map(|s| s.trim().to_string()))
    .bind(user_code_upper)
    .bind(new_primary.as_deref())
    .bind(pilot_id_provided)
    .bind(pilot_id_value)
    .bind(id)
    .bind(claims.org_id)
    .execute(&mut *tx)
    .await?;

    // Replace the role set transactionally when provided: source of truth is
    // user_roles, so we clear and re-insert rather than diff.
    if let Some(ref new_roles) = roles_update {
        sqlx::query("DELETE FROM user_roles WHERE user_id = $1 AND org_id = $2")
            .bind(id)
            .bind(claims.org_id)
            .execute(&mut *tx)
            .await?;
        for role in new_roles {
            sqlx::query(
                "INSERT INTO user_roles (org_id, user_id, role, granted_by) \
                 VALUES ($1, $2, $3, $4)",
            )
            .bind(claims.org_id)
            .bind(id)
            .bind(role)
            .bind(claims.user_id)
            .execute(&mut *tx)
            .await?;
        }
    }

    tx.commit().await?;

    let user = fetch_user_response(&pool, claims.org_id, id).await?;

    let after_audit = user_audit_snapshot(&pool, claims.org_id, id).await;
    if let (Some(b), Some(a)) = (before_audit, after_audit) {
        audit::log_update_diff(
            &pool,
            claims.org_id,
            claims.user_id,
            &claims.user_code,
            "users",
            user.id,
            &b,
            &a,
        )
        .await;
    } else {
        tracing::warn!("audit: user {} snapshot unavailable, skipping row", user.id);
    }

    Ok(Json(user))
}

/// DELETE /api/v1/users/:id — soft-deactivate (admin-only).
pub async fn deactivate(
    claims: Claims,
    State(pool): State<DbPool>,
    Path(id): Path<i32>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_admin(&claims)?;

    if claims.user_id == id {
        return Err(AppError::Conflict(
            "You cannot deactivate your own account".into(),
        ));
    }

    let target: Option<(bool, bool)> = sqlx::query_as(
        "SELECT u.is_active, \
                EXISTS (SELECT 1 FROM user_roles ur \
                        WHERE ur.user_id = u.id AND ur.role = 'admin') \
         FROM users u WHERE u.id = $1 AND u.org_id = $2",
    )
    .bind(id)
    .bind(claims.org_id)
    .fetch_optional(&pool)
    .await?;
    let (target_active, target_has_admin) =
        target.ok_or_else(|| AppError::NotFound(format!("User {id} not found")))?;

    if !target_active {
        return Err(AppError::Conflict("User is already deactivated".into()));
    }
    if target_has_admin && count_other_active_admins(&pool, claims.org_id, id).await? < 1 {
        return Err(AppError::Conflict(
            "Cannot deactivate the last active admin in this organization.".into(),
        ));
    }

    let before = user_audit_snapshot(&pool, claims.org_id, id).await;

    sqlx::query(
        "UPDATE users SET is_active = FALSE, deactivated_at = NOW() \
         WHERE id = $1 AND org_id = $2",
    )
    .bind(id)
    .bind(claims.org_id)
    .execute(&pool)
    .await?;

    let after = user_audit_snapshot(&pool, claims.org_id, id).await;
    if let (Some(b), Some(a)) = (before, after) {
        audit::log_update_diff(
            &pool,
            claims.org_id,
            claims.user_id,
            &claims.user_code,
            "users",
            id,
            &b,
            &a,
        )
        .await;
    }

    Ok(Json(serde_json::json!({ "message": "User deactivated", "id": id })))
}

/// POST /api/v1/users/:id/reactivate — admin-only.
pub async fn reactivate(
    claims: Claims,
    State(pool): State<DbPool>,
    Path(id): Path<i32>,
) -> Result<Json<UserResponse>, AppError> {
    require_admin(&claims)?;

    let target: Option<(bool,)> =
        sqlx::query_as("SELECT is_active FROM users WHERE id = $1 AND org_id = $2")
            .bind(id)
            .bind(claims.org_id)
            .fetch_optional(&pool)
            .await?;
    let (is_active,) = target.ok_or_else(|| AppError::NotFound(format!("User {id} not found")))?;
    if is_active {
        return Err(AppError::Conflict("User is already active".into()));
    }

    let before = user_audit_snapshot(&pool, claims.org_id, id).await;
    sqlx::query(
        "UPDATE users SET is_active = TRUE, deactivated_at = NULL \
         WHERE id = $1 AND org_id = $2",
    )
    .bind(id)
    .bind(claims.org_id)
    .execute(&pool)
    .await?;
    let after = user_audit_snapshot(&pool, claims.org_id, id).await;
    if let (Some(b), Some(a)) = (before, after) {
        audit::log_update_diff(
            &pool,
            claims.org_id,
            claims.user_id,
            &claims.user_code,
            "users",
            id,
            &b,
            &a,
        )
        .await;
    }

    let user = fetch_user_response(&pool, claims.org_id, id).await?;
    Ok(Json(user))
}

/// POST /api/v1/users/:id/reset-token — admin-only.
/// Invalidates any unconsumed reset tokens for the user, then issues a new one.
/// Plaintext token returned once.
pub async fn issue_reset_token(
    claims: Claims,
    State(pool): State<DbPool>,
    Path(id): Path<i32>,
) -> Result<Json<ResetTokenResponse>, AppError> {
    require_admin(&claims)?;

    let exists: Option<(bool,)> =
        sqlx::query_as("SELECT is_active FROM users WHERE id = $1 AND org_id = $2")
            .bind(id)
            .bind(claims.org_id)
            .fetch_optional(&pool)
            .await?;
    let (is_active,) = exists.ok_or_else(|| AppError::NotFound(format!("User {id} not found")))?;
    if !is_active {
        return Err(AppError::Conflict(
            "Reactivate the user before issuing a password reset.".into(),
        ));
    }

    let mut tx = pool.begin().await?;
    let issued = token_service::issue_password_reset_token(
        &mut tx,
        claims.org_id,
        id,
        Some(claims.user_id),
    )
    .await?;
    tx.commit().await?;

    audit::log_insert_full(
        &pool,
        claims.org_id,
        claims.user_id,
        &claims.user_code,
        "user_tokens",
        issued.id as i32,
        &serde_json::json!({
            "user_id": id,
            "purpose": "password_reset",
            "expires_at": issued.expires_at,
            "token_hash": issued.token_hash,
        }),
    )
    .await;

    Ok(Json(ResetTokenResponse {
        token: issued.plaintext,
        expires_at: issued.expires_at,
    }))
}
