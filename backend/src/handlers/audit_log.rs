// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

use axum::{
    extract::{Path, Query, State},
    Json,
};
use chrono::{DateTime, NaiveDate, Utc};
use serde::Deserialize;
use sqlx::{Postgres, QueryBuilder, Row};

use crate::auth::middleware::{require_role, Claims};
use crate::db::DbPool;
use crate::errors::AppError;
use crate::models::audit_log::{
    AuditLogDetail, AuditLogFilters, AuditLogListItem, AuditLogListResponse, AuditUserOption,
};
use crate::services::audit_diff::{compute_changed_fields, summarise_changed_fields};

// PURGE is legacy data from earlier versions — not written by current code, but
// kept here so admins can still filter the historical rows.
// LOGIN_*, LOGOUT, TOKEN_REDEEMED are auth-event actions — see
// `services::audit::log_auth_event`.
const VALID_ACTIONS: &[&str] = &[
    "INSERT",
    "UPDATE",
    "DELETE",
    "EXPORT",
    "IMPORT",
    "PURGE",
    "LOGIN_SUCCESS",
    "LOGIN_FAILURE",
    "LOGOUT",
    "TOKEN_REDEEMED",
];
const DEFAULT_PER_PAGE: i64 = 50;
const MAX_PER_PAGE: i64 = 200;

#[derive(Debug, Deserialize, Default)]
pub struct ListQueryParams {
    pub from: Option<NaiveDate>,
    pub to: Option<NaiveDate>,
    pub user_id: Option<i32>,
    pub table_name: Option<String>,
    pub action: Option<String>,
    pub search: Option<String>,
    pub page: Option<i64>,
    pub per_page: Option<i64>,
}

fn clamp(val: i64, min: i64, max: i64) -> i64 {
    val.max(min).min(max)
}

fn from_bound(d: NaiveDate) -> Result<DateTime<Utc>, AppError> {
    d.and_hms_opt(0, 0, 0)
        .map(|ndt| ndt.and_utc())
        .ok_or_else(|| AppError::BadRequest("invalid 'from' date".into()))
}

fn to_bound(d: NaiveDate) -> Result<DateTime<Utc>, AppError> {
    d.and_hms_opt(23, 59, 59)
        .map(|ndt| ndt.and_utc())
        .ok_or_else(|| AppError::BadRequest("invalid 'to' date".into()))
}

fn validate_action(a: &str) -> Result<(), AppError> {
    if !VALID_ACTIONS.contains(&a) {
        return Err(AppError::BadRequest(format!(
            "action must be one of: {VALID_ACTIONS:?}"
        )));
    }
    Ok(())
}

fn apply_filters<'a>(
    qb: &mut QueryBuilder<'a, Postgres>,
    params: &'a ListQueryParams,
    from_dt: Option<DateTime<Utc>>,
    to_dt: Option<DateTime<Utc>>,
    search: Option<&'a str>,
) {
    if let Some(dt) = from_dt {
        qb.push(" AND al.created_at >= ").push_bind(dt);
    }
    if let Some(dt) = to_dt {
        qb.push(" AND al.created_at <= ").push_bind(dt);
    }
    if let Some(uid) = params.user_id {
        qb.push(" AND al.user_id = ").push_bind(uid);
    }
    if let Some(ref table) = params.table_name {
        qb.push(" AND al.table_name = ").push_bind(table);
    }
    if let Some(ref action) = params.action {
        qb.push(" AND al.action = ").push_bind(action);
    }
    if let Some(needle) = search {
        // TODO(perf): When audit_log grows beyond ~100k rows, add a pg_trgm GIN index:
        //   CREATE EXTENSION IF NOT EXISTS pg_trgm;
        //   CREATE INDEX idx_audit_diff_trgm ON audit_log USING gin ((diff_jsonb::text) gin_trgm_ops);
        // Requires a separate migration — not part of this task.
        qb.push(" AND al.diff_jsonb::text ILIKE '%' || ")
            .push_bind(needle)
            .push(" || '%'");
    }
}

