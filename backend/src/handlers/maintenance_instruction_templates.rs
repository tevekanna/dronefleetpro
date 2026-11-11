// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

//! API for maintenance instruction templates (inspection checklists) and their
//! checklist items.
//!
//! Permission: any authenticated role may list and read templates (same as
//! checklists). Write endpoints (create / patch / edit items / new revision)
//! are admin-only.
//!
//! Org isolation: every query is scoped by `org_id` from the JWT Claims. A
//! template that doesn't exist in the caller's org returns 404 (not 403) so
//! cross-org IDs don't leak existence — matches the checklists convention.
//!
//! ## Editing and EASA traceability
//!
//! `maintenance_checklist_completions` reference checklist items directly, so
//! an item that has ever been completed must never be mutated or deleted.
//! Editing therefore has two paths:
//!   * **in place** (`PUT /:id/items`) — allowed only when NO item of the
//!     template has any completion (typo fixes on fresh templates);
//!   * **new revision** (`POST /:id/revisions`) — otherwise: a new template row
//!     (same name/model/source, revision bumped) is created, the edited items
//!     inserted, every `maintenance_schedules.instruction_template_id` re-pointed
//!     from the old row to the new one, and the old row deactivated. Old items
//!     and their completions stay untouched.

use axum::extract::{Path, Query, State};
use axum::Json;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Postgres, Transaction};

use crate::auth::middleware::Claims;
use crate::db::DbPool;
use crate::errors::AppError;
use crate::services::audit;

#[cfg(test)]
#[path = "maintenance_instruction_templates_tests.rs"]
mod tests;

/// Row shape for the list endpoint — template scalar fields plus a count of
/// its checklist items.
#[derive(Debug, Serialize, Deserialize, FromRow)]
pub struct MaintenanceTemplateListItem {
    pub id: i32,
    pub org_id: i32,
    pub aircraft_model_id: Option<i32>,
    pub name: String,
    pub description: Option<String>,
    pub revision: String,
    pub source: String,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub item_count: i64,
}

/// Query string for the list endpoint.
#[derive(Debug, Deserialize)]
pub struct ListTemplatesQuery {
    #[serde(default)]
    pub aircraft_model_id: Option<i32>,
}

/// A single checklist item within a section (detail response).
#[derive(Debug, Serialize)]
pub struct ChecklistItemSummary {
    pub id: i32,
    pub item_order: i32,
    pub item_text: String,
    pub is_required: bool,
    pub reference_manual: Option<String>,
}

/// A group of checklist items sharing a section (detail response).
#[derive(Debug, Serialize)]
pub struct Section {
    pub section_name: String,
    pub section_order: i32,
    pub items: Vec<ChecklistItemSummary>,
}

/// Full template detail — scalar fields plus the nested sections.
#[derive(Debug, Serialize)]
pub struct MaintenanceTemplateDetail {
    pub id: i32,
    pub org_id: i32,
    pub aircraft_model_id: Option<i32>,
    pub name: String,
    pub description: Option<String>,
    pub revision: String,
    pub source: String,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub sections: Vec<Section>,
}

/// Internal row shape for the detail template fetch (no item_count).
#[derive(Debug, FromRow)]
struct TemplateRow {
    id: i32,
    org_id: i32,
    aircraft_model_id: Option<i32>,
    name: String,
    description: Option<String>,
    revision: String,
    source: String,
    is_active: bool,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

#[derive(Debug, serde::Deserialize)]
pub struct CreateItemInput {
    pub item_text: String,
    pub is_required: bool,
    pub reference_manual: Option<String>,
    pub item_order: i32,
}

#[derive(Debug, serde::Deserialize)]
pub struct CreateSectionInput {
    pub section_name: String,
    pub section_order: i32,
    pub items: Vec<CreateItemInput>,
}

#[derive(Debug, serde::Deserialize)]
pub struct CreateTemplateRequest {
    pub name: String,
    pub aircraft_model_id: Option<i32>,
    pub description: Option<String>,
    pub revision: Option<String>,  // defaults to "01"
    pub sections: Vec<CreateSectionInput>,
}

#[derive(Debug, serde::Deserialize)]
pub struct PatchTemplateRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub revision: Option<String>,
    pub is_active: Option<bool>,
}

