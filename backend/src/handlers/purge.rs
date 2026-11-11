// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

use axum::{
    extract::{Path, State},
    Json,
};
use sqlx::{Postgres, Row, Transaction};

use crate::auth::middleware::{require_admin, Claims};
use crate::db::DbPool;
use crate::errors::AppError;
use crate::services::audit;
use crate::services::retention::{self, PurgeConfirmation, RetentionImpact};

#[derive(Debug, serde::Serialize)]
pub struct PurgeResult {
    pub deleted: i64,
    pub table: String,
}

/// Audit work collected during the purge transaction. The helpers below do
/// NOT write to audit_log directly — they push descriptors here. After the
/// transaction commits successfully, `flush_audit` drains the queue and
/// calls the audit service. This guarantees no audit row is written if the
/// purge rolls back, while still letting individual audit-log inserts use
/// the pool (so a transient audit insert failure doesn't undo the purge).
enum PendingAudit {
    Delete {
        table: &'static str,
        record_id: i32,
        row_json: serde_json::Value,
    },
    BulkUpdate {
        table: &'static str,
        column: &'static str,
        set_to: serde_json::Value,
        row_count: i64,
    },
}

/// Run `DELETE … RETURNING id, to_jsonb(t.*) AS row_json` and queue one
/// per-row DELETE audit entry. Caller must commit the transaction before
/// flushing the audit queue.
async fn delete_and_collect_audit(
    tx: &mut Transaction<'_, Postgres>,
    org_id: i32,
    table: &'static str,
    delete_sql: &str,
    pending: &mut Vec<PendingAudit>,
) -> Result<i64, AppError> {
    let rows = sqlx::query(delete_sql)
        .bind(org_id)
        .fetch_all(&mut **tx)
        .await?;
    let mut count = 0i64;
    for r in rows {
        let id: i32 = r.try_get("id").unwrap_or(0);
        let row_json: serde_json::Value =
            r.try_get("row_json").unwrap_or(serde_json::Value::Null);
        pending.push(PendingAudit::Delete {
            table,
            record_id: id,
            row_json,
        });
        count += 1;
    }
    Ok(count)
}

/// Run a bulk UPDATE and queue a single audit summary if any rows were
/// affected. Skips the audit if `rows_affected == 0`.
async fn nullify_and_collect_audit(
    tx: &mut Transaction<'_, Postgres>,
    org_id: i32,
    table: &'static str,
    column: &'static str,
    sql: &str,
    pending: &mut Vec<PendingAudit>,
) -> Result<u64, AppError> {
    let result = sqlx::query(sql)
        .bind(org_id)
        .execute(&mut **tx)
        .await?;
    let n = result.rows_affected();
    if n > 0 {
        pending.push(PendingAudit::BulkUpdate {
            table,
            column,
            set_to: serde_json::Value::Null,
            row_count: n as i64,
        });
    }
    Ok(n)
}

/// Drain the queue and emit audit entries. Called only after the purge
/// transaction has committed successfully. Audit failures are non-fatal
/// (logged via tracing inside the audit service).
async fn flush_audit(pool: &DbPool, claims: &Claims, pending: Vec<PendingAudit>) {
    for entry in pending {
        match entry {
            PendingAudit::Delete {
                table,
                record_id,
                row_json,
            } => {
                audit::log_delete_full(
                    pool,
                    claims.org_id,
                    claims.user_id,
                    &claims.user_code,
                    table,
                    record_id,
                    &row_json,
                )
                .await;
            }
            PendingAudit::BulkUpdate {
                table,
                column,
                set_to,
                row_count,
            } => {
                audit::log_bulk_update(
                    pool,
                    claims.org_id,
                    claims.user_id,
                    &claims.user_code,
                    table,
                    column,
                    set_to,
                    row_count,
                )
                .await;
            }
        }
    }
}

/// GET /api/v1/admin/purge/:target/preflight
///
/// Read-only retention impact assessment for a purge target. The counts
/// tell the operator what a confirmation body must acknowledge.
pub async fn purge_preflight(
    claims: Claims,
    State(pool): State<DbPool>,
    Path(target): Path<String>,
) -> Result<Json<RetentionImpact>, AppError> {
    require_admin(&claims)?;
    let mut conn = pool.acquire().await?;
    let impact = retention::impact_for_target(&mut conn, claims.org_id, &target).await?;
    Ok(Json(impact))
}

