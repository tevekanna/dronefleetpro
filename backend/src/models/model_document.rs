// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

use chrono::{DateTime, Utc};
use serde::Serialize;

#[derive(Debug, Serialize, sqlx::FromRow, Clone)]
pub struct ModelDocument {
    pub id: i32,
    pub model_id: i32,
    pub org_id: i32,
    pub category: String,
    /// Required when category='other' (operator-supplied label so multiple
    /// 'other' docs per model can be told apart). Must be NULL for
    /// instruction_manual / maintenance_manual — enforced by CHECK
    /// `model_documents_title_required_for_other`.
    pub title: Option<String>,
    pub original_filename: String,
    pub file_path: String,
    pub size_bytes: i64,
    pub mime_type: String,
    pub uploaded_by: i32,
    pub uploaded_at: DateTime<Utc>,
}

/// Public-facing list / response shape. Excludes `file_path` — that's an
/// internal storage detail and exposing it would leak the on-disk layout.
#[derive(Debug, Serialize, Clone)]
pub struct ModelDocumentResponse {
    pub id: i32,
    pub model_id: i32,
    pub category: String,
    pub title: Option<String>,
    pub original_filename: String,
    pub size_bytes: i64,
    pub mime_type: String,
    pub uploaded_by: i32,
    pub uploaded_at: DateTime<Utc>,
}

impl From<ModelDocument> for ModelDocumentResponse {
    fn from(d: ModelDocument) -> Self {
        Self {
            id: d.id,
            model_id: d.model_id,
            category: d.category,
            title: d.title,
            original_filename: d.original_filename,
            size_bytes: d.size_bytes,
            mime_type: d.mime_type,
            uploaded_by: d.uploaded_by,
            uploaded_at: d.uploaded_at,
        }
    }
}