/// Body for both `PUT /:id/items` (edit in place) and `POST /:id/revisions`
/// (create the next revision). `sections` is the full replacement content;
/// `name`/`description` are optional metadata updates applied in the same
/// transaction so a single save is atomic. (Metadata-only edits can also go
/// through `PATCH /:id`, which this does not replace.)
#[derive(Debug, serde::Deserialize)]
pub struct EditTemplateRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub sections: Vec<CreateSectionInput>,
}

/// Shared validation for the create / edit / revision payloads: a template
/// needs at least one section, every section a non-empty name and at least one
/// item, every item non-empty text.
fn validate_sections(sections: &[CreateSectionInput]) -> Result<(), AppError> {
    if sections.is_empty() {
        return Err(AppError::BadRequest("at least one section is required".into()));
    }
    for s in sections {
        if s.section_name.trim().is_empty() {
            return Err(AppError::BadRequest("section_name must not be empty".into()));
        }
        if s.items.is_empty() {
            return Err(AppError::BadRequest(format!(
                "section '{}' must have at least one item",
                s.section_name
            )));
        }
        for item in &s.items {
            if item.item_text.trim().is_empty() {
                return Err(AppError::BadRequest("item_text must not be empty".into()));
            }
        }
    }
    Ok(())
}

/// Insert all items of `sections` for `template_id` inside `tx`. Returns the
/// number of items inserted. Trims text and drops blank `reference_manual`.
async fn insert_items(
    tx: &mut Transaction<'_, Postgres>,
    template_id: i32,
    sections: &[CreateSectionInput],
) -> Result<i32, AppError> {
    let mut total: i32 = 0;
    for section in sections {
        for item in &section.items {
            sqlx::query(
                "INSERT INTO maintenance_checklist_items \
                    (template_id, section_name, section_order, item_order, item_text, \
                     is_required, reference_manual) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7)",
            )
            .bind(template_id)
            .bind(section.section_name.trim())
            .bind(section.section_order)
            .bind(item.item_order)
            .bind(item.item_text.trim())
            .bind(item.is_required)
            .bind(item.reference_manual.as_deref().filter(|s| !s.trim().is_empty()))
            .execute(&mut **tx)
            .await
            .map_err(|e| {
                tracing::error!(error=?e, "insert_items: INSERT item failed");
                AppError::Database(e)
            })?;
            total += 1;
        }
    }
    Ok(total)
}

/// True when any completion references any item of this template — the gate
/// that forces the new-revision path.
async fn template_has_completions(pool: &DbPool, template_id: i32) -> Result<bool, AppError> {
    let count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM maintenance_checklist_completions c \
         JOIN maintenance_checklist_items i ON i.id = c.checklist_item_id \
         WHERE i.template_id = $1",
    )
    .bind(template_id)
    .fetch_one(pool)
    .await?;
    Ok(count.0 > 0)
}

/// Compute the next revision label.
/// - Pure numeric (e.g. "01", "09", "99"): increment, zero-padded to at least
///   the original width. "01"->"02", "09"->"10", "99"->"100".
/// - Otherwise: bump a trailing "-<n>" counter, or append "-2" when absent.
///   "A"->"A-2", "Rev1"->"Rev1-2", "A-2"->"A-3".
fn next_revision(current: &str) -> String {
    let trimmed = current.trim();
    if !trimmed.is_empty() && trimmed.chars().all(|c| c.is_ascii_digit()) {
        let n: u64 = trimmed.parse().unwrap_or(0);
        let width = trimmed.len();
        return format!("{:0width$}", n + 1, width = width);
    }
    if let Some(idx) = trimmed.rfind('-') {
        let (head, tail) = trimmed.split_at(idx);
        let num = &tail[1..];
        if !num.is_empty() && num.chars().all(|c| c.is_ascii_digit()) {
            if let Ok(n) = num.parse::<u64>() {
                return format!("{}-{}", head, n + 1);
            }
        }
    }
    format!("{}-2", trimmed)
}

