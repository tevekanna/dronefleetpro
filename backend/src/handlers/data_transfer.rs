// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

//! Data export/import handlers.
//!
//! Admin-only. Export produces a JSON snapshot of selected entity groups.
//! Import restores from such a snapshot, with dry-run preview and conflict
//! resolution strategies.
//!
//! See docs/data-transfer-design.md for the full design (TBD).

use axum::{
    extract::{Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::Row;

use crate::auth::middleware::{require_admin, Claims};
use crate::db::DbPool;
use crate::errors::AppError;
use crate::services::{
    audit,
    data_export::{self, EntityGroup},
    data_import::{self, ConflictStrategy},
};

#[derive(Debug, Deserialize)]
pub struct ExportPreviewQuery {
    /// Comma-separated entity groups, or the literal "all".
    /// e.g. `equipment,pilots,flights` or `all`.
    pub entities: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ExportPreviewResponse {
    pub counts: Value,
    pub estimated_size_bytes: i64,
}

/// Request body for POST /api/v1/data/export. Accepts either the literal
/// string `"all"` or an array of group names.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum EntitiesField {
    All(String),
    List(Vec<String>),
}

#[derive(Debug, Deserialize)]
pub struct ExportBody {
    entities: EntitiesField,
}

// ---------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------

fn parse_entities_str(raw: &str) -> Result<Vec<EntityGroup>, AppError> {
    let trimmed = raw.trim();
    if trimmed.eq_ignore_ascii_case("all") {
        return Ok(EntityGroup::all().to_vec());
    }
    let mut out = Vec::new();
    for part in trimmed.split(',') {
        let p = part.trim();
        if p.is_empty() {
            continue;
        }
        let g = EntityGroup::from_str(p)
            .ok_or_else(|| AppError::BadRequest(format!("Unknown entity group: {p}")))?;
        if !out.contains(&g) {
            out.push(g);
        }
    }
    if out.is_empty() {
        return Err(AppError::BadRequest(
            "entities must contain at least one group".into(),
        ));
    }
    Ok(out)
}

fn parse_entities_field(field: &EntitiesField) -> Result<Vec<EntityGroup>, AppError> {
    match field {
        EntitiesField::All(s) => {
            if s.eq_ignore_ascii_case("all") {
                Ok(EntityGroup::all().to_vec())
            } else {
                parse_entities_str(s)
            }
        }
        EntitiesField::List(items) => {
            let mut out = Vec::new();
            for s in items {
                let g = EntityGroup::from_str(s)
                    .ok_or_else(|| AppError::BadRequest(format!("Unknown entity group: {s}")))?;
                if !out.contains(&g) {
                    out.push(g);
                }
            }
            if out.is_empty() {
                return Err(AppError::BadRequest(
                    "entities must contain at least one group".into(),
                ));
            }
            Ok(out)
        }
    }
}

// ---------------------------------------------------------------
// GET /api/v1/data/export/preview
// ---------------------------------------------------------------

pub async fn export_preview(
    claims: Claims,
    State(pool): State<DbPool>,
    Query(q): Query<ExportPreviewQuery>,
) -> Result<Json<ExportPreviewResponse>, AppError> {
    require_admin(&claims)?;

    let raw = q.entities.as_deref().ok_or_else(|| {
        AppError::BadRequest("entities query parameter is required".into())
    })?;
    let groups = parse_entities_str(raw)?;

    let counts = data_export::count_entities(&pool, claims.org_id, &groups).await?;
    let estimated_size_bytes = data_export::estimate_size_bytes(&counts);

    Ok(Json(ExportPreviewResponse {
        counts: Value::Object(counts),
        estimated_size_bytes,
    }))
}

// ---------------------------------------------------------------
// POST /api/v1/data/export
// ---------------------------------------------------------------

pub async fn export(
    claims: Claims,
    State(pool): State<DbPool>,
    Json(body): Json<ExportBody>,
) -> Result<Response, AppError> {
    require_admin(&claims)?;

    let groups = parse_entities_field(&body.entities)?;
    let group_names: Vec<String> = groups.iter().map(|g| g.as_str().to_string()).collect();

    // Insert tracking row in 'running' state.
    let job_row = sqlx::query(
        "INSERT INTO data_transfer_jobs \
            (org_id, user_id, user_code, job_type, status, entity_groups, options) \
         VALUES ($1, $2, $3, 'export', 'running', $4, $5) \
         RETURNING id",
    )
    .bind(claims.org_id)
    .bind(claims.user_id)
    .bind(&claims.user_code)
    .bind(serde_json::to_value(&group_names).unwrap_or(Value::Null))
    .bind(Value::Null)
    .fetch_one(&pool)
    .await?;
    let job_id: i64 = job_row.try_get("id")?;

    // Build export.
    match data_export::build_export(&pool, claims.org_id, &groups).await {
        Ok((doc, counts)) => {
            let summary = Value::Object(counts);

            // Mark job complete.
            let _ = sqlx::query(
                "UPDATE data_transfer_jobs \
                 SET status = 'completed', summary = $1, completed_at = NOW() \
                 WHERE id = $2",
            )
            .bind(&summary)
            .bind(job_id)
            .execute(&pool)
            .await;

            audit::log_export(
                &pool,
                claims.org_id,
                claims.user_id,
                &claims.user_code,
                job_id,
                summary.clone(),
            )
            .await;

            // Build response with attachment headers.
            let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
            let filename = format!(
                "dronefleetpro_export_{}_{}.json",
                claims.org_id, timestamp
            );
            let body_str = serde_json::to_string(&doc)
                .map_err(|e| AppError::Internal(anyhow::anyhow!("serialise export: {e}")))?;

            let mut headers = HeaderMap::new();
            headers.insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/json"),
            );
            headers.insert(
                header::CONTENT_DISPOSITION,
                HeaderValue::from_str(&format!("attachment; filename=\"{filename}\""))
                    .unwrap_or_else(|_| HeaderValue::from_static("attachment")),
            );
            Ok((StatusCode::OK, headers, body_str).into_response())
        }
        Err(e) => {
            let msg = format!("{:?}", e);
            let _ = sqlx::query(
                "UPDATE data_transfer_jobs \
                 SET status = 'failed', error_message = $1, completed_at = NOW() \
                 WHERE id = $2",
            )
            .bind(&msg)
            .bind(job_id)
            .execute(&pool)
            .await;
            Err(e)
        }
    }
}

// ---------------------------------------------------------------
// GET /api/v1/data/jobs
// ---------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ListJobsQuery {
    pub limit: Option<i64>,
}

