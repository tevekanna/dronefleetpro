// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

//! Shared aggregate queries for pilot-stats and pilot-detail endpoints.
//!
//! Both `handlers::pilot_stats` (GET /pilots/:id/stats) and
//! `handlers::pilot_detail` (GET /pilots/:id/detail) are read-only views
//! over the same underlying flight history. To keep them from drifting
//! (e.g. one counts soft-deleted flights and the other doesn't), every
//! aggregate they need lives here.
//!
//! Common conventions:
//! - All queries are scoped to `(pilot_id, org_id)`.
//! - Soft-deleted flights (`deleted_at IS NOT NULL`) are always excluded.
//! - Flight-time uses `flight_start` (the event time). `created_at` would
//!   overstate recency windows because flights are typically imported
//!   days or weeks after they happened.
//! - NULL category / mission rows are omitted from those breakdowns —
//!   percentages then sum to 100% across what's actually present.

use chrono::NaiveDate;
use sqlx::Row;

use crate::db::DbPool;
use crate::errors::AppError;

/// Flight count + total seconds for a single time window.
#[derive(Debug, Clone, Copy)]
pub struct WindowAggregate {
    pub count: i64,
    pub seconds: i64,
}

/// One bucket in a categorical breakdown (EASA category, mission type,
/// visual regime, …). Carries both count and seconds so each caller can
/// project to its own response shape (pct vs duration).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct CategoryBucket {
    pub label: String,
    pub flights: i64,
    pub total_seconds: i64,
}

/// Aircraft-level breakdown row. Used by /stats (top-3) and /detail
/// (full list) — the difference is the LIMIT applied.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AircraftBucket {
    pub aircraft_id: i32,
    pub equipment_id: Option<String>,
    pub model: Option<String>,
    pub flights: i64,
    pub total_seconds: i64,
}

/// Model-level breakdown row.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ModelBucket {
    pub model_id: Option<i32>,
    pub label: String,
    pub flights: i64,
    pub total_seconds: i64,
}

/// Qualification entry with computed status. Carries every field a UI
/// might want; each endpoint projects to its public shape.
///
/// `id` is the `pilot_qualifications.id` for endorsement rows and `None`
/// for the synthesised primary-license row (drawn from
/// `pilots.license_type` / `pilots.license_expires`).
#[derive(Debug, Clone)]
pub struct QualificationRow {
    pub id: Option<i32>,
    pub type_name: String,
    pub issued_date: Option<NaiveDate>,
    pub expiry_date: Option<NaiveDate>,
    pub status: &'static str,
    pub days_until_expiry: Option<i32>,
    pub notes: Option<String>,
    pub certificate_ref: Option<String>,
    pub is_primary_license: bool,
}

/// "ok" | "expiring_soon" | "expired" | "no_expiry"
pub fn status_for_expiry(
    expiry: Option<NaiveDate>,
    today: NaiveDate,
) -> (&'static str, Option<i32>) {
    match expiry {
        None => ("no_expiry", None),
        Some(d) => {
            let days = (d - today).num_days() as i32;
            if days < 0 {
                ("expired", Some(days))
            } else if days <= 30 {
                ("expiring_soon", Some(days))
            } else {
                ("ok", Some(days))
            }
        }
    }
}

/// Single-pass aggregate of flight count + total seconds across the
/// total range plus an arbitrary set of `INTERVAL '<n> days'` windows.
///
/// Returns `(total, windows)` where `windows` matches the order of
/// `windows_days` 1-to-1.
///
/// Both endpoints use this:
/// - `/stats` calls with `[30, 365]`.
/// - `/detail` calls with `[90, 365]` (EASA recency is 90 days).
pub async fn flight_window_aggregates(
    pool: &DbPool,
    pilot_id: i32,
    org_id: i32,
    windows_days: &[i32],
) -> Result<(WindowAggregate, Vec<WindowAggregate>), AppError> {
    // Build the FILTER clauses dynamically. We bind each window as $3, $4, …
    // so SQL stays parameterised — no string interpolation of user input.
    let mut select_parts: Vec<String> = vec![
        "COUNT(*)::bigint AS count_total".into(),
        "COALESCE(SUM(duration_seconds), 0)::bigint AS secs_total".into(),
    ];
    for (i, _) in windows_days.iter().enumerate() {
        let bind_idx = i + 3; // $1=pilot, $2=org, $3.. are window day-counts
        select_parts.push(format!(
            "COUNT(*) FILTER (WHERE flight_start >= NOW() - (${bind_idx}::int || ' days')::interval)::bigint AS count_w{i}"
        ));
        select_parts.push(format!(
            "COALESCE(SUM(duration_seconds) FILTER (WHERE flight_start >= NOW() - (${bind_idx}::int || ' days')::interval), 0)::bigint AS secs_w{i}"
        ));
    }
    let sql = format!(
        "SELECT {} FROM flights WHERE pilot_id = $1 AND org_id = $2 AND deleted_at IS NULL",
        select_parts.join(", ")
    );

    let mut q = sqlx::query(&sql).bind(pilot_id).bind(org_id);
    for d in windows_days {
        q = q.bind(*d);
    }
    let row = q.fetch_one(pool).await?;

    let total = WindowAggregate {
        count: row.get::<i64, _>("count_total"),
        seconds: row.get::<i64, _>("secs_total"),
    };
    let mut windows = Vec::with_capacity(windows_days.len());
    for i in 0..windows_days.len() {
        windows.push(WindowAggregate {
            count: row.get::<i64, _>(format!("count_w{i}").as_str()),
            seconds: row.get::<i64, _>(format!("secs_w{i}").as_str()),
        });
    }
    Ok((total, windows))
}