/// DELETE /api/v1/admin/purge/flights
pub async fn purge_flights(
    claims: Claims,
    State(pool): State<DbPool>,
    body: Option<Json<PurgeConfirmation>>,
) -> Result<Json<PurgeResult>, AppError> {
    require_admin(&claims)?;
    let confirmation = body.map(|Json(b)| b);

    let mut pending = Vec::new();
    let mut tx = pool.begin().await?;
    let impact = retention::enforce_retention_guard(
        &mut tx,
        claims.org_id,
        "flights",
        confirmation.as_ref(),
    )
    .await?;

    let deleted = delete_and_collect_audit(
        &mut tx,
        claims.org_id,
        "flights",
        "DELETE FROM flights WHERE org_id = $1 \
         RETURNING id, to_jsonb(flights.*) AS row_json",
        &mut pending,
    )
    .await?;

    tx.commit().await?;
    flush_audit(&pool, &claims, pending).await;
    retention::log_purge_audit(
        &pool,
        claims.org_id,
        claims.user_id,
        &claims.user_code,
        &impact,
        deleted,
        confirmation.as_ref(),
    )
    .await;

    Ok(Json(PurgeResult {
        deleted,
        table: "flights".into(),
    }))
}

/// DELETE /api/v1/admin/purge/aircraft
pub async fn purge_aircraft(
    claims: Claims,
    State(pool): State<DbPool>,
    body: Option<Json<PurgeConfirmation>>,
) -> Result<Json<PurgeResult>, AppError> {
    require_admin(&claims)?;
    let confirmation = body.map(|Json(b)| b);

    let mut pending = Vec::new();
    let mut tx = pool.begin().await?;
    let impact = retention::enforce_retention_guard(
        &mut tx,
        claims.org_id,
        "aircraft",
        confirmation.as_ref(),
    )
    .await?;

    // 1. Audit + delete maintenance_schedules first. The FK is CASCADE so
    //    leaving this to the parent DELETE would silently destroy the rows
    //    with no audit trail. (maintenance_schedules.aircraft_id is also
    //    NOT NULL, so the previous "UPDATE … SET aircraft_id = NULL"
    //    approach would have raised a constraint violation.)
    delete_and_collect_audit(
        &mut tx,
        claims.org_id,
        "maintenance_schedules",
        "DELETE FROM maintenance_schedules \
         WHERE aircraft_id IN (SELECT id FROM aircraft WHERE org_id = $1) \
         RETURNING id, to_jsonb(maintenance_schedules.*) AS row_json",
        &mut pending,
    )
    .await?;

    // 2. Bulk-nullify SET NULL FK references on remaining tables. Each
    //    UPDATE produces a single summary audit row (skipped if no rows
    //    affected).
    for (table, sql) in [
        (
            "flights",
            "UPDATE flights SET aircraft_id = NULL WHERE org_id = $1",
        ),
        (
            "batteries",
            "UPDATE batteries SET aircraft_id = NULL WHERE org_id = $1",
        ),
        (
            "remote_controllers",
            "UPDATE remote_controllers SET aircraft_id = NULL WHERE org_id = $1",
        ),
        (
            "maintenance",
            "UPDATE maintenance SET aircraft_id = NULL WHERE org_id = $1",
        ),
        (
            "defect_log",
            "UPDATE defect_log SET aircraft_id = NULL WHERE org_id = $1",
        ),
        (
            "incidents",
            "UPDATE incidents SET aircraft_id = NULL WHERE org_id = $1",
        ),
    ] {
        nullify_and_collect_audit(
            &mut tx,
            claims.org_id,
            table,
            "aircraft_id",
            sql,
            &mut pending,
        )
        .await?;
    }

    // 3. Delete aircraft (per-row audit).
    let deleted = delete_and_collect_audit(
        &mut tx,
        claims.org_id,
        "aircraft",
        "DELETE FROM aircraft WHERE org_id = $1 \
         RETURNING id, to_jsonb(aircraft.*) AS row_json",
        &mut pending,
    )
    .await?;

    tx.commit().await?;
    flush_audit(&pool, &claims, pending).await;
    retention::log_purge_audit(
        &pool,
        claims.org_id,
        claims.user_id,
        &claims.user_code,
        &impact,
        deleted,
        confirmation.as_ref(),
    )
    .await;

    Ok(Json(PurgeResult {
        deleted,
        table: "aircraft".into(),
    }))
}

