// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

use crate::auth::middleware::Claims;
use crate::db::DbPool;
use crate::errors::AppError;

/// For pilot-role users, resolve their associated pilot_id.
///
/// Returns:
/// - `Ok(pilot_id)` if the user is pilot-role and has `users.pilot_id` set
/// - `Err(Forbidden)` if the user is pilot-role but `pilot_id` is NULL (admin
///   must associate them via the user-management UI)
/// - `Err(Database)` on DB error
///
/// Should NOT be called for non-pilot roles — those have unrestricted access
/// at the row level under the matrix.
pub async fn resolve_pilot_id_for_self(
    pool: &DbPool,
    claims: &Claims,
) -> Result<i32, AppError> {
    let row: Option<(Option<i32>,)> = sqlx::query_as(
        "SELECT pilot_id FROM users WHERE id = $1 AND org_id = $2"
    )
    .bind(claims.user_id)
    .bind(claims.org_id)
    .fetch_optional(pool)
    .await
    .map_err(AppError::Database)?;

    match row {
        Some((Some(pilot_id),)) => Ok(pilot_id),
        Some((None,)) => Err(AppError::Forbidden(
            "User is registered as pilot but not associated with a pilot record. \
             Contact your administrator to link your account."
                .to_string(),
        )),
        None => Err(AppError::Forbidden(
            "User not found in current organisation".to_string(),
        )),
    }
}
