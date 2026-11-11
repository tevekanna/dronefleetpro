// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Debug, sqlx::FromRow, Clone)]
pub struct Checklist {
    pub id: i32,
    pub org_id: i32,
    pub model_id: i32,
    pub r#type: String,
    pub title: String,
    pub revision: String,
    pub revision_date: Option<NaiveDate>,
    pub created_by: i32,
    pub updated_by: Option<i32>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Header-only summary used by the list endpoint. Includes denormalised
/// counts of sections and items so the catalog UI can render an at-a-glance
/// "n sections / m items" line without a follow-up fetch.
#[derive(Serialize, Debug)]
pub struct ChecklistSummary {
    pub id: i32,
    pub model_id: i32,
    pub r#type: String,
    pub title: String,
    pub revision: String,
    pub revision_date: Option<NaiveDate>,
    pub updated_at: DateTime<Utc>,
    pub section_count: i64,
    pub item_count: i64,
}

#[derive(Serialize, Debug, sqlx::FromRow, Clone)]
pub struct ChecklistSection {
    pub id: i32,
    pub checklist_id: i32,
    pub title: String,
    pub sort_order: i32,
    pub start_on_new_page: bool,
}

#[derive(Serialize, Debug, sqlx::FromRow, Clone)]
pub struct ChecklistItem {
    pub id: i32,
    pub section_id: i32,
    pub label: String,
    pub value: String,
    pub responsibility: String,
    pub sort_order: i32,
}

/// Full nested response shape for GET /checklists/:id and the POST/PUT
/// responses. `#[serde(flatten)]` lifts the parent's columns into the
/// top-level JSON object so the client sees one merged record per level.
#[derive(Serialize, Debug)]
pub struct ChecklistFull {
    #[serde(flatten)]
    pub checklist: Checklist,
    pub sections: Vec<ChecklistSectionFull>,
}

#[derive(Serialize, Debug)]
pub struct ChecklistSectionFull {
    #[serde(flatten)]
    pub section: ChecklistSection,
    pub items: Vec<ChecklistItem>,
}

/// Request body for POST (create) and PUT (replace-style update).
/// Server diffs internally — clients submit the full intended state on
/// every save.
#[derive(Deserialize, Debug)]
pub struct ChecklistInputBody {
    pub r#type: String,
    pub title: String,
    /// Optional on create (defaults to '01'). Required on update if the
    /// caller wants to bump it; otherwise the existing value is reused.
    pub revision: Option<String>,
    pub revision_date: Option<NaiveDate>,
    pub sections: Vec<ChecklistSectionInput>,
}

#[derive(Deserialize, Debug)]
pub struct ChecklistSectionInput {
    pub title: String,
    pub sort_order: i32,
    pub start_on_new_page: bool,
    pub items: Vec<ChecklistItemInput>,
}

#[derive(Deserialize, Debug)]
pub struct ChecklistItemInput {
    pub label: String,
    pub value: String,
    pub responsibility: String,
    pub sort_order: i32,
}