/// DELETE /api/v1/admin/purge/batteries
pub async fn purge_batteries(
    claims: Claims,
    State(pool): State<DbPool>,
    body: Option<Json<PurgeConfirmation>>,
) -> Result<Json<PurgeResult>, AppError> {
    require_admin(&claims)?;
    let confirmation = body.map(|Json(b)| b);

    let mut pending = Vec::new();
    let mut tx = pool.begin().await?;
    let impact = retention::enforce_retention_guard(
        &mut tx,
        claims.org_id,
        "batteries",
        confirmation.as_ref(),
    )
    .await?;

    // 1. Audit + delete battery_tests (CASCADE child).
    delete_and_collect_audit(
        &mut tx,
        claims.org_id,
        "battery_tests",
        "DELETE FROM battery_tests \
         WHERE battery_id IN (SELECT id FROM batteries WHERE org_id = $1) \
         RETURNING id, to_jsonb(battery_tests.*) AS row_json",
        &mut pending,
    )
    .await?;

    // 2. Bulk-nullify flights.battery_id.
    nullify_and_collect_audit(
        &mut tx,
        claims.org_id,
        "flights",
        "battery_id",
        "UPDATE flights SET battery_id = NULL WHERE org_id = $1",
        &mut pending,
    )
    .await?;

    // 3. Delete batteries.
    let deleted = delete_and_collect_audit(
        &mut tx,
        claims.org_id,
        "batteries",
        "DELETE FROM batteries WHERE org_id = $1 \
         RETURNING id, to_jsonb(batteries.*) AS row_json",
        &mut pending,
    )
    .await?;

    tx.commit().await?;
    flush_audit(&pool, &claims, pending).await;
    retention::log_purge_audit(
        &pool,
        claims.org_id,
        claims.user_id,
        &claims.user_code,
        &impact,
        deleted,
        confirmation.as_ref(),
    )
    .await;

    Ok(Json(PurgeResult {
        deleted,
        table: "batteries".into(),
    }))
}

/// DELETE /api/v1/admin/purge/controllers
pub async fn purge_controllers(
    claims: Claims,
    State(pool): State<DbPool>,
    body: Option<Json<PurgeConfirmation>>,
) -> Result<Json<PurgeResult>, AppError> {
    require_admin(&claims)?;
    let confirmation = body.map(|Json(b)| b);

    let mut pending = Vec::new();
    let mut tx = pool.begin().await?;
    let impact = retention::enforce_retention_guard(
        &mut tx,
        claims.org_id,
        "controllers",
        confirmation.as_ref(),
    )
    .await?;

    // No CASCADE child tables. Just bulk-nullify flights.controller_id.
    nullify_and_collect_audit(
        &mut tx,
        claims.org_id,
        "flights",
        "controller_id",
        "UPDATE flights SET controller_id = NULL WHERE org_id = $1",
        &mut pending,
    )
    .await?;

    let deleted = delete_and_collect_audit(
        &mut tx,
        claims.org_id,
        "remote_controllers",
        "DELETE FROM remote_controllers WHERE org_id = $1 \
         RETURNING id, to_jsonb(remote_controllers.*) AS row_json",
        &mut pending,
    )
    .await?;

    tx.commit().await?;
    flush_audit(&pool, &claims, pending).await;
    retention::log_purge_audit(
        &pool,
        claims.org_id,
        claims.user_id,
        &claims.user_code,
        &impact,
        deleted,
        confirmation.as_ref(),
    )
    .await;

    Ok(Json(PurgeResult {
        deleted,
        table: "remote_controllers".into(),
    }))
}

