// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::FromRow;

#[derive(Debug, Serialize)]
pub struct AuditLogListItem {
    pub id: i64,
    pub action: String,
    pub table_name: String,
    pub record_id: Option<i32>,
    pub user_id: Option<i32>,
    pub user_code: Option<String>,
    pub user_name: Option<String>,
    pub ip_address: Option<String>,
    pub created_at: DateTime<Utc>,
    pub has_diff: bool,
    /// Populated only for UPDATE entries with a before/after diff; null otherwise.
    pub changed_fields: Option<Vec<String>>,
}

#[derive(Debug, Serialize)]
pub struct AuditLogListResponse {
    pub items: Vec<AuditLogListItem>,
    pub page: i64,
    pub per_page: i64,
    pub total: i64,
    pub total_pages: i64,
}

#[derive(Debug, Serialize)]
pub struct ChangedFieldDetail {
    pub field: String,
    pub before: serde_json::Value,
    pub after: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct AuditLogDetail {
    pub id: i64,
    pub action: String,
    pub table_name: String,
    pub record_id: Option<i32>,
    pub user_id: Option<i32>,
    pub user_code: Option<String>,
    pub user_name: Option<String>,
    pub ip_address: Option<String>,
    pub created_at: DateTime<Utc>,
    pub diff: Option<serde_json::Value>,
    /// Structured diff for UPDATE; null for INSERT and DELETE.
    pub changed_fields: Option<Vec<ChangedFieldDetail>>,
}

#[derive(Debug, Serialize, FromRow)]
pub struct AuditUserOption {
    pub id: i32,
    pub name: String,
    pub user_code: String,
}

#[derive(Debug, Serialize)]
pub struct AuditLogFilters {
    pub tables: Vec<String>,
    pub users: Vec<AuditUserOption>,
    pub actions: Vec<String>,
}