/// Load a template with its grouped sections, scoped to `org_id`. 404 when the
/// id is absent from the caller's org.
async fn load_template_detail(
    pool: &DbPool,
    org_id: i32,
    id: i32,
) -> Result<MaintenanceTemplateDetail, AppError> {
    let template = sqlx::query_as::<_, TemplateRow>(
        "SELECT id, org_id, aircraft_model_id, name, description, revision, \
                source, is_active, created_at, updated_at \
         FROM maintenance_instruction_templates \
         WHERE id = $1 AND org_id = $2",
    )
    .bind(id)
    .bind(org_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| {
        AppError::NotFound(format!("Maintenance instruction template {} not found", id))
    })?;

    let items = sqlx::query_as::<_, ItemRow>(
        "SELECT id, section_name, section_order, item_order, item_text, \
                is_required, reference_manual \
         FROM maintenance_checklist_items \
         WHERE template_id = $1 \
         ORDER BY section_order ASC, item_order ASC",
    )
    .bind(template.id)
    .fetch_all(pool)
    .await?;

    let mut sections: Vec<Section> = Vec::new();
    for item in items {
        let needs_new = match sections.last() {
            Some(s) => s.section_order != item.section_order || s.section_name != item.section_name,
            None => true,
        };
        if needs_new {
            sections.push(Section {
                section_name: item.section_name.clone(),
                section_order: item.section_order,
                items: Vec::new(),
            });
        }
        if let Some(section) = sections.last_mut() {
            section.items.push(ChecklistItemSummary {
                id: item.id,
                item_order: item.item_order,
                item_text: item.item_text,
                is_required: item.is_required,
                reference_manual: item.reference_manual,
            });
        }
    }

    Ok(MaintenanceTemplateDetail {
        id: template.id,
        org_id: template.org_id,
        aircraft_model_id: template.aircraft_model_id,
        name: template.name,
        description: template.description,
        revision: template.revision,
        source: template.source,
        is_active: template.is_active,
        created_at: template.created_at,
        updated_at: template.updated_at,
        sections,
    })
}

/// Internal row shape for the checklist items of a template, ordered.
#[derive(Debug, FromRow)]
struct ItemRow {
    id: i32,
    section_name: String,
    section_order: i32,
    item_order: i32,
    item_text: String,
    is_required: bool,
    reference_manual: Option<String>,
}

// ---------------------------------------------------------------------------
// GET /api/v1/maintenance-instruction-templates
// ---------------------------------------------------------------------------

pub async fn list_templates(
    claims: Claims,
    State(pool): State<DbPool>,
    Query(query): Query<ListTemplatesQuery>,
) -> Result<Json<Vec<MaintenanceTemplateListItem>>, AppError> {
    let mut sql = String::from(
        "SELECT t.id, t.org_id, t.aircraft_model_id, t.name, t.description, \
                t.revision, t.source, t.is_active, t.created_at, t.updated_at, \
                (SELECT count(*) FROM maintenance_checklist_items \
                  WHERE template_id = t.id) AS item_count \
         FROM maintenance_instruction_templates t \
         WHERE t.org_id = $1 AND t.is_active = TRUE",
    );
    if query.aircraft_model_id.is_some() {
        sql.push_str(" AND t.aircraft_model_id = $2");
    }
    sql.push_str(" ORDER BY t.name ASC");

    let mut q = sqlx::query_as::<_, MaintenanceTemplateListItem>(&sql).bind(claims.org_id);
    if let Some(model_id) = query.aircraft_model_id {
        q = q.bind(model_id);
    }

    let rows = q.fetch_all(&pool).await?;
    Ok(Json(rows))
}

