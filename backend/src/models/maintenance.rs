// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use chrono::{DateTime, Utc, NaiveDate};

#[derive(Debug, Serialize, Deserialize, FromRow, Clone)]
pub struct MaintenanceRecord {
    pub id: i32,
    pub org_id: i32,
    pub aircraft_id: Option<i32>,
    pub controller_id: Option<i32>,
    pub performed_date: NaiveDate,
    pub flight_id: Option<i32>,
    pub created_by: String,
    pub remark: Option<String>,
    pub action_taken: Option<String>,
    pub maint_category: Option<String>,
    pub maint_type: Option<String>,
    pub sign_off: String,
    pub created_at: DateTime<Utc>,
    pub schedule_id: Option<i32>,
    pub instruction_template_id: Option<i32>,
    pub aircraft_name: Option<String>,
    pub controller_name: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateMaintenanceRequest {
    #[serde(default)]
    pub aircraft_id: Option<i32>,
    #[serde(default)]
    pub controller_id: Option<i32>,
    pub performed_date: String,
    pub flight_id: Option<i32>,
    pub created_by: String,
    pub remark: String,
    pub action_taken: String,
    pub maint_category: String,
    pub maint_type: String,
    pub sign_off: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateMaintenanceRequest {
    pub remark: Option<String>,
    pub action_taken: Option<String>,
    pub sign_off: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MaintenanceAuditEntry {
    pub id: i64,
    pub action: String,
    pub user_id: Option<i32>,
    pub user_code: Option<String>,
    pub user_name: Option<String>,
    pub ip_address: Option<String>,
    pub created_at: DateTime<Utc>,
    pub changed_fields: Vec<String>,
    pub diff_summary: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MaintenanceDetail {
    pub record: MaintenanceRecord,
    pub audit: Vec<MaintenanceAuditEntry>,
}

#[derive(Debug, Serialize, Deserialize, FromRow, Clone)]
pub struct MaintenanceInstructionTemplate {
    pub id: i32,
    pub org_id: i32,
    pub aircraft_model_id: Option<i32>,
    pub name: String,
    pub description: Option<String>,
    pub revision: String,
    pub source: String,
    pub is_active: bool,
    pub created_by: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, FromRow, Clone)]
pub struct MaintenanceChecklistItem {
    pub id: i32,
    pub template_id: i32,
    pub section_name: String,
    pub section_order: i32,
    pub item_order: i32,
    pub item_text: String,
    pub is_required: bool,
    pub reference_manual: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, FromRow, Clone)]
pub struct MaintenanceChecklistCompletion {
    pub id: i32,
    pub maintenance_id: i32,
    pub checklist_item_id: i32,
    pub status: String,
    pub note: Option<String>,
    pub completed_by: Option<String>,
    pub completed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CompletionInput {
    pub checklist_item_id: i32,
    pub status: String,          // 'ok' | 'nok' | 'na' | 'pending'
    pub note: Option<String>,
    pub completed_by: String,    // 3-letter user_code
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SaveCompletionsRequest {
    pub template_id: i32,
    pub completions: Vec<CompletionInput>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SaveCompletionsResponse {
    pub maintenance_id: i32,
    pub template_id: i32,
    pub saved: usize,
    pub defects_created: usize,
}

// ── Maintenance Hub ──────────────────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct HubFleetSummary {
    pub active_aircraft:    i64,
    pub overdue_schedules:  i64,
    pub due_soon_schedules: i64,
    pub open_defects:       i64,
}

#[derive(Debug, Serialize, FromRow)]
pub struct HubAlert {
    pub schedule_id:    i32,
    pub schedule_name:  String,
    pub interval_type:  String,
    pub equipment_id:   Option<String>,
    pub aircraft_name:  Option<String>,
    pub severity:       String,          // "overdue" | "due_soon"
    pub overdue_days:   Option<i64>,     // calendar_days only
    pub overdue_hours:  Option<f64>,     // flight_hours only
    pub overdue_cycles: Option<i64>,     // cycles only
    pub due_in_days:    Option<i64>,     // due_soon calendar_days
}

#[derive(Debug, Serialize, FromRow)]
pub struct HubUpcoming {
    pub schedule_id:   i32,
    pub schedule_name: String,
    pub next_due_at:   Option<DateTime<Utc>>,
    pub equipment_id:  Option<String>,
    pub aircraft_name: Option<String>,
}

#[derive(Debug, Serialize, FromRow)]
pub struct HubRecentActivity {
    pub id:             i32,
    pub performed_date: NaiveDate,
    pub maint_type:     Option<String>,
    pub created_by:     String,
    pub equipment_id:   Option<String>,
    pub equipment_name: Option<String>,
}

#[derive(Debug, Serialize, FromRow)]
pub struct HubOpenDefect {
    pub id:           i32,
    pub defect_date:  NaiveDate,
    pub description:  String,
    pub status:       String,
    pub equipment_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct MaintenanceHubData {
    pub summary:         HubFleetSummary,
    pub alerts:          Vec<HubAlert>,
    pub upcoming:        Vec<HubUpcoming>,
    pub recent_activity: Vec<HubRecentActivity>,
    pub open_defects:    Vec<HubOpenDefect>,
}

// ── Maintenance Hub v2 — aircraft-centric ────────────────────────────────────

/// Per-aircraft detail payload returned by `GET /maintenance/aircraft/:id`.
/// `total_hours` is `aircraft.total_seconds / 3600.0`. Schedules carry
/// their computed status (overdue/warning/ok) via the existing
/// `ScheduleWithStatus` shape so the frontend can reuse status badges.
#[derive(Debug, Serialize)]
pub struct AircraftMaintenanceDetail {
    pub aircraft_id: i32,
    pub equipment_id: Option<String>,
    pub aircraft_name: String,
    pub serial_number: String,
    pub model: Option<String>,
    pub model_id: Option<i32>,
    pub status: String,
    pub airworthiness_status: String,
    pub airworthiness_reason: Option<String>,
    pub total_flights: i32,
    pub total_hours: f64,
    pub last_flight: Option<DateTime<Utc>>,
    pub schedules: Vec<crate::models::maintenance_schedule::ScheduleWithStatus>,
    pub open_defects: Vec<HubOpenDefect>,
    pub recent_maintenance: Vec<HubRecentActivity>,
    pub drafts: Vec<DraftSummary>,
}

/// One row in the fleet-status aircraft list returned by
/// `GET /maintenance/fleet-status`. Counts are pre-aggregated so the
/// frontend can render the dashboard table without a second roundtrip.
#[derive(Debug, Serialize, FromRow)]
pub struct FleetMaintenanceRow {
    pub aircraft_id: i32,
    pub equipment_id: Option<String>,
    pub aircraft_name: String,
    pub serial_number: String,
    pub model: Option<String>,
    pub airworthiness_status: String,
    pub overdue_count: i64,
    pub due_soon_count: i64,
    pub open_defect_count: i64,
    pub next_due_label: Option<String>,
    pub next_due_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
pub struct FleetMaintenanceStatus {
    pub airworthy_count: i64,
    pub not_airworthy_count: i64,
    pub overdue_count: i64,
    pub due_soon_count: i64,
    pub open_defect_count: i64,
    pub aircraft: Vec<FleetMaintenanceRow>,
}

// ── Maintenance Drafts ──────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize, FromRow)]
pub struct MaintenanceDraft {
    pub id: i32,
    pub org_id: i32,
    pub schedule_id: i32,
    pub saved_by: String,
    pub form_data: serde_json::Value,
    pub checklist_status: serde_json::Value,
    pub checklist_notes: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, serde::Serialize, sqlx::FromRow)]
pub struct DraftSummary {
    pub schedule_id: i32,
    pub items_marked: i32,
    pub items_total: i32,
}

#[derive(Debug, Deserialize)]
pub struct UpsertDraftRequest {
    pub saved_by: String,
    pub form_data: serde_json::Value,
    pub checklist_status: serde_json::Value,
    pub checklist_notes: serde_json::Value,
}
