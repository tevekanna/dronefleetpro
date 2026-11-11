// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::Deserialize;

use crate::auth::middleware::{require_admin_or_technician, Claims};
use crate::db::DbPool;
use crate::errors::AppError;
use crate::models::maintenance::{
    CreateMaintenanceRequest, MaintenanceAuditEntry, MaintenanceDetail, MaintenanceRecord,
    SaveCompletionsRequest, SaveCompletionsResponse, UpdateMaintenanceRequest,
};
use crate::services::audit;
use crate::services::audit_diff::summarise_changed_fields;

#[derive(Debug, Deserialize)]
pub struct ListMaintenanceQuery {
    #[serde(default)]
    pub aircraft_id: Option<i32>,
    #[serde(default)]
    pub controller_id: Option<i32>,
}

#[cfg(test)]
#[path = "maintenance_tests.rs"]
mod tests;

const VALID_CATEGORIES: &[&str] = &["Scheduled", "Unscheduled"];
const VALID_TYPES: &[&str] = &["Repair", "Modification", "Overhaul", "Test"];

/// Column list + JOINs shared across every maintenance SELECT. Every query
/// site (list / get / detail / post-insert reload / pre+post-update reload /
/// pre-delete reload) projects the same shape so MaintenanceRecord round-trips
/// stay consistent.
const MAINTENANCE_SELECT_COLUMNS: &str = "\
m.id, m.org_id, m.aircraft_id, m.controller_id, m.performed_date, m.flight_id, \
m.created_by, m.remark, m.action_taken, m.maint_category, m.maint_type, m.sign_off, \
m.created_at, m.schedule_id, m.instruction_template_id, \
COALESCE(a.nickname, am.name, a.serial_number) AS aircraft_name, \
COALESCE(rc.nickname, rc.model, rc.serial_number) AS controller_name";

const MAINTENANCE_JOINS: &str = "\
FROM maintenance m \
LEFT JOIN aircraft a ON a.id = m.aircraft_id \
LEFT JOIN aircraft_models am ON am.id = a.model_id \
LEFT JOIN remote_controllers rc ON rc.id = m.controller_id";

fn validate_user_code(code: &str, field_name: &str) -> Result<(), AppError> {
    if code.len() != 3 || !code.chars().all(|c| c.is_ascii_uppercase()) {
        return Err(AppError::BadRequest(format!(
            "{} must be exactly 3 uppercase letters", field_name
        )));
    }
    Ok(())
}

/// Enforces the application-layer "exactly one target" invariant on every
/// maintenance write path. The DB CHECK constraint (migration 031) only
/// guarantees "at most one"; the strict "exactly one" rule lives here so
/// that ON DELETE SET NULL on either FK can still null out the surviving
/// reference for audit-trail preservation without rejecting historical
/// rows.
fn validate_exactly_one_target(
    aircraft_id: Option<i32>,
    controller_id: Option<i32>,
) -> Result<(), AppError> {
    match (aircraft_id, controller_id) {
        (Some(a), None) if a > 0 => Ok(()),
        (None, Some(c)) if c > 0 => Ok(()),
        (Some(_), Some(_)) => Err(AppError::BadRequest(
            "Provide exactly one of aircraft_id or controller_id, not both".into(),
        )),
        _ => Err(AppError::BadRequest(
            "Provide exactly one of aircraft_id or controller_id".into(),
        )),
    }
}

/// GET /api/v1/maintenance — List maintenance records for the org.
///
/// Optional query params (`?aircraft_id=N` and/or `?controller_id=N`) filter
/// the list to events targeting that specific equipment row. When neither is
/// set, returns every record in the org (the historical behavior).
pub async fn list_maintenance(
    claims: Claims,
    State(pool): State<DbPool>,
    Query(query): Query<ListMaintenanceQuery>,
) -> Result<Json<Vec<MaintenanceRecord>>, AppError> {
    let mut sql = format!(
        "SELECT {cols} {joins} WHERE m.org_id = $1",
        cols = MAINTENANCE_SELECT_COLUMNS,
        joins = MAINTENANCE_JOINS,
    );
    let mut bind_idx: u32 = 2;
    if query.aircraft_id.is_some() {
        sql.push_str(&format!(" AND m.aircraft_id = ${bind_idx}"));
        bind_idx += 1;
    }
    if query.controller_id.is_some() {
        sql.push_str(&format!(" AND m.controller_id = ${bind_idx}"));
    }
    sql.push_str(" ORDER BY m.performed_date DESC, m.created_at DESC");

    let mut q = sqlx::query_as::<_, MaintenanceRecord>(&sql).bind(claims.org_id);
    if let Some(id) = query.aircraft_id {
        q = q.bind(id);
    }
    if let Some(id) = query.controller_id {
        q = q.bind(id);
    }

    let rows = q.fetch_all(&pool).await?;
    Ok(Json(rows))
}