/// EASA category buckets. NULLs omitted — percentages from this set sum
/// to 100% across what's present.
pub async fn easa_category_breakdown(
    pool: &DbPool,
    pilot_id: i32,
    org_id: i32,
) -> Result<Vec<CategoryBucket>, AppError> {
    let rows = sqlx::query_as::<_, CategoryBucket>(
        "SELECT easa_category AS label, COUNT(*)::bigint AS flights, \
                COALESCE(SUM(duration_seconds), 0)::bigint AS total_seconds \
         FROM flights \
         WHERE pilot_id = $1 AND org_id = $2 AND deleted_at IS NULL \
           AND easa_category IS NOT NULL \
         GROUP BY easa_category \
         ORDER BY flights DESC",
    )
    .bind(pilot_id)
    .bind(org_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Mission type buckets. NULLs omitted, same as `easa_category_breakdown`.
pub async fn mission_type_breakdown(
    pool: &DbPool,
    pilot_id: i32,
    org_id: i32,
) -> Result<Vec<CategoryBucket>, AppError> {
    let rows = sqlx::query_as::<_, CategoryBucket>(
        "SELECT mission_type AS label, COUNT(*)::bigint AS flights, \
                COALESCE(SUM(duration_seconds), 0)::bigint AS total_seconds \
         FROM flights \
         WHERE pilot_id = $1 AND org_id = $2 AND deleted_at IS NULL \
           AND mission_type IS NOT NULL \
         GROUP BY mission_type \
         ORDER BY flights DESC",
    )
    .bind(pilot_id)
    .bind(org_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// VLOS / BVLOS / Unknown breakdown derived from `easa_category`.
///
/// Mapping: `Open*` → VLOS, `Specific*` → BVLOS, NULL → Unknown.
/// Note: SAIL classification (I/II) is risk-based, not strictly
/// BVLOS — this mapping simply treats Specific as BVLOS.
/// Refine the mapping if a fleet flies Specific VLOS.
pub async fn visual_regime_breakdown(
    pool: &DbPool,
    pilot_id: i32,
    org_id: i32,
) -> Result<Vec<CategoryBucket>, AppError> {
    let rows = sqlx::query_as::<_, CategoryBucket>(
        "SELECT \
           CASE \
             WHEN easa_category LIKE 'Open%' THEN 'VLOS' \
             WHEN easa_category LIKE 'Specific%' THEN 'BVLOS' \
             ELSE 'Unknown' \
           END AS label, \
           COUNT(*)::bigint AS flights, \
           COALESCE(SUM(duration_seconds), 0)::bigint AS total_seconds \
         FROM flights \
         WHERE pilot_id = $1 AND org_id = $2 AND deleted_at IS NULL \
         GROUP BY label \
         ORDER BY flights DESC",
    )
    .bind(pilot_id)
    .bind(org_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Top aircraft by flight-seconds. Pass `Some(n)` for stats-style top-N
/// or `None` for the full list (detail page).
pub async fn aircraft_breakdown(
    pool: &DbPool,
    pilot_id: i32,
    org_id: i32,
    limit: Option<i64>,
) -> Result<Vec<AircraftBucket>, AppError> {
    let base = "SELECT a.id AS aircraft_id, a.equipment_id, am.name AS model, \
                COUNT(f.id)::bigint AS flights, \
                COALESCE(SUM(f.duration_seconds), 0)::bigint AS total_seconds \
         FROM flights f \
         JOIN aircraft a ON a.id = f.aircraft_id \
         LEFT JOIN aircraft_models am ON a.model_id = am.id \
         WHERE f.pilot_id = $1 AND f.org_id = $2 AND f.deleted_at IS NULL \
         GROUP BY a.id, am.name \
         ORDER BY total_seconds DESC, flights DESC";
    let rows = if let Some(n) = limit {
        sqlx::query_as::<_, AircraftBucket>(&format!("{base} LIMIT $3"))
            .bind(pilot_id)
            .bind(org_id)
            .bind(n)
            .fetch_all(pool)
            .await?
    } else {
        sqlx::query_as::<_, AircraftBucket>(base)
            .bind(pilot_id)
            .bind(org_id)
            .fetch_all(pool)
            .await?
    };
    Ok(rows)
}

/// Per-model breakdown. Same `limit` semantics as `aircraft_breakdown`.
/// Aircraft missing a model show under label "Unknown" (model_id = NULL).
pub async fn model_breakdown(
    pool: &DbPool,
    pilot_id: i32,
    org_id: i32,
    limit: Option<i64>,
) -> Result<Vec<ModelBucket>, AppError> {
    let base = "SELECT am.id AS model_id, COALESCE(am.name, 'Unknown') AS label, \
                COUNT(f.id)::bigint AS flights, \
                COALESCE(SUM(f.duration_seconds), 0)::bigint AS total_seconds \
         FROM flights f \
         JOIN aircraft a ON a.id = f.aircraft_id \
         LEFT JOIN aircraft_models am ON a.model_id = am.id \
         WHERE f.pilot_id = $1 AND f.org_id = $2 AND f.deleted_at IS NULL \
         GROUP BY am.id, am.name \
         ORDER BY total_seconds DESC, flights DESC";
    let rows = if let Some(n) = limit {
        sqlx::query_as::<_, ModelBucket>(&format!("{base} LIMIT $3"))
            .bind(pilot_id)
            .bind(org_id)
            .bind(n)
            .fetch_all(pool)
            .await?
    } else {
        sqlx::query_as::<_, ModelBucket>(base)
            .bind(pilot_id)
            .bind(org_id)
            .fetch_all(pool)
            .await?
    };
    Ok(rows)
}

/// Primary license + endorsements with computed expiry status.
///
/// Output order: primary license first (if `pilots.license_type` is set),
/// then endorsements ordered by `expiry_date ASC NULLS LAST, type_name`.
///
/// `today` is parameterised so callers can override for tests; production
/// callers pass `chrono::Utc::now().date_naive()`.
pub async fn qualifications_with_status(
    pool: &DbPool,
    pilot_id: i32,
    org_id: i32,
    today: NaiveDate,
) -> Result<Vec<QualificationRow>, AppError> {
    #[derive(sqlx::FromRow)]
    struct PilotLicense {
        license_type: Option<String>,
        license_expires: Option<NaiveDate>,
    }
    let lic = sqlx::query_as::<_, PilotLicense>(
        "SELECT license_type, license_expires FROM pilots \
         WHERE id = $1 AND org_id = $2",
    )
    .bind(pilot_id)
    .bind(org_id)
    .fetch_optional(pool)
    .await?;

    let mut out: Vec<QualificationRow> = Vec::new();

    if let Some(lic) = lic.as_ref() {
        if let Some(t) = &lic.license_type {
            let (status, days) = status_for_expiry(lic.license_expires, today);
            out.push(QualificationRow {
                id: None,
                type_name: t.clone(),
                issued_date: None,
                expiry_date: lic.license_expires,
                status,
                days_until_expiry: days,
                notes: None,
                certificate_ref: None,
                is_primary_license: true,
            });
        }
    }

    #[derive(sqlx::FromRow)]
    struct EndorsementRow {
        id: i32,
        type_name: String,
        issued_date: Option<NaiveDate>,
        expiry_date: Option<NaiveDate>,
        notes: Option<String>,
        certificate_ref: Option<String>,
    }
    let rows = sqlx::query_as::<_, EndorsementRow>(
        "SELECT id, type_name, issued_date, expiry_date, notes, certificate_ref \
         FROM pilot_qualifications \
         WHERE pilot_id = $1 AND org_id = $2 \
         ORDER BY expiry_date ASC NULLS LAST, type_name",
    )
    .bind(pilot_id)
    .bind(org_id)
    .fetch_all(pool)
    .await?;
    for r in rows {
        let (status, days) = status_for_expiry(r.expiry_date, today);
        out.push(QualificationRow {
            id: Some(r.id),
            type_name: r.type_name,
            issued_date: r.issued_date,
            expiry_date: r.expiry_date,
            status,
            days_until_expiry: days,
            notes: r.notes,
            certificate_ref: r.certificate_ref,
            is_primary_license: false,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn status_for_expiry_buckets() {
        let today = NaiveDate::from_ymd_opt(2024, 6, 1).unwrap();
        assert_eq!(status_for_expiry(None, today), ("no_expiry", None));
        assert_eq!(
            status_for_expiry(NaiveDate::from_ymd_opt(2024, 5, 31), today).0,
            "expired"
        );
        assert_eq!(
            status_for_expiry(NaiveDate::from_ymd_opt(2024, 6, 15), today).0,
            "expiring_soon"
        );
        assert_eq!(
            status_for_expiry(NaiveDate::from_ymd_opt(2025, 1, 1), today).0,
            "ok"
        );
    }
}
