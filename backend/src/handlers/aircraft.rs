// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

use axum::{
    extract::{Path, Query, State},
    Json,
};
use crate::auth::middleware::{require_admin, require_admin_or_technician, Claims};
use crate::db::DbPool;
use crate::errors::AppError;
use crate::models::aircraft::{Aircraft, CreateAircraftRequest, UpdateAircraftRequest};
use crate::services::aircraft_merge::{merge_aircraft, MergeOutcome};
use crate::services::aircraft_models::resolve_by_name_ci;
use crate::services::audit;
use crate::services::maintenance_defaults::insert_defaults_for_aircraft;

#[derive(serde::Deserialize, Default)]
pub struct FleetQueryParams {
    pub include_retired: Option<bool>,
}

/// Request body for `PATCH /api/v1/aircraft/:id/airworthiness`.
/// `airworthiness_status` must be `"airworthy"` or `"not_airworthy"`.
/// `comment` is mandatory and captured in the audit diff envelope so the
/// EASA audit trail records the reason for the status change.
#[derive(Debug, serde::Deserialize)]
pub struct PatchAirworthinessRequest {
    pub airworthiness_status: String,
    pub comment: String,
}

/// Canonical SELECT list for aircraft queries. `am.name AS model` keeps
/// existing callers that read `.model` working even though the column was
/// moved to the catalog table. The LEFT JOIN preserves rows whose
/// `model_id` is NULL.
const AIRCRAFT_SELECT: &str =
    "SELECT a.*, am.name AS model \
     FROM aircraft a LEFT JOIN aircraft_models am ON a.model_id = am.id";

/// Resolve the caller-supplied (model_id, model-name) pair into an i32
/// model_id. Precedence:
///   1. `model_id` wins if present (verified to exist in the org).
///   2. `model` (name) is looked up case-insensitively against
///      `aircraft_models` for this org. Unknown → 400.
///   3. Neither → Ok(None). The caller decides whether that is allowed.
async fn resolve_model_binding(
    pool: &DbPool,
    org_id: i32,
    model_id: Option<i32>,
    model_name: Option<&str>,
) -> Result<Option<i32>, AppError> {
    if let Some(id) = model_id {
        // Verify the id belongs to this org — prevents cross-tenant leaks.
        let ok = sqlx::query("SELECT 1 FROM aircraft_models WHERE id = $1 AND org_id = $2")
            .bind(id)
            .bind(org_id)
            .fetch_optional(pool)
            .await?
            .is_some();
        if !ok {
            return Err(AppError::BadRequest(format!(
                "model_id {} does not exist in this organization", id
            )));
        }
        return Ok(Some(id));
    }
    if let Some(name) = model_name.map(str::trim).filter(|s| !s.is_empty()) {
        match resolve_by_name_ci(pool, org_id, name).await? {
            Some(id) => Ok(Some(id)),
            None => Err(AppError::BadRequest(format!(
                "No model named '{}' exists. Create it first via POST /api/v1/aircraft-models.",
                name
            ))),
        }
    } else {
        Ok(None)
    }
}

/// GET /api/v1/aircraft — List aircraft for the authenticated organisation.
pub async fn list_aircraft(
    claims: Claims,
    State(pool): State<DbPool>,
    Query(params): Query<FleetQueryParams>,
) -> Result<Json<Vec<Aircraft>>, AppError> {
    let include_retired = params.include_retired.unwrap_or(false);
    let sql = if include_retired {
        format!(
            "{} WHERE a.org_id = $1 ORDER BY COALESCE(a.nickname, a.serial_number)",
            AIRCRAFT_SELECT
        )
    } else {
        format!(
            "{} WHERE a.org_id = $1 AND a.status != 'retired' ORDER BY COALESCE(a.nickname, a.serial_number)",
            AIRCRAFT_SELECT
        )
    };
    let rows = sqlx::query_as::<_, Aircraft>(&sql)
        .bind(claims.org_id)
        .fetch_all(&pool)
        .await?;
    Ok(Json(rows))
}

