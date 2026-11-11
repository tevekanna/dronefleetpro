// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

//! Shared aggregate queries for the aircraft-detail endpoint.
//!
//! Mirrors `pilot_aggregates.rs` but scoped on `(aircraft_id, org_id)`.
//! Soft-deleted flights are excluded so totals match what the user sees
//! in the flight log.

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
/// visual regime, day/night). Same shape as `pilot_aggregates::CategoryBucket`
/// — kept separate so the two domains can evolve independently.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct CategoryBucket {
    pub label: String,
    pub flights: i64,
    pub total_seconds: i64,
}

/// Single-pass aggregate over the total range plus an arbitrary set of
/// `INTERVAL '<n> days'` windows. Returns `(total, windows)` where
/// `windows` matches the order of `windows_days` 1-to-1. Same shape as
/// `pilot_aggregates::flight_window_aggregates` — kept separate so the
/// two endpoints can evolve independently.
pub async fn flight_window_aggregates(
    pool: &DbPool,
    aircraft_id: i32,
    org_id: i32,
    windows_days: &[i32],
) -> Result<(WindowAggregate, Vec<WindowAggregate>), AppError> {
    let mut select_parts: Vec<String> = vec![
        "COUNT(*)::bigint AS count_total".into(),
        "COALESCE(SUM(duration_seconds), 0)::bigint AS secs_total".into(),
    ];
    for (i, _) in windows_days.iter().enumerate() {
        let bind_idx = i + 3; // $1=aircraft, $2=org, $3.. are window day-counts
        select_parts.push(format!(
            "COUNT(*) FILTER (WHERE flight_start >= NOW() - (${bind_idx}::int || ' days')::interval)::bigint AS count_w{i}"
        ));
        select_parts.push(format!(
            "COALESCE(SUM(duration_seconds) FILTER (WHERE flight_start >= NOW() - (${bind_idx}::int || ' days')::interval), 0)::bigint AS secs_w{i}"
        ));
    }
    let sql = format!(
        "SELECT {} FROM flights WHERE aircraft_id = $1 AND org_id = $2 AND deleted_at IS NULL",
        select_parts.join(", ")
    );

    let mut q = sqlx::query(&sql).bind(aircraft_id).bind(org_id);
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

/// Total distance flown by this aircraft, in metres. Stored per-flight
/// in `flights.total_distance_m` (REAL, populated by the import pipeline
/// via haversine integration of the trajectory).
pub async fn total_distance_m(
    pool: &DbPool,
    aircraft_id: i32,
    org_id: i32,
) -> Result<f64, AppError> {
    let v: f64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(total_distance_m), 0)::float8 \
         FROM flights \
         WHERE aircraft_id = $1 AND org_id = $2 AND deleted_at IS NULL",
    )
    .bind(aircraft_id)
    .bind(org_id)
    .fetch_one(pool)
    .await?;
    Ok(v)
}

// =====================================================================
// Categorical breakdowns — ported from pilot_aggregates with the
// `pilot_id` filter swapped for `aircraft_id`. NULL category rows are
// omitted from EASA / mission so percentages sum to 100% across what's
// actually present. Day/night is handled separately in the handler
// because sun-position classification runs in Rust, not SQL.
// =====================================================================

/// EASA category buckets for this aircraft. NULL categories omitted.
pub async fn easa_category_breakdown(
    pool: &DbPool,
    aircraft_id: i32,
    org_id: i32,
) -> Result<Vec<CategoryBucket>, AppError> {
    let rows = sqlx::query_as::<_, CategoryBucket>(
        "SELECT easa_category AS label, COUNT(*)::bigint AS flights, \
                COALESCE(SUM(duration_seconds), 0)::bigint AS total_seconds \
         FROM flights \
         WHERE aircraft_id = $1 AND org_id = $2 AND deleted_at IS NULL \
           AND easa_category IS NOT NULL \
         GROUP BY easa_category \
         ORDER BY flights DESC",
    )
    .bind(aircraft_id)
    .bind(org_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Mission type buckets for this aircraft. NULL types omitted.
pub async fn mission_type_breakdown(
    pool: &DbPool,
    aircraft_id: i32,
    org_id: i32,
) -> Result<Vec<CategoryBucket>, AppError> {
    let rows = sqlx::query_as::<_, CategoryBucket>(
        "SELECT mission_type AS label, COUNT(*)::bigint AS flights, \
                COALESCE(SUM(duration_seconds), 0)::bigint AS total_seconds \
         FROM flights \
         WHERE aircraft_id = $1 AND org_id = $2 AND deleted_at IS NULL \
           AND mission_type IS NOT NULL \
         GROUP BY mission_type \
         ORDER BY flights DESC",
    )
    .bind(aircraft_id)
    .bind(org_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// VLOS / BVLOS / Unknown derived from `easa_category` (same mapping as
/// pilot_aggregates::visual_regime_breakdown: Open* → VLOS, Specific* →
/// BVLOS, NULL → Unknown). This treats every Specific-category operation
/// as BVLOS — refine if a fleet flies Specific VLOS.
pub async fn visual_regime_breakdown(
    pool: &DbPool,
    aircraft_id: i32,
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
         WHERE aircraft_id = $1 AND org_id = $2 AND deleted_at IS NULL \
         GROUP BY label \
         ORDER BY flights DESC",
    )
    .bind(aircraft_id)
    .bind(org_id)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}
