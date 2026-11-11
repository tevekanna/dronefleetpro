// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

use chrono::{DateTime, Duration, Utc};
use sqlx::FromRow;
use std::collections::HashMap;

use crate::db::DbPool;
use crate::errors::AppError;
use crate::models::maintenance_schedule::{
    MaintenanceSchedule, ScheduleStatus, ScheduleWithStatus,
};

#[derive(Debug, Clone, Copy, Default)]
pub struct AircraftTotals {
    pub flight_hours: f64,
    pub flight_count: i64,
}

#[derive(Debug, FromRow)]
pub struct AircraftTotalsRow {
    pub aircraft_id: i32,
    pub equipment_id: Option<String>,
    pub model: Option<String>,
    pub model_id: Option<i32>,
    pub serial_number: String,
    pub flight_hours: f64,
    pub flight_count: i64,
}

pub const VALID_INTERVAL_TYPES: &[&str] = &["flight_hours", "calendar_days", "cycles"];
pub const VALID_SECONDARY_TYPES: &[&str] = &["flight_hours", "calendar_days"];

/// Default fraction-of-interval after which a schedule flips to `Warning`.
/// The DB column `maintenance_schedules.warning_threshold` is the source of
/// truth per row — this constant is only the fallback when no explicit
/// value is supplied (e.g. by `create_schedule` when the request omits it,
/// or when a caller needs a default without a row context).
pub const DEFAULT_WARNING_THRESHOLD: f32 = 0.80;

pub fn validate_primary_type(t: &str) -> Result<(), AppError> {
    if !VALID_INTERVAL_TYPES.contains(&t) {
        return Err(AppError::BadRequest(format!(
            "interval_type must be one of: {VALID_INTERVAL_TYPES:?}"
        )));
    }
    Ok(())
}

pub fn validate_secondary(t: Option<&str>, v: Option<f32>) -> Result<(), AppError> {
    match (t, v) {
        (Some(t), Some(v)) => {
            if t == "cycles" {
                return Err(AppError::BadRequest(
                    "Secondary interval 'cycles' is not supported — use 'flight_hours' or 'calendar_days'".into(),
                ));
            }
            if !VALID_SECONDARY_TYPES.contains(&t) {
                return Err(AppError::BadRequest(format!(
                    "secondary_interval_type must be one of: {VALID_SECONDARY_TYPES:?}"
                )));
            }
            if v <= 0.0 {
                return Err(AppError::BadRequest(
                    "secondary_interval_value must be greater than 0".into(),
                ));
            }
            Ok(())
        }
        (None, None) => Ok(()),
        _ => Err(AppError::BadRequest(
            "secondary_interval_type and secondary_interval_value must be provided together".into(),
        )),
    }
}

pub fn parse_status(raw: &str) -> Result<ScheduleStatus, AppError> {
    match raw.to_ascii_lowercase().as_str() {
        "ok" => Ok(ScheduleStatus::Ok),
        "warning" => Ok(ScheduleStatus::Warning),
        "overdue" => Ok(ScheduleStatus::Overdue),
        other => Err(AppError::BadRequest(format!(
            "status must be one of ok|warning|overdue (got '{other}')"
        ))),
    }
}

pub async fn aircraft_belongs_to_org(
    pool: &DbPool,
    aircraft_id: i32,
    org_id: i32,
) -> Result<bool, AppError> {
    let exists: Option<(i32,)> =
        sqlx::query_as("SELECT id FROM aircraft WHERE id = $1 AND org_id = $2")
            .bind(aircraft_id)
            .bind(org_id)
            .fetch_optional(pool)
            .await?;
    Ok(exists.is_some())
}