// ---------------------------------------------------------------------------
// GET /api/v1/maintenance-instruction-templates/:id
// ---------------------------------------------------------------------------

pub async fn get_template_detail(
    claims: Claims,
    State(pool): State<DbPool>,
    Path(id): Path<i32>,
) -> Result<Json<MaintenanceTemplateDetail>, AppError> {
    let detail = load_template_detail(&pool, claims.org_id, id).await?;
    Ok(Json(detail))
}

// ---------------------------------------------------------------------------
// POST /api/v1/maintenance-instruction-templates  (admin only)
// ---------------------------------------------------------------------------

/// POST /api/v1/maintenance-instruction-templates — admin only.
/// Creates a template and all its sections/items in one transaction.
pub async fn create_template(
    claims: Claims,
    State(pool): State<DbPool>,
    Json(body): Json<CreateTemplateRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    crate::auth::middleware::require_admin(&claims)?;

    let name = body.name.trim().to_string();
    if name.is_empty() {
        return Err(AppError::BadRequest("name is required".into()));
    }
    validate_sections(&body.sections)?;
    let revision = body.revision.as_deref().unwrap_or("01").to_string();

    let mut tx = pool.begin().await?;

    let (template_id,): (i32,) = sqlx::query_as(
        "INSERT INTO maintenance_instruction_templates \
            (org_id, aircraft_model_id, name, description, revision, source, is_active, created_by) \
         VALUES ($1, $2, $3, $4, $5, 'custom', TRUE, $6) \
         RETURNING id"
    )
    .bind(claims.org_id)
    .bind(body.aircraft_model_id)
    .bind(&name)
    .bind(&body.description)
    .bind(&revision)
    .bind(claims.user_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!(error=?e, "create_template: INSERT template failed");
        AppError::Database(e)
    })?;

    let total_items = insert_items(&mut tx, template_id, &body.sections).await?;

    tx.commit().await?;

    Ok(Json(serde_json::json!({
        "id": template_id,
        "name": name,
        "sections": body.sections.len(),
        "items": total_items,
    })))
}

// ---------------------------------------------------------------------------
// PATCH /api/v1/maintenance-instruction-templates/:id  (admin only)
// ---------------------------------------------------------------------------

/// PATCH /api/v1/maintenance-instruction-templates/:id — admin only.
/// Updates name, description, revision, or is_active (deactivate/reactivate).
pub async fn patch_template(
    claims: Claims,
    State(pool): State<DbPool>,
    Path(id): Path<i32>,
    Json(body): Json<PatchTemplateRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    crate::auth::middleware::require_admin(&claims)?;

    // Verify ownership.
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM maintenance_instruction_templates \
         WHERE id = $1 AND org_id = $2)"
    )
    .bind(id)
    .bind(claims.org_id)
    .fetch_one(&pool)
    .await
    .map_err(|e| {
        tracing::error!(error=?e, "patch_template: exists check failed");
        AppError::Database(e)
    })?;

    if !exists {
        return Err(AppError::NotFound(format!("Template {} not found", id)));
    }

    sqlx::query(
        "UPDATE maintenance_instruction_templates SET \
            name        = COALESCE($1, name), \
            description = COALESCE($2, description), \
            revision    = COALESCE($3, revision), \
            is_active   = COALESCE($4, is_active), \
            updated_at  = now() \
         WHERE id = $5 AND org_id = $6"
    )
    .bind(&body.name)
    .bind(&body.description)
    .bind(&body.revision)
    .bind(body.is_active)
    .bind(id)
    .bind(claims.org_id)
    .execute(&pool)
    .await
    .map_err(|e| {
        tracing::error!(error=?e, "patch_template: UPDATE failed");
        AppError::Database(e)
    })?;

    Ok(Json(serde_json::json!({ "id": id, "updated": true })))
}