/// GET /api/v1/aircraft/:id — Get a single aircraft by ID.
pub async fn get_aircraft(
    claims: Claims,
    State(pool): State<DbPool>,
    Path(id): Path<i32>,
) -> Result<Json<Aircraft>, AppError> {
    let sql = format!("{} WHERE a.id = $1 AND a.org_id = $2", AIRCRAFT_SELECT);
    let row = sqlx::query_as::<_, Aircraft>(&sql)
        .bind(id)
        .bind(claims.org_id)
        .fetch_optional(&pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Aircraft {} not found", id)))?;
    Ok(Json(row))
}

/// POST /api/v1/aircraft — Create a new aircraft.
///
/// Runs the INSERT and default-schedule seeding in a single transaction
/// so the two either commit together or both roll back. If the aircraft
/// has a resolved `model_id`, the seeded schedules are considered part
/// of a successful creation — any seed failure fails the whole request
/// with a 500. If `model_id` is NULL (e.g. a batch-imported aircraft
/// whose model wasn't in the catalog), seeding is skipped silently per
/// policy and the admin can run `POST /maintenance-schedules/seed-defaults`
/// later.
pub async fn create_aircraft(
    claims: Claims,
    State(pool): State<DbPool>,
    Json(body): Json<CreateAircraftRequest>,
) -> Result<Json<Aircraft>, AppError> {
    require_admin_or_technician(&claims)?;
    if body.serial_number.trim().is_empty() {
        return Err(AppError::BadRequest("serial_number is required".into()));
    }

    let model_id = resolve_model_binding(
        &pool, claims.org_id, body.model_id, body.model.as_deref(),
    ).await?;

    let mut tx = pool.begin().await?;

    let new_row = sqlx::query(
        "INSERT INTO aircraft \
           (org_id, serial_number, equipment_id, model_id, nickname, \
            status, airworthiness_status) \
         VALUES ($1,$2,$3,$4,$5,'active','not_airworthy') RETURNING id"
    )
    .bind(claims.org_id)
    .bind(body.serial_number.trim())
    .bind(&body.equipment_id)
    .bind(model_id)
    .bind(&body.nickname)
    .fetch_one(&mut *tx)
    .await?;

    use sqlx::Row;
    let new_id: i32 = new_row.get("id");

    let sql = format!("{} WHERE a.id = $1 AND a.org_id = $2", AIRCRAFT_SELECT);
    let row = sqlx::query_as::<_, Aircraft>(&sql)
        .bind(new_id)
        .bind(claims.org_id)
        .fetch_one(&mut *tx)
        .await?;

    // Seed EASA-aligned default maintenance schedules within the same tx.
    // A failure here rolls back the aircraft INSERT so the request is
    // transactional. Aircraft without a resolved model (or with a model
    // that has no templates) get no defaults — handled internally as
    // SkippedNoModel / SkippedNoTemplates; admin can seed later via the
    // seed-defaults endpoint or by adding templates first.
    insert_defaults_for_aircraft(
        &mut tx,
        claims.org_id,
        row.id,
        row.model_id,
        &claims.user_code,
    )
    .await?;

    tx.commit().await?;

    audit::log_insert_full(&pool, claims.org_id, claims.user_id, &claims.user_code, "aircraft", row.id, &row).await;

    Ok(Json(row))
}

/// PUT /api/v1/aircraft/:id — Update an existing aircraft.
pub async fn update_aircraft(
    claims: Claims,
    State(pool): State<DbPool>,
    Path(id): Path<i32>,
    Json(body): Json<UpdateAircraftRequest>,
) -> Result<Json<Aircraft>, AppError> {
    require_admin_or_technician(&claims)?;

    let sql_select = format!("{} WHERE a.id = $1 AND a.org_id = $2", AIRCRAFT_SELECT);
    let mut tx = pool.begin().await?;
    let before = sqlx::query_as::<_, Aircraft>(&sql_select)
        .bind(id)
        .bind(claims.org_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Aircraft {} not found", id)))?;

    // For updates, NONE of (model_id, model) is valid — means "don't change"
    // Resolve only if either is provided.
    let new_model_id: Option<i32> = if body.model_id.is_some() || body.model.is_some() {
        resolve_model_binding(&pool, claims.org_id, body.model_id, body.model.as_deref()).await?
    } else {
        before.model_id
    };

    // SN is only allowed to change when the row is flagged needs_manual_sn —
    // i.e. the QR scanner couldn't extract a usable SN and the operator is
    // now hand-correcting it. For everyone else SN stays immutable.
    // When SN does change, auto-clear the flag in the same UPDATE so the
    // audit diff reflects both columns and the UI's warning indicators
    // disappear after a single round-trip.
    let new_serial_number: String = match body.serial_number.as_deref() {
        Some(s) => {
            let trimmed = s.trim();
            if !before.needs_manual_sn {
                return Err(AppError::BadRequest(
                    "serial_number can only be changed for aircraft flagged needs_manual_sn".into(),
                ));
            }
            if trimmed.is_empty() {
                return Err(AppError::BadRequest("serial_number cannot be empty".into()));
            }
            trimmed.to_string()
        }
        None => before.serial_number.clone(),
    };
    let new_needs_manual_sn = before.needs_manual_sn && new_serial_number == before.serial_number;

    // Measured airframe empty weight must be strictly positive when present
    // (NaN rejected too). An omitted field keeps its existing value via COALESCE.
    if let Some(w) = body.empty_weight_kg {
        if !(w > 0.0) {
            return Err(AppError::BadRequest(
                "empty_weight_kg must be greater than 0 when provided".into(),
            ));
        }
    }

    sqlx::query(
        "UPDATE aircraft SET \
           equipment_id = COALESCE($1, equipment_id), \
           model_id = $2, \
           nickname = COALESCE($3, nickname), \
           status = COALESCE($4, status), \
           retirement_notes = COALESCE($5, retirement_notes), \
           serial_number = $6, \
           needs_manual_sn = $7, \
           empty_weight_kg = COALESCE($8, empty_weight_kg) \
         WHERE id = $9 AND org_id = $10"
    )
    .bind(&body.equipment_id)
    .bind(new_model_id)
    .bind(&body.nickname)
    .bind(&body.status)
    .bind(&body.retirement_notes)
    .bind(&new_serial_number)
    .bind(new_needs_manual_sn)
    .bind(body.empty_weight_kg)
    .bind(id)
    .bind(claims.org_id)
    .execute(&mut *tx)
    .await?;

    let after = sqlx::query_as::<_, Aircraft>(&sql_select)
        .bind(id)
        .bind(claims.org_id)
        .fetch_one(&mut *tx)
        .await?;
    tx.commit().await?;

    audit::log_update_diff(&pool, claims.org_id, claims.user_id, &claims.user_code, "aircraft", after.id, &before, &after).await;

    Ok(Json(after))
}

/// DELETE /api/v1/aircraft/:id — Retire aircraft by setting status to 'retired'.
pub async fn retire_aircraft(
    claims: Claims,
    State(pool): State<DbPool>,
    Path(id): Path<i32>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_admin_or_technician(&claims)?;

    let sql_select = format!("{} WHERE a.id = $1 AND a.org_id = $2", AIRCRAFT_SELECT);
    let mut tx = pool.begin().await?;
    let before = sqlx::query_as::<_, Aircraft>(&sql_select)
        .bind(id)
        .bind(claims.org_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Aircraft {} not found", id)))?;

    sqlx::query("UPDATE aircraft SET status = 'retired' WHERE id = $1 AND org_id = $2")
        .bind(id)
        .bind(claims.org_id)
        .execute(&mut *tx)
        .await?;

    let after = sqlx::query_as::<_, Aircraft>(&sql_select)
        .bind(id)
        .bind(claims.org_id)
        .fetch_one(&mut *tx)
        .await?;
    tx.commit().await?;

    audit::log_update_diff(&pool, claims.org_id, claims.user_id, &claims.user_code, "aircraft", id, &before, &after).await;

    Ok(Json(serde_json::json!({ "message": "Aircraft retired", "id": id })))
}

/// Request body for `POST /api/v1/aircraft/:source_id/merge`.
#[derive(Debug, serde::Deserialize)]
pub struct MergeAircraftRequest {
    pub target_id: i32,
}

/// POST /api/v1/aircraft/:source_id/merge — admin-only.
///
/// Merges a pending-review aircraft (`needs_manual_sn = TRUE`) into an
/// existing active canonical aircraft of the same model. All flights,
/// batteries, controllers, defects, incidents, maintenance records, and
/// identifiers move to the target; per-aircraft maintenance schedules on
/// the source are deleted as duplicates; the source row is deleted.
/// See `services::aircraft_merge` for the validation rules and audit
/// envelope.
pub async fn merge_aircraft_handler(
    claims: Claims,
    State(pool): State<DbPool>,
    Path(source_id): Path<i32>,
    Json(body): Json<MergeAircraftRequest>,
) -> Result<Json<MergeOutcome>, AppError> {
    require_admin(&claims)?;
    let outcome = merge_aircraft(
        &pool,
        source_id,
        body.target_id,
        claims.org_id,
        claims.user_id,
        &claims.user_code,
    )
    .await?;
    Ok(Json(outcome))
}

/// PATCH /api/v1/aircraft/:id/airworthiness
///
/// Admin or technician sets the Maintenance Manager-controlled
/// airworthiness flag (`airworthy` | `not_airworthy`) on an aircraft.
/// The mandatory `comment` field is included in the audit diff envelope
/// so the EASA audit trail records the reason for the status change.
///
/// Independent of the operational `status` column (active/retired): an
/// active aircraft can be `not_airworthy` (awaiting parts), and a
/// retired aircraft is implicitly not airworthy regardless of this flag.
pub async fn patch_airworthiness(
    claims: Claims,
    State(pool): State<DbPool>,
    Path(id): Path<i32>,
    Json(body): Json<PatchAirworthinessRequest>,
) -> Result<Json<Aircraft>, AppError> {
    require_admin_or_technician(&claims)?;

    let new_status = body.airworthiness_status.trim();
    if new_status != "airworthy" && new_status != "not_airworthy" {
        return Err(AppError::BadRequest(
            "airworthiness_status must be 'airworthy' or 'not_airworthy'".into(),
        ));
    }
    let comment = body.comment.trim();
    if comment.is_empty() {
        return Err(AppError::BadRequest(
            "comment is required and cannot be empty".into(),
        ));
    }

    let sql_select = format!("{} WHERE a.id = $1 AND a.org_id = $2", AIRCRAFT_SELECT);
    let mut tx = pool.begin().await?;
    let before = sqlx::query_as::<_, Aircraft>(&sql_select)
        .bind(id)
        .bind(claims.org_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Aircraft {} not found", id)))?;

    sqlx::query(
        "UPDATE aircraft SET airworthiness_status = $1 \
         WHERE id = $2 AND org_id = $3",
    )
    .bind(new_status)
    .bind(id)
    .bind(claims.org_id)
    .execute(&mut *tx)
    .await?;

    let after = sqlx::query_as::<_, Aircraft>(&sql_select)
        .bind(id)
        .bind(claims.org_id)
        .fetch_one(&mut *tx)
        .await?;
    tx.commit().await?;

    // Wrap the before/after pair in a small envelope so the operator's
    // mandatory comment lands inside the audit diff alongside the status
    // change. The audit service serialises the wrapper as-is.
    #[derive(serde::Serialize)]
    struct AirworthinessSnapshot<'a> {
        airworthiness_status: &'a str,
        comment: &'a str,
    }
    let before_snap = AirworthinessSnapshot {
        airworthiness_status: &before.airworthiness_status,
        comment: "",
    };
    let after_snap = AirworthinessSnapshot {
        airworthiness_status: &after.airworthiness_status,
        comment,
    };
    audit::log_update_diff(
        &pool,
        claims.org_id,
        claims.user_id,
        &claims.user_code,
        "aircraft",
        after.id,
        &before_snap,
        &after_snap,
    )
    .await;

    Ok(Json(after))
}
