// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

//! GET /api/v1/flights/:id/detail — full data block for the flight detail page.
//!
//! Single round-trip for the page. The existing `GET /flights/:id`
//! intentionally stays slim (returns the bare row) and remains the canonical
//! endpoint for the listing/edit flows. This `/detail` endpoint resolves
//! joined entities (aircraft, battery, controller, pilot), pulls active
//! maintenance schedules for the aircraft (computed against current totals),
//! and surfaces the imported-by user from the audit_log row.

use axum::{
    extract::{Path, State},
    Json,
};
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::collections::HashMap;

use crate::auth::middleware::Claims;
use crate::db::DbPool;
use crate::errors::AppError;
use crate::models::flight::Flight;
use crate::models::maintenance_schedule::{MaintenanceSchedule, ScheduleWithStatus};
use crate::services::schedule_calc::{build_with_status, get_fleet_totals, AircraftTotalsRow};

#[derive(Debug, Serialize)]
pub struct FlightDetailResponse {
    pub flight: Flight,
    pub aircraft: Option<AircraftRef>,
    pub battery: Option<BatteryRef>,
    pub controller: Option<ControllerRef>,
    pub pilot: Option<PilotRef>,
    /// Active schedules on the flight's aircraft, with current status
    /// computed against the aircraft's running totals. Empty when the
    /// flight has no aircraft assigned. No per-flight attribution — the
    /// list is informational ("here's what's on the horizon for this
    /// drone") and links out to the schedules page.
    pub aircraft_schedules: Vec<ScheduleWithStatus>,
    /// Audit info: who imported the flight and when. NULL when there's
    /// no INSERT row in audit_log (very old flights pre-audit).
    pub audit: Option<FlightAudit>,
    /// Operationally-relevant findings extracted from the flight log
    /// (ArduPilot .bin today). Empty for flights with no findings.
    pub findings: Vec<FlightFinding>,
}