// ---------------------------------------------------------------------------
// PUT /api/v1/maintenance-instruction-templates/:id/items  (admin only)
// ---------------------------------------------------------------------------

/// Replace a template's checklist items in place. Allowed only when NO item of
/// the template has any completion; otherwise 409 `requires_new_revision` so
/// the caller switches to `POST /:id/revisions`. Optional `name`/`description`
/// are updated in the same transaction.
pub async fn update_items(
    claims: Claims,
    State(pool): State<DbPool>,
    Path(id): Path<i32>,
    Json(body): Json<EditTemplateRequest>,
) -> Result<Json<MaintenanceTemplateDetail>, AppError> {
    crate::auth::middleware::require_admin(&claims)?;
    validate_sections(&body.sections)?;

    // Must exist in this org AND still be active.
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM maintenance_instruction_templates \
         WHERE id = $1 AND org_id = $2 AND is_active = TRUE)",
    )
    .bind(id)
    .bind(claims.org_id)
    .fetch_one(&pool)
    .await?;
    if !exists {
        return Err(AppError::NotFound(format!(
            "Maintenance instruction template {} not found",
            id
        )));
    }

    // Completion gate — items with recorded completions must never be mutated.
    if template_has_completions(&pool, id).await? {
        return Err(AppError::ConflictCode {
            code: "requires_new_revision".into(),
            message: "This checklist has recorded completions and cannot be changed \
                      in place. Save it as a new revision instead."
                .into(),
        });
    }

    let items_before: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM maintenance_checklist_items WHERE template_id = $1",
    )
    .bind(id)
    .fetch_one(&pool)
    .await?;

    let mut tx = pool.begin().await?;

    sqlx::query("DELETE FROM maintenance_checklist_items WHERE template_id = $1")
        .bind(id)
        .execute(&mut *tx)
        .await
        .map_err(|e| {
            tracing::error!(error=?e, "update_items: DELETE items failed");
            AppError::Database(e)
        })?;

    let items_after = insert_items(&mut tx, id, &body.sections).await?;

    // Optional metadata update in the same transaction (atomic single save).
    let new_name = body.name.as_ref().map(|s| s.trim().to_string());
    if let Some(ref n) = new_name {
        if n.is_empty() {
            return Err(AppError::BadRequest("name must not be empty".into()));
        }
    }
    sqlx::query(
        "UPDATE maintenance_instruction_templates SET \
            name        = COALESCE($1, name), \
            description = COALESCE($2, description), \
            updated_at  = now() \
         WHERE id = $3 AND org_id = $4",
    )
    .bind(new_name.as_deref())
    .bind(body.description.as_deref())
    .bind(id)
    .bind(claims.org_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!(error=?e, "update_items: UPDATE metadata failed");
        AppError::Database(e)
    })?;

    tx.commit().await?;

    audit::log_custom_action(
        &pool,
        claims.org_id,
        claims.user_id,
        &claims.user_code,
        "UPDATE_CHECKLIST_ITEMS",
        "maintenance_instruction_templates",
        id,
        serde_json::json!({
            "before": { "item_count": items_before.0 },
            "after": { "item_count": items_after, "name": new_name },
        }),
    )
    .await?;

    let detail = load_template_detail(&pool, claims.org_id, id).await?;
    Ok(Json(detail))
}

// ---------------------------------------------------------------------------
// POST /api/v1/maintenance-instruction-templates/:id/revisions  (admin only)
// ---------------------------------------------------------------------------

/// Response for the new-revision endpoint: the freshly created template plus
/// the number of maintenance schedules re-pointed from the old revision.
#[derive(Debug, Serialize)]
pub struct RevisionResponse {
    pub template: MaintenanceTemplateDetail,
    pub repointed_schedules: u64,
}

