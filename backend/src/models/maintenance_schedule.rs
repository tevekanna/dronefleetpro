// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sqlx::FromRow;

#[derive(Debug, Serialize, Deserialize, FromRow, Clone)]
pub struct MaintenanceSchedule {
    pub id: i32,
    pub org_id: i32,
    pub aircraft_id: i32,
    pub name: String,
    pub description: Option<String>,
    pub interval_type: String,
    pub interval_value: f32,
    pub warning_threshold: f32,
    pub is_critical: bool,
    pub last_performed_at: Option<DateTime<Utc>>,
    pub last_performed_flight_hours: f32,
    pub last_performed_cycles: i32,
    pub next_due_at: Option<DateTime<Utc>>,
    pub next_due_flight_hours: Option<f32>,
    pub next_due_cycles: Option<i32>,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub secondary_interval_type: Option<String>,
    pub secondary_interval_value: Option<f32>,
    pub secondary_next_due_at: Option<DateTime<Utc>>,
    pub secondary_next_due_hours: Option<f32>,
    pub created_by: Option<String>,
    pub instruction_template_id: Option<i32>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateMaintenanceScheduleRequest {
    pub aircraft_id: i32,
    pub name: String,
    pub description: Option<String>,
    pub interval_type: String,
    pub interval_value: f32,
    pub warning_threshold: Option<f32>,
    pub is_critical: Option<bool>,
    pub last_performed_at: Option<DateTime<Utc>>,
    pub last_performed_flight_hours: Option<f32>,
    pub last_performed_cycles: Option<i32>,
    pub secondary_interval_type: Option<String>,
    pub secondary_interval_value: Option<f32>,
    pub instruction_template_id: Option<i32>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdateMaintenanceScheduleRequest {
    pub aircraft_id: Option<i32>,
    pub name: Option<String>,
    pub description: Option<String>,
    pub interval_type: Option<String>,
    pub interval_value: Option<f32>,
    pub warning_threshold: Option<f32>,
    pub is_critical: Option<bool>,
    pub last_performed_at: Option<DateTime<Utc>>,
    pub last_performed_flight_hours: Option<f32>,
    pub last_performed_cycles: Option<i32>,
    pub is_active: Option<bool>,
    pub secondary_interval_type: Option<String>,
    pub secondary_interval_value: Option<f32>,
    pub instruction_template_id: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct CompleteScheduleRequest {
    pub performed_date: NaiveDate,
    pub action_taken: String,
    pub remark: String,
    pub maint_type: String,
    pub sign_off: String,
}

#[derive(Debug, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ScheduleStatus {
    Ok,
    Warning,
    Overdue,
}

#[derive(Debug, Serialize)]
pub struct ScheduleWithStatus {
    #[serde(flatten)]
    pub schedule: MaintenanceSchedule,
    pub aircraft_equipment_id: Option<String>,
    pub aircraft_model: Option<String>,
    pub aircraft_model_id: Option<i32>,
    pub aircraft_serial_number: String,
    pub status: ScheduleStatus,
    pub consumption_pct: f64,
    pub secondary_consumption_pct: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct ScheduleDashboard {
    pub overdue: usize,
    pub warning: usize,
    pub ok: usize,
    pub items: Vec<ScheduleWithStatus>,
}

#[derive(Debug, Serialize, FromRow)]
pub struct ScheduleHistoryItem {
    pub maintenance_id: i32,
    pub performed_date: NaiveDate,
    pub created_by: String,
    pub remark: String,
    pub action_taken: String,
    pub maint_type: String,
    pub sign_off: String,
}

#[derive(Debug, Serialize)]
pub struct ScheduleDetail {
    #[serde(flatten)]
    pub with_status: ScheduleWithStatus,
    pub history: Vec<ScheduleHistoryItem>,
}
