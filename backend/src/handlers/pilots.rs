// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

use axum::{
    extract::{Path, State},
    Json,
};

use crate::auth::middleware::{require_admin, Claims};
use crate::db::DbPool;
use crate::errors::AppError;
use crate::models::pilot::{Pilot, PilotQualification, PilotWithQualifications, CreatePilotRequest, UpdatePilotRequest};
use crate::services::audit;

/// Helper: fetch qualifications for a set of pilot IDs
async fn fetch_qualifications_for_org(pool: &DbPool, org_id: i32) -> Result<Vec<PilotQualification>, AppError> {
    let quals = sqlx::query_as::<_, PilotQualification>(
        "SELECT pq.* FROM pilot_qualifications pq INNER JOIN pilots p ON pq.pilot_id = p.id WHERE p.org_id = $1 ORDER BY pq.pilot_id, pq.type_name"
    )
    .bind(org_id)
    .fetch_all(pool)
    .await?;
    Ok(quals)
}

/// Helper: fetch qualifications for a single pilot
async fn fetch_qualifications_for_pilot(pool: &DbPool, pilot_id: i32) -> Result<Vec<PilotQualification>, AppError> {
    let quals = sqlx::query_as::<_, PilotQualification>(
        "SELECT * FROM pilot_qualifications WHERE pilot_id = $1 ORDER BY type_name"
    )
    .bind(pilot_id)
    .fetch_all(pool)
    .await?;
    Ok(quals)
}

/// Helper: sync qualifications for a pilot (delete + re-insert)
async fn sync_qualifications(pool: &DbPool, org_id: i32, pilot_id: i32, quals: &[String]) -> Result<(), AppError> {
    // Delete existing
    sqlx::query("DELETE FROM pilot_qualifications WHERE pilot_id = $1")
        .bind(pilot_id)
        .execute(pool)
        .await?;

    // Insert new
    for q in quals {
        let q_trimmed = q.trim().to_uppercase();
        if !q_trimmed.is_empty() {
            let _ = sqlx::query(
                "INSERT INTO pilot_qualifications (org_id, pilot_id, type_name) VALUES ($1, $2, $3) ON CONFLICT (pilot_id, type_name) DO NOTHING"
            )
            .bind(org_id)
            .bind(pilot_id)
            .bind(&q_trimmed)
            .execute(pool)
            .await;
        }
    }
    Ok(())
}

/// GET /api/v1/pilots — List all pilots with qualifications.
pub async fn list_pilots(
    claims: Claims,
    State(pool): State<DbPool>,
) -> Result<Json<Vec<PilotWithQualifications>>, AppError> {
    let pilots = sqlx::query_as::<_, Pilot>(
        "SELECT * FROM pilots WHERE org_id = $1 ORDER BY name"
    )
    .bind(claims.org_id)
    .fetch_all(&pool)
    .await?;

    let all_quals = fetch_qualifications_for_org(&pool, claims.org_id).await?;

    let result: Vec<PilotWithQualifications> = pilots.into_iter().map(|p| {
        let pilot_quals: Vec<PilotQualification> = all_quals.iter()
            .filter(|q| q.pilot_id == p.id)
            .cloned()
            .collect();
        PilotWithQualifications::from_pilot(p, pilot_quals)
    }).collect();

    Ok(Json(result))
}

/// GET /api/v1/pilots/:id — Get a single pilot with qualifications.
pub async fn get_pilot(
    claims: Claims,
    State(pool): State<DbPool>,
    Path(id): Path<i32>,
) -> Result<Json<PilotWithQualifications>, AppError> {
    let pilot = sqlx::query_as::<_, Pilot>(
        "SELECT * FROM pilots WHERE id = $1 AND org_id = $2"
    )
    .bind(id)
    .bind(claims.org_id)
    .fetch_optional(&pool)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Pilot {} not found", id)))?;

    let quals = fetch_qualifications_for_pilot(&pool, pilot.id).await?;

    Ok(Json(PilotWithQualifications::from_pilot(pilot, quals)))
}

