// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

//! Three-year retention guard for the admin purge routes.
//!
//! Computes, per purge target, how many records fall inside the EASA
//! retention window (and would be destroyed) or would lose identity data
//! (and be degraded), and enforces an explicit operator confirmation
//! before any retention-protected record is purged.

use serde::{Deserialize, Serialize};
use sqlx::{PgConnection, Row};

use crate::db::DbPool;
use crate::errors::AppError;
use crate::services::audit;

/// Minimum record-keeping period in years, per EASA UAS.SPEC.050(1)(g):
/// records of the qualifications and training of staff, of maintenance
/// activities, and of UAS operations — including any unusual technical or
/// operational occurrences — must be kept for a minimum of three years.
///
/// Note: AMC1 UAS.SPEC.050(1)(g)(c) states a two-year period, but the
/// Implementing Rule figure of three years takes precedence over the AMC.
pub const RETENTION_YEARS: i32 = 3;

/// The six valid purge targets, as they appear in the purge route paths.
pub const PURGE_TARGETS: &[&str] = &[
    "flights",
    "aircraft",
    "batteries",
    "controllers",
    "pilots",
    "incidents",
];

/// One affected table within a purge impact assessment.
#[derive(Debug, Serialize)]
pub struct RetentionDetail {
    pub table: String,
    pub column: String,
    pub count: i64,
    /// "deleted" (row is destroyed) or "identity_stripped" (row survives
    /// but loses an identity FK the regulation requires it to carry).
    /// May carry an explanatory suffix where the base date is unknowable
    /// (pilot qualifications — see `impact_pilots`).
    pub effect: String,
}

/// Impact assessment for purging one target, returned by the preflight
/// endpoint and used to gate the DELETE handlers.
#[derive(Debug, Serialize)]
pub struct RetentionImpact {
    pub target: String,
    pub total_rows: i64,
    pub within_retention: i64,
    pub degraded_records: i64,
    pub retention_years: i32,
    pub details: Vec<RetentionDetail>,
}

/// Operator confirmation body for a purge that touches protected records.
/// `acknowledged` must equal the current `within_retention +
/// degraded_records` total; `reason` must be at least 10 characters after
/// trimming.
#[derive(Debug, Deserialize)]
pub struct PurgeConfirmation {
    pub acknowledged: i64,
    pub reason: String,
}

/// Retention-window predicate for flights. `created_at` is the final
/// fallback so a flight with neither `flight_start` nor `flight_datetime`
/// set cannot silently escape the guard.
const FLIGHT_WITHIN: &str =
    "COALESCE(flight_start, flight_datetime, created_at) >= NOW() - make_interval(years => $2)";

async fn count_org(conn: &mut PgConnection, sql: &str, org_id: i32) -> Result<i64, AppError> {
    let row = sqlx::query(sql).bind(org_id).fetch_one(&mut *conn).await?;
    Ok(row.get::<i64, _>(0))
}

async fn count_retained(conn: &mut PgConnection, sql: &str, org_id: i32) -> Result<i64, AppError> {
    let row = sqlx::query(sql)
        .bind(org_id)
        .bind(RETENTION_YEARS)
        .fetch_one(&mut *conn)
        .await?;
    Ok(row.get::<i64, _>(0))
}

/// Compute the retention impact for a purge target. Read-only. Returns
/// `BadRequest` for an unknown target.
pub async fn impact_for_target(
    conn: &mut PgConnection,
    org_id: i32,
    target: &str,
) -> Result<RetentionImpact, AppError> {
    match target {
        "flights" => impact_flights(conn, org_id).await,
        "aircraft" => impact_aircraft(conn, org_id).await,
        "batteries" => impact_batteries(conn, org_id).await,
        "controllers" => impact_controllers(conn, org_id).await,
        "pilots" => impact_pilots(conn, org_id).await,
        "incidents" => impact_incidents(conn, org_id).await,
        other => Err(AppError::BadRequest(format!(
            "Unknown purge target '{}'. Valid targets: {}",
            other,
            PURGE_TARGETS.join(", ")
        ))),
    }
}

async fn impact_flights(conn: &mut PgConnection, org_id: i32) -> Result<RetentionImpact, AppError> {
    let total = count_org(conn, "SELECT COUNT(*) FROM flights WHERE org_id = $1", org_id).await?;
    // No deleted_at filter: a soft-deleted flight is still a retained record.
    let within = count_retained(
        conn,
        &format!("SELECT COUNT(*) FROM flights WHERE org_id = $1 AND {FLIGHT_WITHIN}"),
        org_id,
    )
    .await?;
    Ok(RetentionImpact {
        target: "flights".into(),
        total_rows: total,
        within_retention: within,
        degraded_records: 0,
        retention_years: RETENTION_YEARS,
        details: vec![RetentionDetail {
            table: "flights".into(),
            column: "COALESCE(flight_start, flight_datetime, created_at)".into(),
            count: within,
            effect: "deleted".into(),
        }],
    })
}

