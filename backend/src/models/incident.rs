// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use chrono::{DateTime, Utc};

use crate::services::occurrence_reporting::ReportingStatus;

#[derive(Debug, Serialize, Deserialize, FromRow, Clone)]
pub struct Incident {
    pub id: i32,
    pub org_id: i32,
    pub flight_id: Option<i32>,
    pub pilot_id: Option<i32>,
    pub aircraft_id: Option<i32>,
    pub occurred_at: DateTime<Utc>,
    pub severity: String,
    pub category: String,
    pub location_description: Option<String>,
    pub location_lat: Option<f32>,
    pub location_lon: Option<f32>,
    pub description: String,
    pub immediate_action: Option<String>,
    pub root_cause: Option<String>,
    pub corrective_action: Option<String>,
    pub reported_to_authority: bool,
    pub authority_ref: Option<String>,
    pub status: String,
    /// Annex VI occurrence code (migration 054), validated against
    /// `services::occurrence_taxonomy`.
    pub occurrence_code: Option<String>,
    /// Start of the 72-hour clock under Article 4(7) of Reg (EU) No 376/2014.
    pub became_aware_at: Option<DateTime<Utc>>,
    pub reported_at: Option<DateTime<Utc>>,
    pub reporting_delay_reason: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub created_by_user_id: Option<i32>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateIncidentRequest {
    pub occurred_at: String,
    pub severity: String,
    pub category: String,
    pub description: String,
    pub flight_id: Option<i32>,
    pub pilot_id: Option<i32>,
    pub aircraft_id: Option<i32>,
    pub location_description: Option<String>,
    pub immediate_action: Option<String>,
    pub reported_to_authority: Option<bool>,
    pub occurrence_code: Option<String>,
    pub became_aware_at: Option<DateTime<Utc>>,
    pub reported_at: Option<DateTime<Utc>>,
    pub reporting_delay_reason: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateIncidentRequest {
    pub status: Option<String>,
    pub root_cause: Option<String>,
    pub corrective_action: Option<String>,
    pub authority_ref: Option<String>,
    // Tri-state fields: omitted -> keep, explicit null -> clear to NULL,
    // value -> set. All four must be clearable (a misclassification, a
    // wrongly entered timestamp or an obsolete delay reason must be
    // removable, not just overwritable), so COALESCE cannot be used.
    #[serde(default, deserialize_with = "crate::models::serde_helpers::double_option")]
    pub occurrence_code: Option<Option<String>>,
    #[serde(default, deserialize_with = "crate::models::serde_helpers::double_option")]
    pub became_aware_at: Option<Option<DateTime<Utc>>>,
    #[serde(default, deserialize_with = "crate::models::serde_helpers::double_option")]
    pub reported_at: Option<Option<DateTime<Utc>>>,
    #[serde(default, deserialize_with = "crate::models::serde_helpers::double_option")]
    pub reporting_delay_reason: Option<Option<String>>,
}

/// One row of `GET /api/v1/incidents` — the incident plus its computed
/// occurrence-reporting status.
#[derive(Debug, Serialize)]
pub struct IncidentListItem {
    #[serde(flatten)]
    pub incident: Incident,
    pub reporting: ReportingStatus,
}

/// A file attached to an incident. Content lives on the filesystem under
/// `${UPLOAD_DIR}/incidents/{incident_id}/{attachment_id}.{ext}`, mirroring
/// `model_documents`. `file_path` is internal storage detail and is never
/// exposed to API callers (see `IncidentAttachmentResponse`).
#[derive(Debug, Serialize, FromRow, Clone)]
pub struct IncidentAttachment {
    pub id: i32,
    pub incident_id: i32,
    pub org_id: i32,
    pub kind: String,
    pub original_filename: String,
    pub file_path: String,
    pub size_bytes: i64,
    pub mime_type: String,
    pub uploaded_by: i32,
    pub uploaded_at: DateTime<Utc>,
}

/// Public-facing attachment shape. Excludes `file_path` — exposing it would
/// leak the on-disk storage layout.
#[derive(Debug, Serialize, Clone)]
pub struct IncidentAttachmentResponse {
    pub id: i32,
    pub incident_id: i32,
    pub kind: String,
    pub original_filename: String,
    pub size_bytes: i64,
    pub mime_type: String,
    pub uploaded_by: i32,
    pub uploaded_at: DateTime<Utc>,
}

impl From<IncidentAttachment> for IncidentAttachmentResponse {
    fn from(a: IncidentAttachment) -> Self {
        Self {
            id: a.id,
            incident_id: a.incident_id,
            kind: a.kind,
            original_filename: a.original_filename,
            size_bytes: a.size_bytes,
            mime_type: a.mime_type,
            uploaded_by: a.uploaded_by,
            uploaded_at: a.uploaded_at,
        }
    }
}

/// An append-only note on an incident. The API exposes no update or delete —
/// notes are preserved intact for EASA traceability.
#[derive(Debug, Serialize, FromRow, Clone)]
pub struct IncidentNote {
    pub id: i32,
    pub incident_id: i32,
    pub note: String,
    pub created_by: Option<i32>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct AddNoteRequest {
    pub note: String,
}

/// Detail payload for `GET /api/v1/incidents/:id` — the incident plus its
/// attachments and notes.
#[derive(Debug, Serialize)]
pub struct IncidentDetailResponse {
    pub incident: Incident,
    pub reporting: ReportingStatus,
    pub attachments: Vec<IncidentAttachmentResponse>,
    pub notes: Vec<IncidentNote>,
}