/// POST /api/v1/pilots — Create a new pilot with optional qualifications.
pub async fn create_pilot(
    claims: Claims,
    State(pool): State<DbPool>,
    Json(body): Json<CreatePilotRequest>,
) -> Result<Json<PilotWithQualifications>, AppError> {
    require_admin(&claims)?;
    if body.name.trim().is_empty() {
        return Err(AppError::BadRequest("name is required".into()));
    }

    let pilot = sqlx::query_as::<_, Pilot>(
        "INSERT INTO pilots (org_id, name, email, phone, license_number, license_type, license_expires, notes, is_active) VALUES ($1,$2,$3,$4,$5,$6,$7::date,$8,true) RETURNING *"
    )
    .bind(claims.org_id)
    .bind(body.name.trim())
    .bind(&body.email)
    .bind(&body.phone)
    .bind(&body.license_number)
    .bind(&body.license_type)
    .bind(&body.license_expires)
    .bind(&body.notes)
    .fetch_one(&pool)
    .await
    .map_err(|e| {
        tracing::error!("create_pilot DB error: {:?}", e);
        AppError::Database(e)
    })?;

    // Sync qualifications if provided and license_type is SPEC
    if let Some(quals) = &body.qualifications {
        if !quals.is_empty() {
            sync_qualifications(&pool, claims.org_id, pilot.id, quals).await?;
        }
    }

    let pilot_quals = fetch_qualifications_for_pilot(&pool, pilot.id).await?;
    audit::log_insert_full(&pool, claims.org_id, claims.user_id, &claims.user_code, "pilots", pilot.id, &pilot).await;

    Ok(Json(PilotWithQualifications::from_pilot(pilot, pilot_quals)))
}

/// PUT /api/v1/pilots/:id — Update an existing pilot with qualifications.
pub async fn update_pilot(
    claims: Claims,
    State(pool): State<DbPool>,
    Path(id): Path<i32>,
    Json(body): Json<UpdatePilotRequest>,
) -> Result<Json<PilotWithQualifications>, AppError> {
    require_admin(&claims)?;
    let mut tx = pool.begin().await?;
    let before = sqlx::query_as::<_, Pilot>(
        "SELECT * FROM pilots WHERE id = $1 AND org_id = $2"
    )
    .bind(id)
    .bind(claims.org_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Pilot {} not found", id)))?;

    let pilot = sqlx::query_as::<_, Pilot>(
        "UPDATE pilots SET name = COALESCE($1, name), email = COALESCE($2, email), phone = COALESCE($3, phone), license_number = COALESCE($4, license_number), license_type = COALESCE($5, license_type), license_expires = COALESCE($6::date, license_expires), is_active = COALESCE($7, is_active), notes = COALESCE($8, notes) WHERE id = $9 AND org_id = $10 RETURNING *"
    )
    .bind(&body.name)
    .bind(&body.email)
    .bind(&body.phone)
    .bind(&body.license_number)
    .bind(&body.license_type)
    .bind(&body.license_expires)
    .bind(&body.is_active)
    .bind(&body.notes)
    .bind(id)
    .bind(claims.org_id)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;

    // Sync qualifications if provided
    if let Some(quals) = &body.qualifications {
        // If license_type changed away from SPEC, clear qualifications
        if pilot.license_type.as_deref() != Some("SPEC") {
            sqlx::query("DELETE FROM pilot_qualifications WHERE pilot_id = $1")
                .bind(id)
                .execute(&pool)
                .await?;
        } else {
            sync_qualifications(&pool, claims.org_id, id, quals).await?;
        }
    }

    let pilot_quals = fetch_qualifications_for_pilot(&pool, pilot.id).await?;
    audit::log_update_diff(&pool, claims.org_id, claims.user_id, &claims.user_code, "pilots", pilot.id, &before, &pilot).await;

    Ok(Json(PilotWithQualifications::from_pilot(pilot, pilot_quals)))
}

/// DELETE /api/v1/pilots/:id — Soft delete by setting is_active = false.
/// NOTE: logged with action=UPDATE (per the decision that soft-update deletes
/// should not misrepresent themselves as hard deletes in the audit log).
/// Older audit rows for this handler may still say DELETE.
pub async fn delete_pilot(
    claims: Claims,
    State(pool): State<DbPool>,
    Path(id): Path<i32>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_admin(&claims)?;
    let mut tx = pool.begin().await?;
    let before = sqlx::query_as::<_, Pilot>(
        "SELECT * FROM pilots WHERE id = $1 AND org_id = $2"
    )
    .bind(id)
    .bind(claims.org_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Pilot {} not found", id)))?;

    let after = sqlx::query_as::<_, Pilot>(
        "UPDATE pilots SET is_active = false WHERE id = $1 AND org_id = $2 RETURNING *"
    )
    .bind(id)
    .bind(claims.org_id)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;

    audit::log_update_diff(&pool, claims.org_id, claims.user_id, &claims.user_code, "pilots", id, &before, &after).await;

    Ok(Json(serde_json::json!({ "message": "Pilot deactivated", "id": id })))
}