/// Returns the most recent data_transfer_jobs rows for the caller's org.
/// Used by the Data Management page to show export/import history.
pub async fn list_jobs(
    claims: Claims,
    State(pool): State<DbPool>,
    Query(q): Query<ListJobsQuery>,
) -> Result<Json<Vec<Value>>, AppError> {
    require_admin(&claims)?;
    let limit = q.limit.unwrap_or(10).clamp(1, 50);
    let rows = sqlx::query(
        "SELECT to_jsonb(t) AS row FROM ( \
            SELECT id, job_type, status, entity_groups, user_code, summary, \
                   error_message, started_at, completed_at \
              FROM data_transfer_jobs \
             WHERE org_id = $1 \
             ORDER BY id DESC \
             LIMIT $2 \
         ) t",
    )
    .bind(claims.org_id)
    .bind(limit)
    .fetch_all(&pool)
    .await?;

    let result: Vec<Value> = rows
        .into_iter()
        .map(|r| r.try_get::<Value, _>("row").unwrap_or(Value::Null))
        .collect();
    Ok(Json(result))
}

// ---------------------------------------------------------------
// Import
// ---------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct ImportRequest {
    pub document: Value,
    #[serde(default = "default_strategy")]
    pub strategy: ConflictStrategy,
    /// Entity groups to exclude from this import even if present in the
    /// document. Names are parsed via `EntityGroup::from_str`; an unknown
    /// name is rejected with 400. Empty (default) imports everything.
    #[serde(default)]
    pub skip_groups: Vec<String>,
}

fn default_strategy() -> ConflictStrategy {
    ConflictStrategy::Skip
}

/// Parse the request's `skip_groups` names into `EntityGroup`s, rejecting
/// unknown names with 400.
fn parse_skip_groups(names: &[String]) -> Result<Vec<EntityGroup>, AppError> {
    let mut out = Vec::new();
    for name in names {
        let g = EntityGroup::from_str(name)
            .ok_or_else(|| AppError::BadRequest(format!("Unknown entity group: {name}")))?;
        if !out.contains(&g) {
            out.push(g);
        }
    }
    Ok(out)
}

pub async fn import_preview(
    claims: Claims,
    State(pool): State<DbPool>,
    Json(body): Json<ImportRequest>,
) -> Result<Json<data_import::ImportSummary>, AppError> {
    require_admin(&claims)?;
    let skip_groups = parse_skip_groups(&body.skip_groups)?;
    let summary = data_import::preview_import(
        &pool,
        claims.org_id,
        claims.user_id,
        &claims.user_code,
        body.document,
        body.strategy,
        &skip_groups,
    )
    .await?;
    Ok(Json(summary))
}

pub async fn import_apply(
    claims: Claims,
    State(pool): State<DbPool>,
    Json(body): Json<ImportRequest>,
) -> Result<Json<data_import::ImportSummary>, AppError> {
    require_admin(&claims)?;
    let skip_groups = parse_skip_groups(&body.skip_groups)?;
    let summary = data_import::apply_import(
        &pool,
        claims.org_id,
        claims.user_id,
        &claims.user_code,
        body.document,
        body.strategy,
        &skip_groups,
    )
    .await?;
    Ok(Json(summary))
}