/// GET /api/v1/audit-log
pub async fn list(
    claims: Claims,
    State(pool): State<DbPool>,
    Query(params): Query<ListQueryParams>,
) -> Result<Json<AuditLogListResponse>, AppError> {
    require_role(&claims, &["admin", "safety_officer"])?;

    if let Some(ref a) = params.action {
        validate_action(a)?;
    }
    let from_dt = params.from.map(from_bound).transpose()?;
    let to_dt = params.to.map(to_bound).transpose()?;

    // Whitespace-only or empty search → treat as not provided.
    let search = params
        .search
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());

    let page = clamp(params.page.unwrap_or(1), 1, i64::MAX);
    let per_page = clamp(params.per_page.unwrap_or(DEFAULT_PER_PAGE), 1, MAX_PER_PAGE);
    let offset = (page - 1) * per_page;

    // Count query
    let mut count_qb: QueryBuilder<Postgres> =
        QueryBuilder::new("SELECT COUNT(*) AS total FROM audit_log al WHERE al.org_id = ");
    count_qb.push_bind(claims.org_id);
    apply_filters(&mut count_qb, &params, from_dt, to_dt, search);
    let total: i64 = count_qb.build().fetch_one(&pool).await?.get("total");

    // Page query
    let mut page_qb: QueryBuilder<Postgres> = QueryBuilder::new(
        "SELECT al.id, al.action, al.table_name, al.record_id, al.user_id, al.user_code, \
                u.name AS user_name, al.ip_address, al.created_at, al.diff_jsonb \
         FROM audit_log al \
         LEFT JOIN users u ON u.id = al.user_id \
         WHERE al.org_id = ",
    );
    page_qb.push_bind(claims.org_id);
    apply_filters(&mut page_qb, &params, from_dt, to_dt, search);
    page_qb
        .push(" ORDER BY al.created_at DESC, al.id DESC LIMIT ")
        .push_bind(per_page)
        .push(" OFFSET ")
        .push_bind(offset);

    let rows = page_qb.build().fetch_all(&pool).await?;

    let items: Vec<AuditLogListItem> = rows
        .iter()
        .map(|row| {
            let diff_jsonb: Option<serde_json::Value> = row.get("diff_jsonb");
            let action: String = row.get("action");
            let has_diff = diff_jsonb.is_some();
            let changed_fields = if action == "UPDATE" {
                diff_jsonb.as_ref().map(|d| summarise_changed_fields(d))
            } else {
                None
            };
            AuditLogListItem {
                id: row.get("id"),
                action,
                table_name: row.get("table_name"),
                record_id: row.get("record_id"),
                user_id: row.get("user_id"),
                user_code: row.get("user_code"),
                user_name: row.get("user_name"),
                ip_address: row.get("ip_address"),
                created_at: row.get("created_at"),
                has_diff,
                changed_fields,
            }
        })
        .collect();

    let total_pages = if per_page > 0 {
        (total + per_page - 1) / per_page
    } else {
        0
    };

    Ok(Json(AuditLogListResponse {
        items,
        page,
        per_page,
        total,
        total_pages,
    }))
}

/// GET /api/v1/audit-log/:id
pub async fn get(
    claims: Claims,
    State(pool): State<DbPool>,
    Path(id): Path<i64>,
) -> Result<Json<AuditLogDetail>, AppError> {
    require_role(&claims, &["admin", "safety_officer"])?;

    let row = sqlx::query(
        "SELECT al.id, al.action, al.table_name, al.record_id, al.user_id, al.user_code, \
                u.name AS user_name, al.ip_address, al.created_at, al.diff_jsonb \
         FROM audit_log al \
         LEFT JOIN users u ON u.id = al.user_id \
         WHERE al.id = $1 AND al.org_id = $2",
    )
    .bind(id)
    .bind(claims.org_id)
    .fetch_optional(&pool)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Audit log entry {id} not found")))?;

    let diff: Option<serde_json::Value> = row.get("diff_jsonb");
    let action: String = row.get("action");
    let changed_fields = if action == "UPDATE" {
        diff.as_ref().map(|d| compute_changed_fields(d))
    } else {
        None
    };

    Ok(Json(AuditLogDetail {
        id: row.get("id"),
        action,
        table_name: row.get("table_name"),
        record_id: row.get("record_id"),
        user_id: row.get("user_id"),
        user_code: row.get("user_code"),
        user_name: row.get("user_name"),
        ip_address: row.get("ip_address"),
        created_at: row.get("created_at"),
        diff,
        changed_fields,
    }))
}

/// GET /api/v1/audit-log/filters
pub async fn filters(
    claims: Claims,
    State(pool): State<DbPool>,
) -> Result<Json<AuditLogFilters>, AppError> {
    require_role(&claims, &["admin", "safety_officer"])?;

    let tables_rows = sqlx::query(
        "SELECT DISTINCT table_name FROM audit_log \
         WHERE org_id = $1 ORDER BY table_name",
    )
    .bind(claims.org_id)
    .fetch_all(&pool)
    .await?;
    let tables: Vec<String> = tables_rows
        .iter()
        .map(|r| r.get::<String, _>("table_name"))
        .collect();

    // Users who have actually appeared in this org's audit log. Joining keeps us
    // from listing users who have never performed an audited action.
    let users = sqlx::query_as::<_, AuditUserOption>(
        "SELECT DISTINCT u.id, u.name, u.user_code \
         FROM audit_log al \
         JOIN users u ON u.id = al.user_id \
         WHERE al.org_id = $1 AND al.user_id IS NOT NULL \
         ORDER BY u.name",
    )
    .bind(claims.org_id)
    .fetch_all(&pool)
    .await?;

    let action_rows = sqlx::query(
        "SELECT DISTINCT action FROM audit_log \
         WHERE org_id = $1 ORDER BY action",
    )
    .bind(claims.org_id)
    .fetch_all(&pool)
    .await?;
    let actions: Vec<String> = action_rows
        .iter()
        .map(|r| r.get::<String, _>("action"))
        .collect();

    Ok(Json(AuditLogFilters {
        tables,
        users,
        actions,
    }))
}