/// DELETE /api/v1/admin/purge/pilots
pub async fn purge_pilots(
    claims: Claims,
    State(pool): State<DbPool>,
    body: Option<Json<PurgeConfirmation>>,
) -> Result<Json<PurgeResult>, AppError> {
    require_admin(&claims)?;
    let confirmation = body.map(|Json(b)| b);

    let mut pending = Vec::new();
    let mut tx = pool.begin().await?;
    let impact = retention::enforce_retention_guard(
        &mut tx,
        claims.org_id,
        "pilots",
        confirmation.as_ref(),
    )
    .await?;

    // 1. Audit + delete pilot_qualifications (CASCADE child).
    delete_and_collect_audit(
        &mut tx,
        claims.org_id,
        "pilot_qualifications",
        "DELETE FROM pilot_qualifications \
         WHERE pilot_id IN (SELECT id FROM pilots WHERE org_id = $1) \
         RETURNING id, to_jsonb(pilot_qualifications.*) AS row_json",
        &mut pending,
    )
    .await?;

    // 2. Bulk-nullify FK references.
    for (table, column, sql) in [
        (
            "flights",
            "pilot_id",
            "UPDATE flights SET pilot_id = NULL WHERE org_id = $1",
        ),
        (
            "flights",
            "pilot_monitoring_id",
            "UPDATE flights SET pilot_monitoring_id = NULL WHERE org_id = $1",
        ),
        (
            "incidents",
            "pilot_id",
            "UPDATE incidents SET pilot_id = NULL WHERE org_id = $1",
        ),
    ] {
        nullify_and_collect_audit(&mut tx, claims.org_id, table, column, sql, &mut pending)
            .await?;
    }

    // 3. Delete pilots. (users.pilot_id has FK SET NULL — auto-cleared by
    //    the database with no audit row; this is intentional.)
    let deleted = delete_and_collect_audit(
        &mut tx,
        claims.org_id,
        "pilots",
        "DELETE FROM pilots WHERE org_id = $1 \
         RETURNING id, to_jsonb(pilots.*) AS row_json",
        &mut pending,
    )
    .await?;

    tx.commit().await?;
    flush_audit(&pool, &claims, pending).await;
    retention::log_purge_audit(
        &pool,
        claims.org_id,
        claims.user_id,
        &claims.user_code,
        &impact,
        deleted,
        confirmation.as_ref(),
    )
    .await;

    Ok(Json(PurgeResult {
        deleted,
        table: "pilots".into(),
    }))
}

/// DELETE /api/v1/admin/purge/incidents
pub async fn purge_incidents(
    claims: Claims,
    State(pool): State<DbPool>,
    body: Option<Json<PurgeConfirmation>>,
) -> Result<Json<PurgeResult>, AppError> {
    require_admin(&claims)?;
    let confirmation = body.map(|Json(b)| b);

    // Retention guard runs BEFORE any attachment file is unlinked — a
    // refused purge must leave the files on disk. The transaction is opened
    // here so the guard's counts and the DELETE below share one snapshot.
    let mut tx = pool.begin().await?;
    let impact = retention::enforce_retention_guard(
        &mut tx,
        claims.org_id,
        "incidents",
        confirmation.as_ref(),
    )
    .await?;

    // Unlink every attachment file for the org's incidents BEFORE the bulk
    // DELETE. The incident_attachments FK is CASCADE, so once the incident
    // rows go the attachment rows (and their file_path values) are gone too.
    let attachment_paths = sqlx::query(
        "SELECT ia.file_path FROM incident_attachments ia WHERE ia.org_id = $1",
    )
    .bind(claims.org_id)
    .fetch_all(&pool)
    .await?;
    for row in &attachment_paths {
        let file_path: String = match row.try_get("file_path") {
            Ok(p) => p,
            Err(_) => continue,
        };
        let path = std::path::Path::new(
            &std::env::var("UPLOAD_DIR").unwrap_or_else(|_| "./uploads".to_string()),
        )
        .join(&file_path);
        match std::fs::remove_file(&path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                tracing::warn!(
                    "purge_incidents: failed to unlink attachment file {:?}: {:?}",
                    path,
                    e
                );
            }
        }
    }

    let mut pending = Vec::new();

    // incident_attachments and incident_notes are CASCADE children of
    // incidents; their rows are removed by the parent DELETE. (The attachment
    // files on disk were unlinked above.)
    let deleted = delete_and_collect_audit(
        &mut tx,
        claims.org_id,
        "incidents",
        "DELETE FROM incidents WHERE org_id = $1 \
         RETURNING id, to_jsonb(incidents.*) AS row_json",
        &mut pending,
    )
    .await?;

    tx.commit().await?;
    flush_audit(&pool, &claims, pending).await;
    retention::log_purge_audit(
        &pool,
        claims.org_id,
        claims.user_id,
        &claims.user_code,
        &impact,
        deleted,
        confirmation.as_ref(),
    )
    .await;

    Ok(Json(PurgeResult {
        deleted,
        table: "incidents".into(),
    }))
}

#[cfg(test)]
#[path = "purge_tests.rs"]
mod tests;