/// Create the next revision of a template. Inserts a new template row (same
/// model + source, revision bumped) with the edited items, re-points every
/// `maintenance_schedules.instruction_template_id` from the old row to the new
/// one, and deactivates the old row. Old items + completions are untouched.
pub async fn create_revision(
    claims: Claims,
    State(pool): State<DbPool>,
    Path(id): Path<i32>,
    Json(body): Json<EditTemplateRequest>,
) -> Result<Json<RevisionResponse>, AppError> {
    crate::auth::middleware::require_admin(&claims)?;
    validate_sections(&body.sections)?;

    // Source template must exist in this org and still be active.
    let source = sqlx::query_as::<_, TemplateRow>(
        "SELECT id, org_id, aircraft_model_id, name, description, revision, \
                source, is_active, created_at, updated_at \
         FROM maintenance_instruction_templates \
         WHERE id = $1 AND org_id = $2 AND is_active = TRUE",
    )
    .bind(id)
    .bind(claims.org_id)
    .fetch_optional(&pool)
    .await?
    .ok_or_else(|| {
        AppError::NotFound(format!("Maintenance instruction template {} not found", id))
    })?;

    let new_revision = next_revision(&source.revision);
    let new_name = body
        .name
        .as_ref()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| source.name.clone());
    let new_description = body.description.clone().or_else(|| source.description.clone());

    let mut tx = pool.begin().await?;

    let (new_id,): (i32,) = sqlx::query_as(
        "INSERT INTO maintenance_instruction_templates \
            (org_id, aircraft_model_id, name, description, revision, source, is_active, created_by) \
         VALUES ($1, $2, $3, $4, $5, $6, TRUE, $7) \
         RETURNING id",
    )
    .bind(claims.org_id)
    .bind(source.aircraft_model_id)
    .bind(&new_name)
    .bind(&new_description)
    .bind(&new_revision)
    .bind(&source.source)
    .bind(claims.user_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!(error=?e, "create_revision: INSERT template failed");
        AppError::Database(e)
    })?;

    let items_inserted = insert_items(&mut tx, new_id, &body.sections).await?;

    // Re-point schedules. Safe by design: the FK is ON DELETE SET NULL and
    // completions reference checklist items directly, not this column (mig 042).
    let repointed = sqlx::query(
        "UPDATE maintenance_schedules \
         SET instruction_template_id = $1 \
         WHERE instruction_template_id = $2 AND org_id = $3",
    )
    .bind(new_id)
    .bind(id)
    .bind(claims.org_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!(error=?e, "create_revision: re-point schedules failed");
        AppError::Database(e)
    })?
    .rows_affected();

    sqlx::query(
        "UPDATE maintenance_instruction_templates \
         SET is_active = FALSE, updated_at = now() \
         WHERE id = $1 AND org_id = $2",
    )
    .bind(id)
    .bind(claims.org_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!(error=?e, "create_revision: deactivate source failed");
        AppError::Database(e)
    })?;

    tx.commit().await?;

    // Audit both templates: the new revision (create) and the old one (superseded).
    audit::log_custom_action(
        &pool,
        claims.org_id,
        claims.user_id,
        &claims.user_code,
        "CREATE_CHECKLIST_REVISION",
        "maintenance_instruction_templates",
        new_id,
        serde_json::json!({
            "from_template_id": id,
            "revision": new_revision,
            "items": items_inserted,
            "repointed_schedules": repointed,
        }),
    )
    .await?;
    audit::log_custom_action(
        &pool,
        claims.org_id,
        claims.user_id,
        &claims.user_code,
        "SUPERSEDE_CHECKLIST",
        "maintenance_instruction_templates",
        id,
        serde_json::json!({
            "superseded_by": new_id,
            "old_revision": source.revision,
            "new_revision": new_revision,
            "repointed_schedules": repointed,
            "deactivated": true,
        }),
    )
    .await?;

    let detail = load_template_detail(&pool, claims.org_id, new_id).await?;
    Ok(Json(RevisionResponse {
        template: detail,
        repointed_schedules: repointed,
    }))
}