/// A row from `flight_log_findings`, in API shape.
#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct FlightFinding {
    pub id: i32,
    pub source: String,
    pub code: String,
    pub subsystem: Option<String>,
    pub subsys_id: Option<i32>,
    pub ecode: Option<i32>,
    pub severity: String,
    pub message: String,
    pub occurrence_count: i32,
    pub first_offset_seconds: Option<f32>,
    pub last_offset_seconds: Option<f32>,
    pub resolved: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct AircraftRef {
    pub id: i32,
    pub equipment_id: Option<String>,
    pub serial_number: String,
    pub nickname: Option<String>,
    pub model: Option<String>,
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct BatteryRef {
    pub id: i32,
    pub equipment_id: Option<String>,
    pub serial_number: String,
    pub compatible_model: Option<String>,
    pub status: String,
}

#[derive(Debug, Serialize)]
pub struct ControllerRef {
    pub id: i32,
    pub equipment_id: Option<String>,
    pub serial_number: String,
    pub model: Option<String>,
    pub nickname: Option<String>,
    pub status: String,
}

#[derive(Debug, Serialize)]
pub struct PilotRef {
    pub id: i32,
    pub name: String,
    pub user_code: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct FlightAudit {
    pub imported_by_user_id: Option<i32>,
    pub imported_by_user_code: Option<String>,
    pub imported_at: DateTime<Utc>,
}

pub async fn get_flight_detail(
    claims: Claims,
    State(pool): State<DbPool>,
    Path(flight_id): Path<i32>,
) -> Result<Json<FlightDetailResponse>, AppError> {
    // Fetch the flight (org-scoped, soft-delete excluded). 404 leaks no info
    // about another org's flights.
    let flight = sqlx::query_as::<_, Flight>(
        "SELECT * FROM flights WHERE id = $1 AND org_id = $2 AND deleted_at IS NULL",
    )
    .bind(flight_id)
    .bind(claims.org_id)
    .fetch_optional(&pool)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Flight {} not found", flight_id)))?;

    // Resolved aircraft + model name (joined).
    let aircraft = if let Some(aid) = flight.aircraft_id {
        #[derive(sqlx::FromRow)]
        struct Row {
            id: i32,
            equipment_id: Option<String>,
            serial_number: String,
            nickname: Option<String>,
            model: Option<String>,
            status: String,
        }
        sqlx::query_as::<_, Row>(
            "SELECT a.id, a.equipment_id, a.serial_number, a.nickname, \
                    am.name AS model, a.status \
             FROM aircraft a \
             LEFT JOIN aircraft_models am ON a.model_id = am.id \
             WHERE a.id = $1 AND a.org_id = $2",
        )
        .bind(aid)
        .bind(claims.org_id)
        .fetch_optional(&pool)
        .await?
        .map(|r| AircraftRef {
            id: r.id,
            equipment_id: r.equipment_id,
            serial_number: r.serial_number,
            nickname: r.nickname,
            model: r.model,
            status: r.status,
        })
    } else {
        None
    };

    let battery = if let Some(bid) = flight.battery_id {
        #[derive(sqlx::FromRow)]
        struct Row {
            id: i32,
            equipment_id: Option<String>,
            serial_number: String,
            compatible_model: Option<String>,
            status: String,
        }
        sqlx::query_as::<_, Row>(
            "SELECT id, equipment_id, serial_number, compatible_model, status \
             FROM batteries WHERE id = $1 AND org_id = $2",
        )
        .bind(bid)
        .bind(claims.org_id)
        .fetch_optional(&pool)
        .await?
        .map(|r| BatteryRef {
            id: r.id,
            equipment_id: r.equipment_id,
            serial_number: r.serial_number,
            compatible_model: r.compatible_model,
            status: r.status,
        })
    } else {
        None
    };

    let controller = if let Some(cid) = flight.controller_id {
        #[derive(sqlx::FromRow)]
        struct Row {
            id: i32,
            equipment_id: Option<String>,
            serial_number: String,
            model: Option<String>,
            nickname: Option<String>,
            status: String,
        }
        sqlx::query_as::<_, Row>(
            "SELECT id, equipment_id, serial_number, model, nickname, status \
             FROM remote_controllers WHERE id = $1 AND org_id = $2",
        )
        .bind(cid)
        .bind(claims.org_id)
        .fetch_optional(&pool)
        .await?
        .map(|r| ControllerRef {
            id: r.id,
            equipment_id: r.equipment_id,
            serial_number: r.serial_number,
            model: r.model,
            nickname: r.nickname,
            status: r.status,
        })
    } else {
        None
    };

    // Pilot + linked user_code (LEFT JOIN users so the user_code is null when
    // the pilot has no login account — same pattern as pilot_detail).
    let pilot = if let Some(pid) = flight.pilot_id {
        #[derive(sqlx::FromRow)]
        struct Row {
            id: i32,
            name: String,
            user_code: Option<String>,
        }
        sqlx::query_as::<_, Row>(
            "SELECT p.id, p.name, \
                    (SELECT user_code FROM users \
                       WHERE pilot_id = p.id AND org_id = p.org_id \
                       ORDER BY id LIMIT 1) AS user_code \
             FROM pilots p \
             WHERE p.id = $1 AND p.org_id = $2",
        )
        .bind(pid)
        .bind(claims.org_id)
        .fetch_optional(&pool)
        .await?
        .map(|r| PilotRef {
            id: r.id,
            name: r.name,
            user_code: r.user_code,
        })
    } else {
        None
    };

    // Active maintenance schedules on this aircraft, with current status.
    // Reuses the same helpers the schedules endpoint uses so /detail can
    // never disagree with /maintenance-schedules.
    let aircraft_schedules: Vec<ScheduleWithStatus> = if let Some(aid) = flight.aircraft_id {
        let schedules = sqlx::query_as::<_, MaintenanceSchedule>(
            "SELECT * FROM maintenance_schedules \
             WHERE org_id = $1 AND aircraft_id = $2 AND is_active = TRUE \
             ORDER BY name",
        )
        .bind(claims.org_id)
        .bind(aid)
        .fetch_all(&pool)
        .await?;

        if schedules.is_empty() {
            Vec::new()
        } else {
            // get_fleet_totals scans the org once. For per-flight detail this
            // is a small overhead (org-wide aggregate, indexed by aircraft_id);
            // the alternative — a single-aircraft variant — would duplicate
            // SQL with no observable benefit. Reuse trumps micro-optimisation.
            let totals = get_fleet_totals(&pool, claims.org_id).await?;
            let map: HashMap<i32, &AircraftTotalsRow> =
                totals.iter().map(|r| (r.aircraft_id, r)).collect();
            let now = Utc::now();
            schedules
                .into_iter()
                .map(|s| build_with_status(s, &map, now))
                .collect()
        }
    } else {
        Vec::new()
    };

    // Imported-by — pull the INSERT audit row. Defensive ORDER BY created_at
    // ASC LIMIT 1 in case multiple INSERT rows ever existed (shouldn't, but
    // costs nothing to be safe). NULL when no row exists (pre-audit flights).
    #[derive(sqlx::FromRow)]
    struct AuditRow {
        user_id: Option<i32>,
        user_code: Option<String>,
        created_at: DateTime<Utc>,
    }
    let audit = sqlx::query_as::<_, AuditRow>(
        "SELECT user_id, user_code, created_at \
         FROM audit_log \
         WHERE org_id = $1 AND table_name = 'flights' \
           AND record_id = $2 AND action = 'INSERT' \
         ORDER BY created_at ASC \
         LIMIT 1",
    )
    .bind(claims.org_id)
    .bind(flight_id)
    .fetch_optional(&pool)
    .await?
    .map(|r| FlightAudit {
        imported_by_user_id: r.user_id,
        imported_by_user_code: r.user_code,
        imported_at: r.created_at,
    });

    // Log findings for this flight, most-relevant ordering (severity desc,
    // then chronological). Severity rank keeps critical/high at the top.
    let findings = sqlx::query_as::<_, FlightFinding>(
        "SELECT id, source, code, subsystem, subsys_id, ecode, severity, message, \
                occurrence_count, first_offset_seconds, last_offset_seconds, resolved \
         FROM flight_log_findings \
         WHERE org_id = $1 AND flight_id = $2 \
         ORDER BY CASE severity \
                    WHEN 'critical' THEN 0 WHEN 'high' THEN 1 \
                    WHEN 'medium' THEN 2 WHEN 'low' THEN 3 ELSE 4 END, \
                  first_offset_seconds NULLS LAST, id",
    )
    .bind(claims.org_id)
    .bind(flight_id)
    .fetch_all(&pool)
    .await?;

    Ok(Json(FlightDetailResponse {
        flight,
        aircraft,
        battery,
        controller,
        pilot,
        aircraft_schedules,
        audit,
        findings,
    }))
}
