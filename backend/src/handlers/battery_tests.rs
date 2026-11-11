// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

use axum::{
    extract::{Path, State},
    Json,
};

use crate::auth::middleware::{require_admin_or_technician, Claims};
use crate::db::DbPool;
use crate::errors::AppError;
use crate::models::battery_test::{BatteryTest, CreateBatteryTestRequest};
use crate::services::audit;

#[cfg(test)]
#[path = "battery_tests_tests.rs"]
mod tests;

const VALID_RESULTS: &[&str] = &["pass", "warning", "fail"];
const VALID_SWELLING: &[&str] = &["none", "minor", "severe"];

/// Confirm the battery exists in the caller's org. Returns 404 otherwise.
async fn ensure_battery_in_org(
    pool: &DbPool,
    org_id: i32,
    battery_id: i32,
) -> Result<(), AppError> {
    let exists = sqlx::query_scalar::<_, i32>(
        "SELECT id FROM batteries WHERE id = $1 AND org_id = $2",
    )
    .bind(battery_id)
    .bind(org_id)
    .fetch_optional(pool)
    .await?;
    if exists.is_none() {
        return Err(AppError::NotFound(format!("Battery {} not found", battery_id)));
    }
    Ok(())
}

/// GET /api/v1/batteries/:id/tests — List battery tests, newest first.
/// Any authenticated role.
pub async fn list_battery_tests(
    claims: Claims,
    State(pool): State<DbPool>,
    Path(battery_id): Path<i32>,
) -> Result<Json<Vec<BatteryTest>>, AppError> {
    ensure_battery_in_org(&pool, claims.org_id, battery_id).await?;

    let rows = sqlx::query_as::<_, BatteryTest>(
        "SELECT * FROM battery_tests WHERE battery_id = $1 AND org_id = $2 \
         ORDER BY test_date DESC, id DESC",
    )
    .bind(battery_id)
    .bind(claims.org_id)
    .fetch_all(&pool)
    .await?;

    Ok(Json(rows))
}

/// POST /api/v1/batteries/:id/tests — Log a new battery test.
/// Admin or technician only. `tested_by` and `test_type` are server-set.
pub async fn create_battery_test(
    claims: Claims,
    State(pool): State<DbPool>,
    Path(battery_id): Path<i32>,
    Json(body): Json<CreateBatteryTestRequest>,
) -> Result<Json<BatteryTest>, AppError> {
    require_admin_or_technician(&claims)?;

    if !VALID_RESULTS.contains(&body.result.as_str()) {
        return Err(AppError::BadRequest(
            "result must be one of: pass, warning, fail".into(),
        ));
    }

    let swelling_status = body.swelling_status.as_deref().unwrap_or("none");
    if !VALID_SWELLING.contains(&swelling_status) {
        return Err(AppError::BadRequest(
            "swelling_status must be one of: none, minor, severe".into(),
        ));
    }

    ensure_battery_in_org(&pool, claims.org_id, battery_id).await?;

    let cell_voltages = body.cell_voltages.as_ref().map(|v| serde_json::json!(v));

    let row = sqlx::query_as::<_, BatteryTest>(
        "INSERT INTO battery_tests \
         (org_id, battery_id, test_date, test_type, result, capacity_measured, \
          internal_resistance, swelling_status, cell_voltages, notes, tested_by) \
         VALUES ($1,$2,$3::date,'health',$4,$5,$6,$7,$8,$9,$10) RETURNING *",
    )
    .bind(claims.org_id)
    .bind(battery_id)
    .bind(body.test_date)
    .bind(&body.result)
    .bind(body.capacity_measured)
    .bind(body.internal_resistance)
    .bind(swelling_status)
    .bind(cell_voltages)
    .bind(&body.notes)
    .bind(&claims.user_code)
    .fetch_one(&pool)
    .await
    .map_err(|e| {
        tracing::error!("create_battery_test DB error: {:?}", e);
        AppError::Database(e)
    })?;

    audit::log_insert_full(
        &pool,
        claims.org_id,
        claims.user_id,
        &claims.user_code,
        "battery_tests",
        row.id,
        &row,
    )
    .await;

    Ok(Json(row))
}