/// Validates that an instruction template may be linked to a schedule for
/// the given aircraft: it must exist in this org, be active, and either be
/// generic (aircraft_model_id IS NULL) or match the aircraft's model.
pub async fn validate_instruction_template(
    pool: &DbPool,
    org_id: i32,
    template_id: i32,
    aircraft_id: i32,
) -> Result<(), AppError> {
    let tmpl: Option<(Option<i32>, bool)> = sqlx::query_as(
        "SELECT aircraft_model_id, is_active \
         FROM maintenance_instruction_templates \
         WHERE id = $1 AND org_id = $2",
    )
    .bind(template_id)
    .bind(org_id)
    .fetch_optional(pool)
    .await?;

    let Some((tmpl_model_id, is_active)) = tmpl else {
        return Err(AppError::BadRequest(format!(
            "instruction_template_id {template_id} not found in this organisation"
        )));
    };
    if !is_active {
        return Err(AppError::BadRequest("instruction template is inactive".into()));
    }

    let ac_model_id: Option<i32> = sqlx::query_scalar(
        "SELECT model_id FROM aircraft WHERE id = $1 AND org_id = $2",
    )
    .bind(aircraft_id)
    .bind(org_id)
    .fetch_optional(pool)
    .await?
    .flatten();

    if let Some(tmpl_m) = tmpl_model_id {
        if Some(tmpl_m) != ac_model_id {
            return Err(AppError::BadRequest(
                "instruction template belongs to a different aircraft model".into(),
            ));
        }
    }
    Ok(())
}

/// Compute primary-axis next_due values based on interval type and the last-performed baseline.
/// If the baseline for the relevant axis is absent, the next_due for that axis stays None.
pub fn compute_next_due(
    interval_type: &str,
    interval_value: f32,
    last_performed_at: Option<DateTime<Utc>>,
    last_performed_flight_hours: Option<f32>,
    last_performed_cycles: Option<i32>,
) -> (Option<DateTime<Utc>>, Option<f32>, Option<i32>) {
    match interval_type {
        "calendar_days" => match last_performed_at {
            Some(t) => (Some(t + Duration::days(interval_value as i64)), None, None),
            None => (None, None, None),
        },
        "flight_hours" => match last_performed_flight_hours {
            Some(h) => (None, Some(h + interval_value), None),
            None => (None, None, None),
        },
        "cycles" => match last_performed_cycles {
            Some(c) => (None, None, Some(c + interval_value as i32)),
            None => (None, None, None),
        },
        _ => (None, None, None),
    }
}

/// Secondary trigger only supports calendar_days and flight_hours (no dedicated cycles column).
pub fn compute_secondary_next_due(
    secondary_interval_type: Option<&str>,
    secondary_interval_value: Option<f32>,
    last_performed_at: Option<DateTime<Utc>>,
    last_performed_flight_hours: Option<f32>,
) -> (Option<DateTime<Utc>>, Option<f32>) {
    match (secondary_interval_type, secondary_interval_value) {
        (Some("calendar_days"), Some(v)) => match last_performed_at {
            Some(t) => (Some(t + Duration::days(v as i64)), None),
            None => (None, None),
        },
        (Some("flight_hours"), Some(v)) => match last_performed_flight_hours {
            Some(h) => (None, Some(h + v)),
            None => (None, None),
        },
        _ => (None, None),
    }
}

pub async fn get_aircraft_totals(
    pool: &DbPool,
    org_id: i32,
    aircraft_id: i32,
) -> Result<AircraftTotals, AppError> {
    let row: (Option<f64>, i64) = sqlx::query_as(
        "SELECT COALESCE(SUM(duration_seconds)::float8 / 3600.0, 0.0), COUNT(*)::bigint \
         FROM flights WHERE aircraft_id = $1 AND org_id = $2 AND deleted_at IS NULL",
    )
    .bind(aircraft_id)
    .bind(org_id)
    .fetch_one(pool)
    .await?;
    Ok(AircraftTotals {
        flight_hours: row.0.unwrap_or(0.0),
        flight_count: row.1,
    })
}

