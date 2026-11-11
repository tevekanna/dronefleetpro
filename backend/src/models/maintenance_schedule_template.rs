// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

/// A per-(org, model) default schedule. New aircraft of this model spawn
/// per-individual rows in `maintenance_schedules` from these templates.
#[derive(Debug, Serialize, Deserialize, FromRow, Clone)]
pub struct MaintenanceScheduleTemplate {
    pub id: i32,
    pub org_id: i32,
    pub model_id: i32,
    pub name: String,
    pub description: Option<String>,
    pub interval_type: String,
    pub interval_value: f32,
    pub warning_threshold: f32,
    pub is_critical: bool,
    pub secondary_interval_type: Option<String>,
    pub secondary_interval_value: Option<f32>,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub created_by: Option<String>,
}

/// List-response row: template + count of maintenance_schedules that
/// were spawned from it. A schedule is "spawned from" a template when
/// its name matches the template's name AND its aircraft uses the
/// template's model — the same scope `sync_template` operates on.
#[derive(Debug, Serialize, FromRow, Clone)]
pub struct MaintenanceScheduleTemplateWithCount {
    pub id: i32,
    pub org_id: i32,
    pub model_id: i32,
    pub name: String,
    pub description: Option<String>,
    pub interval_type: String,
    pub interval_value: f32,
    pub warning_threshold: f32,
    pub is_critical: bool,
    pub secondary_interval_type: Option<String>,
    pub secondary_interval_value: Option<f32>,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub created_by: Option<String>,
    pub reference_count: i64,
}

#[derive(Debug, Deserialize)]
pub struct CreateScheduleTemplateRequest {
    pub model_id: i32,
    pub name: String,
    pub description: Option<String>,
    pub interval_type: String,
    pub interval_value: f32,
    pub warning_threshold: Option<f32>,
    pub is_critical: Option<bool>,
    pub secondary_interval_type: Option<String>,
    pub secondary_interval_value: Option<f32>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateScheduleTemplateRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub interval_type: Option<String>,
    pub interval_value: Option<f32>,
    pub warning_threshold: Option<f32>,
    pub is_critical: Option<bool>,
    pub secondary_interval_type: Option<String>,
    pub secondary_interval_value: Option<f32>,
    pub is_active: Option<bool>,
}

#[derive(Debug, Deserialize, Default)]
pub struct SyncTemplateRequest {
    /// Restrict the sync to a subset of aircraft. If empty / missing,
    /// every aircraft in the org with a matching schedule name is
    /// synced. Aircraft outside the org are silently filtered out.
    pub aircraft_ids: Option<Vec<i32>>,
}

#[derive(Debug, Serialize)]
pub struct SyncTemplateResponse {
    /// Aircraft that had their schedule rewritten from the template.
    pub synced: i64,
    /// Aircraft that did not have a schedule with this template's name —
    /// no update was performed for them.
    pub skipped_no_schedule: i64,
    /// Total schedule rows updated. Matches `synced` today (one schedule
    /// per aircraft+name pair, enforced by the unique constraint), but
    /// surfaced separately for symmetry with seed-defaults.
    pub schedules_updated: i64,
}