async fn impact_incidents(
    conn: &mut PgConnection,
    org_id: i32,
) -> Result<RetentionImpact, AppError> {
    let total = count_org(conn, "SELECT COUNT(*) FROM incidents WHERE org_id = $1", org_id).await?;
    let within = count_retained(
        conn,
        "SELECT COUNT(*) FROM incidents WHERE org_id = $1 \
         AND occurred_at >= NOW() - make_interval(years => $2)",
        org_id,
    )
    .await?;
    Ok(RetentionImpact {
        target: "incidents".into(),
        total_rows: total,
        within_retention: within,
        degraded_records: 0,
        retention_years: RETENTION_YEARS,
        details: vec![RetentionDetail {
            table: "incidents".into(),
            column: "occurred_at".into(),
            count: within,
            effect: "deleted".into(),
        }],
    })
}

/// Aircraft rows are not themselves retained records, but purging them
/// strips the UA identity (aircraft_id) from retained flights, maintenance
/// records, defect entries and incidents — identity AMC1
/// UAS.SPEC.050(1)(g)(b) requires those records to carry.
async fn impact_aircraft(
    conn: &mut PgConnection,
    org_id: i32,
) -> Result<RetentionImpact, AppError> {
    let total = count_org(conn, "SELECT COUNT(*) FROM aircraft WHERE org_id = $1", org_id).await?;
    let mut details = Vec::new();
    let mut degraded = 0i64;

    let checks: [(&str, &str, String); 4] = [
        (
            "flights",
            "aircraft_id",
            format!(
                "SELECT COUNT(*) FROM flights WHERE org_id = $1 \
                 AND aircraft_id IS NOT NULL AND {FLIGHT_WITHIN}"
            ),
        ),
        (
            "maintenance",
            "aircraft_id",
            "SELECT COUNT(*) FROM maintenance WHERE org_id = $1 \
             AND aircraft_id IS NOT NULL \
             AND performed_date >= (NOW() - make_interval(years => $2))::date"
                .into(),
        ),
        (
            "defect_log",
            "aircraft_id",
            "SELECT COUNT(*) FROM defect_log WHERE org_id = $1 \
             AND aircraft_id IS NOT NULL \
             AND defect_date >= (NOW() - make_interval(years => $2))::date"
                .into(),
        ),
        (
            "incidents",
            "aircraft_id",
            "SELECT COUNT(*) FROM incidents WHERE org_id = $1 \
             AND aircraft_id IS NOT NULL \
             AND occurred_at >= NOW() - make_interval(years => $2)"
                .into(),
        ),
    ];
    for (table, column, sql) in checks {
        let n = count_retained(conn, &sql, org_id).await?;
        degraded += n;
        details.push(RetentionDetail {
            table: table.into(),
            column: column.into(),
            count: n,
            effect: "identity_stripped".into(),
        });
    }

    Ok(RetentionImpact {
        target: "aircraft".into(),
        total_rows: total,
        within_retention: 0,
        degraded_records: degraded,
        retention_years: RETENTION_YEARS,
        details,
    })
}

async fn impact_pilots(conn: &mut PgConnection, org_id: i32) -> Result<RetentionImpact, AppError> {
    let total = count_org(conn, "SELECT COUNT(*) FROM pilots WHERE org_id = $1", org_id).await?;

    // UAS.SPEC.050(1)(g) measures the three years from the date the person
    // left the organisation or changed position. The application records no
    // such date, so every qualification row must be treated as within
    // retention.
    let quals = count_org(
        conn,
        "SELECT COUNT(*) FROM pilot_qualifications WHERE org_id = $1",
        org_id,
    )
    .await?;

    let mut details = vec![RetentionDetail {
        table: "pilot_qualifications".into(),
        column: "(none)".into(),
        count: quals,
        effect: "deleted (no leaving/position-change date is recorded, so every \
                 qualification row is treated as within retention)"
            .into(),
    }];
    let mut degraded = 0i64;

    let flights = count_retained(
        conn,
        &format!(
            "SELECT COUNT(*) FROM flights WHERE org_id = $1 \
             AND (pilot_id IS NOT NULL OR pilot_monitoring_id IS NOT NULL) AND {FLIGHT_WITHIN}"
        ),
        org_id,
    )
    .await?;
    degraded += flights;
    details.push(RetentionDetail {
        table: "flights".into(),
        column: "pilot_id / pilot_monitoring_id".into(),
        count: flights,
        effect: "identity_stripped".into(),
    });

    let incidents = count_retained(
        conn,
        "SELECT COUNT(*) FROM incidents WHERE org_id = $1 \
         AND pilot_id IS NOT NULL \
         AND occurred_at >= NOW() - make_interval(years => $2)",
        org_id,
    )
    .await?;
    degraded += incidents;
    details.push(RetentionDetail {
        table: "incidents".into(),
        column: "pilot_id".into(),
        count: incidents,
        effect: "identity_stripped".into(),
    });

    Ok(RetentionImpact {
        target: "pilots".into(),
        total_rows: total,
        within_retention: quals,
        degraded_records: degraded,
        retention_years: RETENTION_YEARS,
        details,
    })
}