pub async fn get_fleet_totals(
    pool: &DbPool,
    org_id: i32,
) -> Result<Vec<AircraftTotalsRow>, AppError> {
    let rows = sqlx::query_as::<_, AircraftTotalsRow>(
        "SELECT a.id AS aircraft_id, \
                a.equipment_id, am.name AS model, a.model_id AS model_id, a.serial_number, \
                COALESCE(SUM(f.duration_seconds)::float8 / 3600.0, 0.0) AS flight_hours, \
                COUNT(f.id)::bigint AS flight_count \
         FROM aircraft a \
         LEFT JOIN aircraft_models am ON a.model_id = am.id \
         LEFT JOIN flights f ON f.aircraft_id = a.id AND f.deleted_at IS NULL \
         WHERE a.org_id = $1 \
         GROUP BY a.id, am.name",
    )
    .bind(org_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub fn axis_consumption(
    interval_type: &str,
    interval_value: f32,
    schedule: &MaintenanceSchedule,
    now: DateTime<Utc>,
    totals: AircraftTotals,
) -> Option<f64> {
    if interval_value <= 0.0 {
        return None;
    }
    match interval_type {
        "calendar_days" => {
            let baseline = schedule.last_performed_at.unwrap_or(schedule.created_at);
            let consumed_days = (now - baseline).num_seconds() as f64 / 86_400.0;
            Some(consumed_days / interval_value as f64)
        }
        "flight_hours" => {
            let consumed = totals.flight_hours - schedule.last_performed_flight_hours as f64;
            Some(consumed / interval_value as f64)
        }
        "cycles" => {
            let consumed = totals.flight_count as f64 - schedule.last_performed_cycles as f64;
            Some(consumed / interval_value as f64)
        }
        _ => None,
    }
}

pub fn derive_status(max_consumption: f64, warning_threshold: f32) -> ScheduleStatus {
    if max_consumption >= 1.0 {
        ScheduleStatus::Overdue
    } else if max_consumption >= warning_threshold as f64 {
        ScheduleStatus::Warning
    } else {
        ScheduleStatus::Ok
    }
}

/// Returns (primary, secondary) consumption ratios. Primary is `Some` unless the
/// interval_type is unrecognised. Secondary is `Some` only when the schedule
/// defines a secondary axis and a baseline is present.
pub fn consumption_pair(
    schedule: &MaintenanceSchedule,
    now: DateTime<Utc>,
    totals: AircraftTotals,
) -> (Option<f64>, Option<f64>) {
    let primary = axis_consumption(
        &schedule.interval_type,
        schedule.interval_value,
        schedule,
        now,
        totals,
    );
    let secondary = schedule
        .secondary_interval_type
        .as_deref()
        .and_then(|t| {
            schedule
                .secondary_interval_value
                .map(|v| axis_consumption(t, v, schedule, now, totals))
        })
        .flatten();
    (primary, secondary)
}

/// Numeric rank for sorting: Overdue (0) is most urgent.
pub fn status_rank(s: ScheduleStatus) -> u8 {
    match s {
        ScheduleStatus::Overdue => 0,
        ScheduleStatus::Warning => 1,
        ScheduleStatus::Ok => 2,
    }
}

pub fn build_with_status(
    schedule: MaintenanceSchedule,
    map: &HashMap<i32, &AircraftTotalsRow>,
    now: DateTime<Utc>,
) -> ScheduleWithStatus {
    let row = map.get(&schedule.aircraft_id);
    let totals = AircraftTotals {
        flight_hours: row.map(|r| r.flight_hours).unwrap_or(0.0),
        flight_count: row.map(|r| r.flight_count).unwrap_or(0),
    };
    let aircraft_equipment_id = row.and_then(|r| r.equipment_id.clone());
    let aircraft_model = row.and_then(|r| r.model.clone());
    let aircraft_model_id = row.and_then(|r| r.model_id);
    let aircraft_serial_number = row
        .map(|r| r.serial_number.clone())
        .unwrap_or_default();

    let (primary, secondary) = consumption_pair(&schedule, now, totals);
    let max_consumption = [primary, secondary]
        .into_iter()
        .flatten()
        .fold(0.0_f64, f64::max);
    let status = derive_status(max_consumption, schedule.warning_threshold);

    ScheduleWithStatus {
        schedule,
        aircraft_equipment_id,
        aircraft_model,
        aircraft_model_id,
        aircraft_serial_number,
        status,
        consumption_pct: primary.unwrap_or(0.0) * 100.0,
        secondary_consumption_pct: secondary.map(|v| v * 100.0),
    }
}