/// GET /api/v1/maintenance/:id — Get a single maintenance record.
pub async fn get_maintenance(
    claims: Claims,
    State(pool): State<DbPool>,
    Path(id): Path<i32>,
) -> Result<Json<MaintenanceRecord>, AppError> {
    let sql = format!(
        "SELECT {cols} {joins} WHERE m.id = $1 AND m.org_id = $2",
        cols = MAINTENANCE_SELECT_COLUMNS,
        joins = MAINTENANCE_JOINS,
    );
    let row = sqlx::query_as::<_, MaintenanceRecord>(&sql)
        .bind(id)
        .bind(claims.org_id)
        .fetch_optional(&pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Maintenance record {} not found", id)))?;

    Ok(Json(row))
}

/// GET /api/v1/maintenance/:id/detail — Get a maintenance record with its audit trail.
///
/// Single round-trip for the maintenance detail page. The existing
/// `GET /maintenance/:id` stays slim and remains the canonical endpoint for
/// listing/edit flows. This `/detail` endpoint additionally returns every
/// audit_log row keyed on (table_name='maintenance', record_id), newest first,
/// with the changed-field summary computed server-side (same helper used by
/// the audit-log viewer).
pub async fn get_maintenance_detail(
    claims: Claims,
    State(pool): State<DbPool>,
    Path(id): Path<i32>,
) -> Result<Json<MaintenanceDetail>, AppError> {
    let sql = format!(
        "SELECT {cols} {joins} WHERE m.id = $1 AND m.org_id = $2",
        cols = MAINTENANCE_SELECT_COLUMNS,
        joins = MAINTENANCE_JOINS,
    );
    let record = sqlx::query_as::<_, MaintenanceRecord>(&sql)
        .bind(id)
        .bind(claims.org_id)
        .fetch_optional(&pool)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Maintenance record {} not found", id)))?;

    #[derive(sqlx::FromRow)]
    struct AuditRow {
        id: i64,
        action: String,
        user_id: Option<i32>,
        user_code: Option<String>,
        user_name: Option<String>,
        ip_address: Option<String>,
        created_at: chrono::DateTime<chrono::Utc>,
        diff_jsonb: Option<serde_json::Value>,
    }

    let rows = sqlx::query_as::<_, AuditRow>(
        "SELECT al.id, al.action, al.user_id, al.user_code, \
                u.name AS user_name, al.ip_address, al.created_at, al.diff_jsonb \
         FROM audit_log al \
         LEFT JOIN users u ON u.id = al.user_id \
         WHERE al.org_id = $1 AND al.table_name = 'maintenance' AND al.record_id = $2 \
         ORDER BY al.created_at DESC, al.id DESC"
    )
    .bind(claims.org_id)
    .bind(id)
    .fetch_all(&pool)
    .await?;

    let audit: Vec<MaintenanceAuditEntry> = rows
        .into_iter()
        .map(|r| {
            // Only compute changed_fields for UPDATE — mirrors the audit_log
            // viewer (handlers::audit_log::list). INSERT and DELETE rows would
            // surface every populated column as "changed" which is noise.
            let changed_fields = if r.action == "UPDATE" {
                r.diff_jsonb
                    .as_ref()
                    .map(summarise_changed_fields)
                    .unwrap_or_default()
            } else {
                Vec::new()
            };
            let diff_summary = if r.action == "UPDATE" && !changed_fields.is_empty() {
                Some(changed_fields.join(", "))
            } else {
                None
            };
            MaintenanceAuditEntry {
                id: r.id,
                action: r.action,
                user_id: r.user_id,
                user_code: r.user_code,
                user_name: r.user_name,
                ip_address: r.ip_address,
                created_at: r.created_at,
                changed_fields,
                diff_summary,
            }
        })
        .collect();

    Ok(Json(MaintenanceDetail { record, audit }))
}

/// POST /api/v1/maintenance — Create an EASA OSO #3 compliant maintenance record.
pub async fn create_maintenance(
    claims: Claims,
    State(pool): State<DbPool>,
    Json(body): Json<CreateMaintenanceRequest>,
) -> Result<Json<MaintenanceRecord>, AppError> {
    require_admin_or_technician(&claims)?;

    // Exactly one of aircraft_id / controller_id must be provided.
    validate_exactly_one_target(body.aircraft_id, body.controller_id)?;

    validate_user_code(&body.created_by, "created_by")?;
    if body.remark.trim().is_empty() {
        return Err(AppError::BadRequest("remark must describe WHAT was observed / WHEN".into()));
    }
    if body.action_taken.trim().is_empty() {
        return Err(AppError::BadRequest("action_taken must describe WHAT was done / HOW verified".into()));
    }
    if !VALID_CATEGORIES.contains(&body.maint_category.as_str()) {
        return Err(AppError::BadRequest(format!(
            "maint_category must be one of: {:?}", VALID_CATEGORIES
        )));
    }
    if !VALID_TYPES.contains(&body.maint_type.as_str()) {
        return Err(AppError::BadRequest(format!(
            "maint_type must be one of: {:?}", VALID_TYPES
        )));
    }
    validate_user_code(&body.sign_off, "sign_off")?;

    // EASA OSO #3 — separation of duties: created_by must differ from sign_off.
    // Existing rows pre-dating this check (E2E fixtures) violate the rule but
    // are not retroactively rejected; only new inserts are blocked.
    if body.created_by.eq_ignore_ascii_case(&body.sign_off) {
        return Err(AppError::BadRequest(
            "EASA OSO #3 requires separation of duties: \
             the technician who performed the work (created_by) must differ \
             from the technician who signs off (sign_off)."
                .to_string(),
        ));
    }

    // Step 1: Insert and get the id
    let row = sqlx::query(
        "INSERT INTO maintenance (org_id, aircraft_id, controller_id, performed_date, flight_id, created_by, remark, action_taken, maint_category, maint_type, sign_off) \
         VALUES ($1,$2,$3,$4::date,$5,$6,$7,$8,$9,$10,$11) RETURNING id"
    )
    .bind(claims.org_id)
    .bind(body.aircraft_id)
    .bind(body.controller_id)
    .bind(&body.performed_date)
    .bind(body.flight_id)
    .bind(&body.created_by)
    .bind(&body.remark)
    .bind(&body.action_taken)
    .bind(&body.maint_category)
    .bind(&body.maint_type)
    .bind(&body.sign_off)
    .fetch_one(&pool)
    .await
    .map_err(|e| {
        tracing::error!("create_maintenance DB error: {:?}", e);
        AppError::Database(e)
    })?;

    use sqlx::Row;
    let new_id: i32 = row.get("id");

    // Step 2: Fetch full record with JOIN (includes aircraft_name + controller_name)
    let sql = format!(
        "SELECT {cols} {joins} WHERE m.id = $1",
        cols = MAINTENANCE_SELECT_COLUMNS,
        joins = MAINTENANCE_JOINS,
    );
    let record = sqlx::query_as::<_, MaintenanceRecord>(&sql)
        .bind(new_id)
        .fetch_one(&pool)
        .await
        .map_err(|e| AppError::Database(e))?;

    audit::log_insert_full(&pool, claims.org_id, claims.user_id, &claims.user_code, "maintenance", new_id, &record).await;

    Ok(Json(record))
}

/// PUT /api/v1/maintenance/:id — Update remark, action_taken, or sign_off.
///
/// Note: aircraft_id and controller_id are immutable after create. To
/// re-target a maintenance event to a different aircraft or controller,
/// delete and re-create the record.
pub async fn update_maintenance(
    claims: Claims,
    State(pool): State<DbPool>,
    Path(id): Path<i32>,
    Json(body): Json<UpdateMaintenanceRequest>,
) -> Result<Json<MaintenanceRecord>, AppError> {
    require_admin_or_technician(&claims)?;
    // Validate sign_off if provided
    if let Some(ref code) = body.sign_off {
        validate_user_code(code, "sign_off")?;
    }

    let mut tx = pool.begin().await?;
    let select_by_id_org = format!(
        "SELECT {cols} {joins} WHERE m.id = $1 AND m.org_id = $2",
        cols = MAINTENANCE_SELECT_COLUMNS,
        joins = MAINTENANCE_JOINS,
    );
    let before = sqlx::query_as::<_, MaintenanceRecord>(&select_by_id_org)
        .bind(id)
        .bind(claims.org_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Maintenance record {} not found", id)))?;

    // EASA OSO #3 — separation of duties on update: a new sign_off must differ
    // from the (immutable) created_by. Same forward-only enforcement as create.
    if let Some(ref new_sign_off) = body.sign_off {
        if before.created_by.eq_ignore_ascii_case(new_sign_off) {
            return Err(AppError::BadRequest(
                "EASA OSO #3 requires separation of duties: \
                 sign_off must differ from the record's created_by."
                    .to_string(),
            ));
        }
    }

    sqlx::query(
        "UPDATE maintenance SET remark = COALESCE($1, remark), action_taken = COALESCE($2, action_taken), sign_off = COALESCE($3, sign_off) \
         WHERE id = $4 AND org_id = $5"
    )
    .bind(&body.remark)
    .bind(&body.action_taken)
    .bind(&body.sign_off)
    .bind(id)
    .bind(claims.org_id)
    .execute(&mut *tx)
    .await?;

    let after = sqlx::query_as::<_, MaintenanceRecord>(&select_by_id_org)
        .bind(id)
        .bind(claims.org_id)
        .fetch_one(&mut *tx)
        .await?;
    tx.commit().await?;

    audit::log_update_diff(&pool, claims.org_id, claims.user_id, &claims.user_code, "maintenance", after.id, &before, &after).await;

    Ok(Json(after))
}

/// DELETE /api/v1/maintenance/:id — Delete a maintenance record.
pub async fn delete_maintenance(
    claims: Claims,
    State(pool): State<DbPool>,
    Path(id): Path<i32>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_admin_or_technician(&claims)?;
    let mut tx = pool.begin().await?;
    let select_by_id_org = format!(
        "SELECT {cols} {joins} WHERE m.id = $1 AND m.org_id = $2",
        cols = MAINTENANCE_SELECT_COLUMNS,
        joins = MAINTENANCE_JOINS,
    );
    let before = sqlx::query_as::<_, MaintenanceRecord>(&select_by_id_org)
        .bind(id)
        .bind(claims.org_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Maintenance record {} not found", id)))?;

    sqlx::query("DELETE FROM maintenance WHERE id = $1 AND org_id = $2")
        .bind(id)
        .bind(claims.org_id)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;

    audit::log_delete_full(&pool, claims.org_id, claims.user_id, &claims.user_code, "maintenance", id, &before).await;

    Ok(Json(serde_json::json!({ "message": "Maintenance record deleted", "id": id })))
}

/// POST /api/v1/maintenance/:id/completions
///
/// Saves checklist completions for an existing maintenance record and links
/// the instruction template. Idempotent per (maintenance_id, checklist_item_id)
/// — re-posting updates status and note. Only admin or technician may call this.
pub async fn save_completions(
    claims: Claims,
    State(pool): State<DbPool>,
    Path(id): Path<i32>,
    Json(body): Json<SaveCompletionsRequest>,
) -> Result<Json<SaveCompletionsResponse>, AppError> {
    require_admin_or_technician(&claims)?;

    // Fetch the maintenance record (and its aircraft_id + performed_date for
    // NOK auto-defect creation) and verify it belongs to this org.
    #[derive(sqlx::FromRow)]
    struct MaintTarget {
        aircraft_id: Option<i32>,
        performed_date: chrono::NaiveDate,
    }
    let target = sqlx::query_as::<_, MaintTarget>(
        "SELECT aircraft_id, performed_date FROM maintenance WHERE id = $1 AND org_id = $2"
    )
    .bind(id)
    .bind(claims.org_id)
    .fetch_optional(&pool)
    .await
    .map_err(|e| {
        tracing::error!(error=?e, "save_completions: fetch target failed");
        AppError::Database(e)
    })?
    .ok_or_else(|| AppError::NotFound(format!("Maintenance record {} not found", id)))?;

    // Validate status values up front.
    const VALID_STATUSES: &[&str] = &["ok", "nok", "na", "pending"];
    for c in &body.completions {
        if !VALID_STATUSES.contains(&c.status.as_str()) {
            return Err(AppError::BadRequest(format!(
                "Invalid status '{}' — must be one of: ok, nok, na, pending",
                c.status
            )));
        }
        if c.completed_by.len() != 3 || !c.completed_by.chars().all(|ch| ch.is_ascii_uppercase()) {
            return Err(AppError::BadRequest(format!(
                "completed_by '{}' must be exactly 3 uppercase ASCII letters",
                c.completed_by
            )));
        }
    }

    let mut tx = pool.begin().await?;

    // Link the template to the maintenance record.
    sqlx::query(
        "UPDATE maintenance SET instruction_template_id = $1 WHERE id = $2 AND org_id = $3"
    )
    .bind(body.template_id)
    .bind(id)
    .bind(claims.org_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!(error=?e, "save_completions: UPDATE template_id failed");
        AppError::Database(e)
    })?;

    // Upsert each completion row.
    let mut saved: usize = 0;
    for c in &body.completions {
        let completed_at: Option<chrono::DateTime<chrono::Utc>> =
            if c.status != "pending" { Some(chrono::Utc::now()) } else { None };

        sqlx::query(
            "INSERT INTO maintenance_checklist_completions
                (maintenance_id, checklist_item_id, status, note, completed_by, completed_at)
             VALUES ($1, $2, $3, $4, $5, $6)
             ON CONFLICT (maintenance_id, checklist_item_id)
             DO UPDATE SET
                status       = EXCLUDED.status,
                note         = EXCLUDED.note,
                completed_by = EXCLUDED.completed_by,
                completed_at = EXCLUDED.completed_at"
        )
        .bind(id)
        .bind(c.checklist_item_id)
        .bind(&c.status)
        .bind(&c.note)
        .bind(&c.completed_by)
        .bind(completed_at)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            tracing::error!(error=?e, "save_completions: upsert item {} failed", c.checklist_item_id);
            AppError::Database(e)
        })?;
        saved += 1;
    }

    // Auto-create defects for NOK items (aircraft maintenance only).
    // NOTE: these auto-created defects do NOT get individual audit_log entries
    // in this PR — they are inserted inside the transaction and per-row audit
    // would require post-commit audit calls. Deferred to a later cleanup PR.
    let mut defects_created: usize = 0;
    if let Some(aircraft_id) = target.aircraft_id {
        for c in &body.completions {
            if c.status != "nok" {
                continue;
            }
            // Fetch item_text for the description.
            let item_text: Option<String> = sqlx::query_scalar(
                "SELECT item_text FROM maintenance_checklist_items WHERE id = $1"
            )
            .bind(c.checklist_item_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| {
                tracing::error!(error=?e, "save_completions: fetch item_text failed");
                AppError::Database(e)
            })?;

            let item_text = item_text.unwrap_or_else(|| format!("Item {}", c.checklist_item_id));

            let description = match &c.note {
                Some(note) if !note.trim().is_empty() => {
                    format!("[Checklist NOK] {}: {}", item_text, note.trim())
                }
                _ => format!("[Checklist NOK] {}", item_text),
            };

            sqlx::query(
                "INSERT INTO defect_log \
                    (org_id, aircraft_id, flight_id, defect_date, reported_by, description, status) \
                 VALUES ($1, $2, NULL, $3, $4, $5, 'open')"
            )
            .bind(claims.org_id)
            .bind(aircraft_id)
            .bind(target.performed_date)
            .bind(&c.completed_by)
            .bind(&description)
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                tracing::error!(error=?e, "save_completions: auto-defect insert failed");
                AppError::Database(e)
            })?;

            defects_created += 1;
        }
    }

    tx.commit().await?;

    Ok(Json(SaveCompletionsResponse {
        maintenance_id: id,
        template_id: body.template_id,
        saved,
        defects_created,
    }))
}