async fn impact_batteries(
    conn: &mut PgConnection,
    org_id: i32,
) -> Result<RetentionImpact, AppError> {
    let total = count_org(conn, "SELECT COUNT(*) FROM batteries WHERE org_id = $1", org_id).await?;
    let flights = count_retained(
        conn,
        &format!(
            "SELECT COUNT(*) FROM flights WHERE org_id = $1 \
             AND battery_id IS NOT NULL AND {FLIGHT_WITHIN}"
        ),
        org_id,
    )
    .await?;
    Ok(RetentionImpact {
        target: "batteries".into(),
        total_rows: total,
        within_retention: 0,
        degraded_records: flights,
        retention_years: RETENTION_YEARS,
        details: vec![RetentionDetail {
            table: "flights".into(),
            column: "battery_id".into(),
            count: flights,
            effect: "identity_stripped".into(),
        }],
    })
}

async fn impact_controllers(
    conn: &mut PgConnection,
    org_id: i32,
) -> Result<RetentionImpact, AppError> {
    let total = count_org(
        conn,
        "SELECT COUNT(*) FROM remote_controllers WHERE org_id = $1",
        org_id,
    )
    .await?;
    let flights = count_retained(
        conn,
        &format!(
            "SELECT COUNT(*) FROM flights WHERE org_id = $1 \
             AND controller_id IS NOT NULL AND {FLIGHT_WITHIN}"
        ),
        org_id,
    )
    .await?;
    Ok(RetentionImpact {
        target: "controllers".into(),
        total_rows: total,
        within_retention: 0,
        degraded_records: flights,
        retention_years: RETENTION_YEARS,
        details: vec![RetentionDetail {
            table: "flights".into(),
            column: "controller_id".into(),
            count: flights,
            effect: "identity_stripped".into(),
        }],
    })
}

/// Enforce the retention guard for a purge. Computes the impact using the
/// caller's transaction connection and either returns it (purge may
/// proceed) or an error (the caller's `?` drops the transaction, rolling
/// back):
///
/// - no protected records → proceed, confirmation not required;
/// - protected records and no body → 409 pointing at the preflight route;
/// - `acknowledged` mismatch → 409 with the current protected count;
/// - `reason` under 10 characters after trimming → 400.
pub async fn enforce_retention_guard(
    conn: &mut PgConnection,
    org_id: i32,
    target: &str,
    confirmation: Option<&PurgeConfirmation>,
) -> Result<RetentionImpact, AppError> {
    let impact = impact_for_target(conn, org_id, target).await?;
    let protected = impact.within_retention + impact.degraded_records;
    if protected == 0 {
        return Ok(impact);
    }
    match confirmation {
        None => Err(AppError::Conflict(format!(
            "Purge of {target} is blocked by the {RETENTION_YEARS}-year retention period \
             (EASA UAS.SPEC.050(1)(g)): {} record(s) within retention would be deleted and \
             {} record(s) would lose identity data ({protected} protected in total). Review \
             GET /api/v1/admin/purge/{target}/preflight, then resubmit with a body \
             {{ \"acknowledged\": {protected}, \"reason\": \"...\" }}.",
            impact.within_retention, impact.degraded_records,
        ))),
        Some(c) if c.acknowledged != protected => Err(AppError::Conflict(format!(
            "Retention acknowledgement mismatch: body acknowledged {} record(s) but \
             {protected} record(s) are currently protected. Re-check \
             GET /api/v1/admin/purge/{target}/preflight and resubmit.",
            c.acknowledged,
        ))),
        Some(c) if c.reason.trim().len() < 10 => Err(AppError::BadRequest(
            "A reason of at least 10 characters is required to purge \
             retention-protected records."
                .into(),
        )),
        Some(_) => Ok(impact),
    }
}

fn target_table(target: &str) -> &'static str {
    match target {
        "flights" => "flights",
        "aircraft" => "aircraft",
        "batteries" => "batteries",
        "controllers" => "remote_controllers",
        "pilots" => "pilots",
        "incidents" => "incidents",
        _ => "unknown",
    }
}

/// Record one summary audit entry for a confirmed purge of protected
/// records. No-op when nothing was protected (the purge then behaved
/// exactly as before the guard existed). Best-effort, like all audit
/// writes — called only after the purge transaction has committed.
pub async fn log_purge_audit(
    pool: &DbPool,
    org_id: i32,
    user_id: i32,
    user_code: &str,
    impact: &RetentionImpact,
    rows_deleted: i64,
    confirmation: Option<&PurgeConfirmation>,
) {
    if impact.within_retention + impact.degraded_records == 0 {
        return;
    }
    let details = serde_json::json!({
        "purge": {
            "target": impact.target,
            "rows_deleted": rows_deleted,
            "within_retention": impact.within_retention,
            "degraded_records": impact.degraded_records,
            "retention_years": impact.retention_years,
            "reason": confirmation.map(|c| c.reason.trim().to_string()),
        }
    });
    let _ = audit::log_custom_action(
        pool,
        org_id,
        user_id,
        user_code,
        "PURGE",
        target_table(&impact.target),
        0,
        details,
    )
    .await;
}
