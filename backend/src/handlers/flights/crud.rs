// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

use std::collections::HashMap;

use axum::{
    extract::{Path, Query, State},
    Json,
};
use chrono::{NaiveDate, NaiveTime, TimeZone, Utc};
use serde::{Deserialize, Serialize};

use crate::auth::middleware::{require_admin_or_technician, require_role, Claims};
use crate::db::DbPool;
use crate::errors::AppError;
use crate::models::flight::{CreateFlightRequest, Flight};
use crate::services::audit;

use super::detail::{AircraftRef, BatteryRef};

/// Maximum gap (minutes) between one flight's landing and the next flight's
/// takeoff for both to belong to the same *sortie* — a presentation-only
/// grouping of consecutive same-aircraft flights (e.g. a battery-swap
/// power-cycle sequence). The individual flight remains the unit of truth for
/// the EASA logbook; sorties are computed per-request at read time and are
/// never persisted.
///
/// Hardcoded for this release. A later PR makes this per-org via the existing
/// `settings` table — change it here and thread the setting through
/// `list_flights` when that lands. This const is the single source of truth.
const SORTIE_GAP_MINUTES: i64 = 15;

#[derive(Debug, Deserialize, Default)]
pub struct FlightQueryParams {
    pub date_from:     Option<String>,
    pub date_to:       Option<String>,
    pub aircraft_id:   Option<i32>,
    pub pilot_id:      Option<i32>,
    pub mission_type:  Option<String>,
    pub easa_category: Option<String>,
    pub search:        Option<String>,
    pub page:          Option<i64>,
    pub per_page:      Option<i64>,
}

/// One row in the flight-list response. Wraps a `Flight` with its
/// resolved aircraft (model + equipment id) and battery (equipment id +
/// SN), so the frontend can render two-line "UAV-XXX / Model · SN" and
/// "BAT-XXX / model · SN" cells without a second round-trip.
///
/// `#[serde(flatten)]` keeps the wire shape backwards-compatible: every
/// existing flight field stays at the top level. The new keys are
/// `aircraft` and `battery`, each `null` when the flight has no linked
/// aircraft / battery.
#[derive(Debug, Serialize)]
pub struct FlightListItem {
    #[serde(flatten)]
    pub flight: Flight,
    pub aircraft: Option<AircraftRef>,
    pub battery: Option<BatteryRef>,
    /// Sortie grouping metadata, computed server-side over the ENTIRE
    /// filtered+ordered set before pagination (see `list_flights`). View-level
    /// only — never persisted.
    ///
    /// `sortie_id` is stable *within a single response* (equal for flights in
    /// the same sortie, distinct across sorties); it is not a database id and
    /// must not be stored or referenced across requests. `sortie_size` and
    /// `sortie_total_seconds` are computed over the full set, so a group that
    /// straddles a page boundary still reports correct totals on either page.
    pub sortie_id: i64,
    pub sortie_size: i64,
    pub sortie_total_seconds: i64,
}

/// Internal row shape for the sortie-aware list query: a full `Flight`
/// flattened alongside the three window-computed sortie columns. Extra helper
/// columns in the SQL (`_grp`, `_s`, `_e`, `_new`) are unmapped and ignored by
/// `FromRow`.
#[derive(Debug, sqlx::FromRow)]
struct FlightSortieRow {
    #[sqlx(flatten)]
    flight: Flight,
    sortie_id: i64,
    sortie_size: i64,
    sortie_total_seconds: i64,
}

#[derive(Debug, Serialize)]
pub struct FlightListResponse {
    pub flights: Vec<FlightListItem>,
    pub total: i64,
    pub page: i64,
    pub per_page: i64,
    pub total_pages: i64,
}

pub async fn list_flights(
    claims: Claims,
    State(pool): State<DbPool>,
    Query(params): Query<FlightQueryParams>,
) -> Result<Json<FlightListResponse>, AppError> {
    let page = params.page.unwrap_or(1).max(1);
    let per_page = params.per_page.unwrap_or(50).min(200).max(1);
    let offset = (page - 1) * per_page;

    // Build dynamic WHERE clause
    let mut conditions = vec!["org_id = $1".to_string(), "deleted_at IS NULL".to_string()];
    let mut bind_idx: usize = 2;

    // We'll collect typed bind values for use later
    struct Binds {
        date_from: Option<chrono::DateTime<Utc>>,
        date_to: Option<chrono::DateTime<Utc>>,
        aircraft_id: Option<i32>,
        pilot_id: Option<i32>,
        mission_type: Option<String>,
        easa_category: Option<String>,
        search: Option<String>,
    }

    let mut binds = Binds {
        date_from: None,
        date_to: None,
        aircraft_id: None,
        pilot_id: None,
        mission_type: None,
        easa_category: None,
        search: None,
    };

    if let Some(ref df) = params.date_from {
        let d = NaiveDate::parse_from_str(df, "%Y-%m-%d")
            .map_err(|_| AppError::BadRequest(format!("Invalid date_from: {}", df)))?;
        binds.date_from = Some(Utc.from_utc_datetime(&d.and_hms_opt(0, 0, 0).unwrap_or_default()));
        conditions.push(format!("flight_datetime >= ${}", bind_idx));
        bind_idx += 1;
    }

    if let Some(ref dt) = params.date_to {
        let d = NaiveDate::parse_from_str(dt, "%Y-%m-%d")
            .map_err(|_| AppError::BadRequest(format!("Invalid date_to: {}", dt)))?;
        binds.date_to = Some(Utc.from_utc_datetime(&d.and_time(NaiveTime::from_hms_opt(23, 59, 59).unwrap_or_default())));
        conditions.push(format!("flight_datetime <= ${}", bind_idx));
        bind_idx += 1;
    }

    if let Some(aid) = params.aircraft_id {
        binds.aircraft_id = Some(aid);
        conditions.push(format!("aircraft_id = ${}", bind_idx));
        bind_idx += 1;
    }

    if let Some(pid) = params.pilot_id {
        binds.pilot_id = Some(pid);
        conditions.push(format!("pilot_id = ${}", bind_idx));
        bind_idx += 1;
    }

    if let Some(ref mt) = params.mission_type {
        binds.mission_type = Some(mt.clone());
        conditions.push(format!("mission_type = ${}", bind_idx));
        bind_idx += 1;
    }

    if let Some(ref ec) = params.easa_category {
        binds.easa_category = Some(ec.clone());
        conditions.push(format!("easa_category = ${}", bind_idx));
        bind_idx += 1;
    }

    if let Some(ref s) = params.search {
        let like_val = format!("%{}%", s);
        binds.search = Some(like_val);
        let idx = format!("${}", bind_idx);
        conditions.push(format!("(operation_name ILIKE {} OR remarks ILIKE {} OR aircraft_name ILIKE {})", idx, idx, idx));
        bind_idx += 1;
    }

    let where_clause = conditions.join(" AND ");

    // Count query
    let count_sql = format!("SELECT COUNT(*) as count FROM flights WHERE {}", where_clause);
    let mut count_query = sqlx::query_scalar::<_, i64>(&count_sql)
        .bind(claims.org_id);
    if let Some(ref v) = binds.date_from { count_query = count_query.bind(v); }
    if let Some(ref v) = binds.date_to { count_query = count_query.bind(v); }
    if let Some(v) = binds.aircraft_id { count_query = count_query.bind(v); }
    if let Some(v) = binds.pilot_id { count_query = count_query.bind(v); }
    if let Some(ref v) = binds.mission_type { count_query = count_query.bind(v); }
    if let Some(ref v) = binds.easa_category { count_query = count_query.bind(v); }
    if let Some(ref v) = binds.search { count_query = count_query.bind(v); }
    let total = count_query.fetch_one(&pool).await?;

    // Data query — sortie-aware.
    //
    // Grouping is computed over the ENTIRE filtered set BEFORE limit/offset so
    // a sortie that straddles a page boundary still reports the correct
    // count + summed time on both pages. Steps (CTE chain):
    //   filtered — apply the same WHERE as the count query; derive the sortie
    //              start/end with the manual-flight fallback
    //              (COALESCE(flight_start, flight_datetime) and
    //               COALESCE(flight_end, flight_datetime + duration)), and a
    //              partition key `_grp` = aircraft_id, but `-id` for
    //              aircraft-less flights so each lands in its own partition
    //              (never grouped with another null-aircraft flight).
    //   ordered  — within each aircraft, ordered chronologically by the
    //              coalesced start, flag a new sortie at the first row of the
    //              partition or when the gap from the previous landing reaches
    //              the threshold.
    //   numbered — running sum of the new-sortie flags → a stable sortie_id.
    //   grouped  — per-sortie COUNT and SUM(duration) over the full set.
    // The outer SELECT restores the existing flight_datetime DESC ordering and
    // applies limit/offset, so list order and pagination semantics are
    // unchanged.
    let data_sql = format!(
        "WITH filtered AS ( \
             SELECT *, \
                    COALESCE(aircraft_id, -id) AS _grp, \
                    COALESCE(flight_start, flight_datetime) AS _s, \
                    COALESCE(flight_end, flight_datetime + make_interval(secs => duration_seconds)) AS _e \
             FROM flights WHERE {where_clause} \
         ), \
         ordered AS ( \
             SELECT *, \
                    CASE \
                        WHEN LAG(_e) OVER w IS NULL THEN 1 \
                        WHEN _s - LAG(_e) OVER w >= INTERVAL '{gap} minutes' THEN 1 \
                        ELSE 0 \
                    END AS _new \
             FROM filtered \
             WINDOW w AS (PARTITION BY _grp ORDER BY _s ASC NULLS FIRST, id ASC) \
         ), \
         numbered AS ( \
             SELECT *, \
                    SUM(_new) OVER (ORDER BY _grp ASC, _s ASC NULLS FIRST, id ASC) AS sortie_id \
             FROM ordered \
         ), \
         grouped AS ( \
             SELECT *, \
                    COUNT(*) OVER (PARTITION BY sortie_id) AS sortie_size, \
                    COALESCE(SUM(duration_seconds) OVER (PARTITION BY sortie_id), 0) AS sortie_total_seconds \
             FROM numbered \
         ) \
         SELECT * FROM grouped \
         ORDER BY flight_datetime DESC NULLS LAST \
         LIMIT ${limit} OFFSET ${offset}",
        where_clause = where_clause,
        gap = SORTIE_GAP_MINUTES,
        limit = bind_idx,
        offset = bind_idx + 1,
    );
    let mut data_query = sqlx::query_as::<_, FlightSortieRow>(&data_sql)
        .bind(claims.org_id);
    if let Some(ref v) = binds.date_from { data_query = data_query.bind(v); }
    if let Some(ref v) = binds.date_to { data_query = data_query.bind(v); }
    if let Some(v) = binds.aircraft_id { data_query = data_query.bind(v); }
    if let Some(v) = binds.pilot_id { data_query = data_query.bind(v); }
    if let Some(ref v) = binds.mission_type { data_query = data_query.bind(v); }
    if let Some(ref v) = binds.easa_category { data_query = data_query.bind(v); }
    if let Some(ref v) = binds.search { data_query = data_query.bind(v); }
    data_query = data_query.bind(per_page).bind(offset);
    let flights: Vec<FlightSortieRow> = data_query.fetch_all(&pool).await?;

    // Resolve the joined aircraft + model_name for every flight on this page
    // in a single follow-up query. Cheaper than re-shaping the main SQL with
    // a JOIN (Flight is `SELECT *` everywhere — keeping that intact avoids
    // touching the PDF report code and other Flight consumers).
    let aircraft_ids: Vec<i32> = {
        let mut ids: Vec<i32> = flights.iter().filter_map(|f| f.flight.aircraft_id).collect();
        ids.sort_unstable();
        ids.dedup();
        ids
    };
    let aircraft_by_id: HashMap<i32, AircraftRef> = if aircraft_ids.is_empty() {
        HashMap::new()
    } else {
        #[derive(sqlx::FromRow)]
        struct Row {
            id: i32,
            equipment_id: Option<String>,
            serial_number: String,
            nickname: Option<String>,
            model: Option<String>,
            status: String,
        }
        let rows = sqlx::query_as::<_, Row>(
            "SELECT a.id, a.equipment_id, a.serial_number, a.nickname, \
                    am.name AS model, a.status \
             FROM aircraft a \
             LEFT JOIN aircraft_models am ON am.id = a.model_id \
             WHERE a.org_id = $1 AND a.id = ANY($2)",
        )
        .bind(claims.org_id)
        .bind(&aircraft_ids)
        .fetch_all(&pool)
        .await?;
        rows.into_iter()
            .map(|r| {
                (
                    r.id,
                    AircraftRef {
                        id: r.id,
                        equipment_id: r.equipment_id,
                        serial_number: r.serial_number,
                        nickname: r.nickname,
                        model: r.model,
                        status: r.status,
                    },
                )
            })
            .collect()
    };

    // Resolve the joined battery for every flight on this page in a single
    // follow-up query — mirrors aircraft_by_id above. One extra query for the
    // whole page, no JOIN on the main SELECT * (keeps Flight consumers intact).
    let battery_ids: Vec<i32> = {
        let mut ids: Vec<i32> = flights.iter().filter_map(|f| f.flight.battery_id).collect();
        ids.sort_unstable();
        ids.dedup();
        ids
    };
    let battery_by_id: HashMap<i32, BatteryRef> = if battery_ids.is_empty() {
        HashMap::new()
    } else {
        #[derive(sqlx::FromRow)]
        struct Row {
            id: i32,
            equipment_id: Option<String>,
            serial_number: String,
            compatible_model: Option<String>,
            status: String,
        }
        let rows = sqlx::query_as::<_, Row>(
            "SELECT id, equipment_id, serial_number, compatible_model, status \
             FROM batteries WHERE org_id = $1 AND id = ANY($2)",
        )
        .bind(claims.org_id)
        .bind(&battery_ids)
        .fetch_all(&pool)
        .await?;
        rows.into_iter()
            .map(|r| {
                (
                    r.id,
                    BatteryRef {
                        id: r.id,
                        equipment_id: r.equipment_id,
                        serial_number: r.serial_number,
                        compatible_model: r.compatible_model,
                        status: r.status,
                    },
                )
            })
            .collect()
    };

    // flights.pic_name / pm_name are denormalised columns that are never
    // populated on write (import/create/update set pilot_id only). Resolve
    // the names on read here — same pattern as aircraft_by_id above, and
    // drift-free if a pilot is later renamed.
    let pilot_ids: Vec<i32> = {
        let mut ids: Vec<i32> = flights
            .iter()
            .flat_map(|f| [f.flight.pilot_id, f.flight.pilot_monitoring_id])
            .flatten()
            .collect();
        ids.sort_unstable();
        ids.dedup();
        ids
    };
    let pilot_name_by_id: HashMap<i32, String> = if pilot_ids.is_empty() {
        HashMap::new()
    } else {
        #[derive(sqlx::FromRow)]
        struct PilotNameRow {
            id: i32,
            name: String,
        }
        let rows = sqlx::query_as::<_, PilotNameRow>(
            "SELECT id, name FROM pilots WHERE org_id = $1 AND id = ANY($2)",
        )
        .bind(claims.org_id)
        .bind(&pilot_ids)
        .fetch_all(&pool)
        .await?;
        rows.into_iter().map(|r| (r.id, r.name)).collect()
    };

    let items: Vec<FlightListItem> = flights
        .into_iter()
        .map(|row| {
            let FlightSortieRow { mut flight, sortie_id, sortie_size, sortie_total_seconds } = row;
            let aircraft = flight.aircraft_id.and_then(|aid| aircraft_by_id.get(&aid).cloned());
            let battery = flight.battery_id.and_then(|bid| battery_by_id.get(&bid).cloned());
            flight.pic_name = flight.pilot_id.and_then(|pid| pilot_name_by_id.get(&pid).cloned());
            flight.pm_name = flight
                .pilot_monitoring_id
                .and_then(|pid| pilot_name_by_id.get(&pid).cloned());
            FlightListItem {
                flight,
                aircraft,
                battery,
                sortie_id,
                sortie_size,
                sortie_total_seconds,
            }
        })
        .collect();

    let total_pages = if total == 0 { 1 } else { (total + per_page - 1) / per_page };

    Ok(Json(FlightListResponse {
        flights: items,
        total,
        page,
        per_page,
        total_pages,
    }))
}

/// GET /api/v1/flights/:id — Get a single flight by ID.
pub async fn get_flight(
    claims: Claims,
    State(pool): State<DbPool>,
    Path(id): Path<i32>,
) -> Result<Json<Flight>, AppError> {
    let row = sqlx::query_as::<_, Flight>(
        "SELECT * FROM flights WHERE id = $1 AND org_id = $2 AND deleted_at IS NULL"
    )
    .bind(id)
    .bind(claims.org_id)
    .fetch_optional(&pool)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Flight {} not found", id)))?;

    Ok(Json(row))
}

/// POST /api/v1/flights — Create a manual flight entry.
pub async fn create_flight(
    claims: Claims,
    State(pool): State<DbPool>,
    Json(body): Json<CreateFlightRequest>,
) -> Result<Json<Flight>, AppError> {
    require_role(
        &claims,
        &["admin", "safety_officer", "technician", "pilot"],
    )?;

    // Wrap INSERT + denormalisation + counter bump in one transaction so
    // a counter-update failure rolls back the flight insert. For a brand-
    // new manual entry that is the right trade — there is nothing to lose
    // if the counter step fails, and committing the flight without the
    // counter bump is the exact drift bug we are fixing.
    let mut tx = pool.begin().await?;

    let row = sqlx::query_as::<_, Flight>(
        "INSERT INTO flights (org_id, pilot_id, aircraft_id, battery_id, flight_datetime, duration_seconds, max_altitude_m, max_distance_m, easa_category, mission_type, operation_name, remarks, preflight_done, log_source) VALUES ($1,$2,$3,$4,$5::timestamptz,$6,COALESCE($7,0),COALESCE($8,0),$9,$10,$11,$12,COALESCE($13,false),'manual') RETURNING *"
    )
    .bind(claims.org_id)
    .bind(&body.pilot_id)
    .bind(&body.aircraft_id)
    .bind(&body.battery_id)
    .bind(&body.flight_datetime)
    .bind(&body.duration_seconds)
    .bind(&body.max_altitude_m)
    .bind(&body.max_distance_m)
    .bind(&body.easa_category)
    .bind(&body.mission_type)
    .bind(&body.operation_name)
    .bind(&body.remarks)
    .bind(&body.preflight_done)
    .fetch_one(&mut *tx)
    .await
    .map_err(|e| {
        tracing::error!("create_flight DB error: {:?}", e);
        AppError::Database(e)
    })?;

    // Denormalize aircraft name for display
    if let Some(aircraft_id) = body.aircraft_id {
        sqlx::query(
            "UPDATE flights SET aircraft_name = \
                (SELECT COALESCE(a.nickname, am.name, a.serial_number) \
                 FROM aircraft a LEFT JOIN aircraft_models am ON a.model_id = am.id \
                 WHERE a.id = $1 AND a.org_id = $2) WHERE id = $3"
        )
        .bind(aircraft_id)
        .bind(claims.org_id)
        .bind(row.id)
        .execute(&mut *tx)
        .await?;

        // Bump the denormalised counters on the linked aircraft. Inside
        // the tx, propagate failure with `?` — see comment above.
        sqlx::query(
            "UPDATE aircraft SET
               total_flights = total_flights + 1,
               total_seconds = total_seconds + $1,
               last_flight   = GREATEST(COALESCE(last_flight, 'epoch'::timestamptz), $2)
             WHERE id = $3 AND org_id = $4"
        )
        .bind(row.duration_seconds)
        .bind(row.flight_datetime)
        .bind(aircraft_id)
        .bind(claims.org_id)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;

    audit::log_insert_full(&pool, claims.org_id, claims.user_id, &claims.user_code, "flights", row.id, &row).await;

    Ok(Json(row))
}

/// PUT /api/v1/flights/:id — Update flight metadata.
pub async fn update_flight(
    claims: Claims,
    State(pool): State<DbPool>,
    Path(id): Path<i32>,
    Json(body): Json<CreateFlightRequest>,
) -> Result<Json<Flight>, AppError> {
    require_admin_or_technician(&claims)?;
    let mut tx = pool.begin().await?;
    let before = sqlx::query_as::<_, Flight>(
        "SELECT * FROM flights WHERE id = $1 AND org_id = $2 AND deleted_at IS NULL"
    )
    .bind(id)
    .bind(claims.org_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Flight {} not found", id)))?;

    let after = sqlx::query_as::<_, Flight>(
        "UPDATE flights SET easa_category = COALESCE($1, easa_category), mission_type = COALESCE($2, mission_type), remarks = COALESCE($3, remarks), preflight_done = COALESCE($4, preflight_done), pilot_id = COALESCE($5, pilot_id) WHERE id = $6 AND org_id = $7 AND deleted_at IS NULL RETURNING *"
    )
    .bind(&body.easa_category)
    .bind(&body.mission_type)
    .bind(&body.remarks)
    .bind(&body.preflight_done)
    .bind(&body.pilot_id)
    .bind(id)
    .bind(claims.org_id)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;

    audit::log_update_diff(&pool, claims.org_id, claims.user_id, &claims.user_code, "flights", after.id, &before, &after).await;

    Ok(Json(after))
}

/// DELETE /api/v1/flights/:id — Soft delete.
pub async fn delete_flight(
    claims: Claims,
    State(pool): State<DbPool>,
    Path(id): Path<i32>,
) -> Result<Json<serde_json::Value>, AppError> {
    require_admin_or_technician(&claims)?;
    let mut tx = pool.begin().await?;
    let before = sqlx::query_as::<_, Flight>(
        "SELECT * FROM flights WHERE id = $1 AND org_id = $2 AND deleted_at IS NULL"
    )
    .bind(id)
    .bind(claims.org_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Flight {} not found", id)))?;

    sqlx::query(
        "UPDATE flights SET deleted_at = NOW() WHERE id = $1 AND org_id = $2 AND deleted_at IS NULL"
    )
    .bind(id)
    .bind(claims.org_id)
    .execute(&mut *tx)
    .await?;

    // Mirror the INCREMENT in create_flight: decrement the denormalised
    // counters and recompute last_flight from the remaining rows. The
    // soft-delete above already excluded this row, so the subquery
    // naturally falls to NULL when the aircraft has no remaining flights.
    // GREATEST(... , 0) guards against pre-existing data drift.
    if let Some(aircraft_id) = before.aircraft_id {
        sqlx::query(
            "UPDATE aircraft SET
               total_flights = GREATEST(total_flights - 1, 0),
               total_seconds = GREATEST(total_seconds - $1, 0),
               last_flight   = (SELECT MAX(flight_datetime) FROM flights
                                WHERE aircraft_id = $2 AND deleted_at IS NULL)
             WHERE id = $2 AND org_id = $3"
        )
        .bind(before.duration_seconds)
        .bind(aircraft_id)
        .bind(claims.org_id)
        .execute(&mut *tx)
        .await?;
    }

    tx.commit().await?;

    // Soft delete semantically but flagged as DELETE for audit continuity.
    audit::log_delete_full(&pool, claims.org_id, claims.user_id, &claims.user_code, "flights", id, &before).await;

    Ok(Json(serde_json::json!({ "message": "Flight deleted", "id": id })))
}

/// GET /api/v1/flights/filters — Distinct values to populate filter dropdowns.
/// Aircraft and pilot dropdowns reuse `GET /api/v1/aircraft` and
/// `GET /api/v1/pilots`; this endpoint only returns the two text-column
/// distinct lists (mission_type and easa_category).
pub async fn filter_options(
    claims: Claims,
    State(pool): State<DbPool>,
) -> Result<Json<serde_json::Value>, AppError> {
    use sqlx::Row;

    let mt_rows = sqlx::query(
        "SELECT DISTINCT mission_type FROM flights \
         WHERE org_id = $1 AND deleted_at IS NULL AND mission_type IS NOT NULL \
         ORDER BY mission_type",
    )
    .bind(claims.org_id)
    .fetch_all(&pool)
    .await?;
    let mission_types: Vec<String> = mt_rows
        .iter()
        .map(|r| r.get::<String, _>("mission_type"))
        .collect();

    let ec_rows = sqlx::query(
        "SELECT DISTINCT easa_category FROM flights \
         WHERE org_id = $1 AND deleted_at IS NULL AND easa_category IS NOT NULL \
         ORDER BY easa_category",
    )
    .bind(claims.org_id)
    .fetch_all(&pool)
    .await?;
    let easa_categories: Vec<String> = ec_rows
        .iter()
        .map(|r| r.get::<String, _>("easa_category"))
        .collect();

    Ok(Json(serde_json::json!({
        "mission_types": mission_types,
        "easa_categories": easa_categories,
    })))
}

/// GET /api/v1/flights/stats — Aggregated flight statistics for the org.
pub async fn get_stats(
    claims: Claims,
    State(pool): State<DbPool>,
) -> Result<Json<serde_json::Value>, AppError> {
    let row = sqlx::query(
        "SELECT
            COUNT(*)::bigint as total_flights,
            COALESCE(SUM(duration_seconds), 0)::bigint as total_seconds,
            COALESCE(SUM(duration_seconds), 0)::float8 / 3600.0 as total_hours,
            COALESCE(AVG(duration_seconds), 0)::float8 as avg_duration_seconds,
            COALESCE(MAX(max_altitude_m), 0)::float8 as max_altitude_m,
            COALESCE(SUM(total_distance_m), 0)::float8 as total_distance_m,
            MAX(flight_datetime) as last_flight
         FROM flights
         WHERE org_id = $1 AND deleted_at IS NULL",
    )
    .bind(claims.org_id)
    .fetch_one(&pool)
    .await
    .map_err(|e| {
        tracing::error!("get_stats error: {:?}", e);
        AppError::Database(e)
    })?;

    use sqlx::Row;
    let stats = serde_json::json!({
        "total_flights": row.get::<i64, _>("total_flights"),
        "total_seconds": row.get::<i64, _>("total_seconds"),
        "total_hours": row.get::<f64, _>("total_hours"),
        "avg_duration_seconds": row.get::<f64, _>("avg_duration_seconds"),
        "max_altitude_m": row.get::<f64, _>("max_altitude_m"),
        "total_distance_km": row.get::<f64, _>("total_distance_m") / 1000.0,
        "last_flight": row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("last_flight"),
    });

    Ok(Json(stats))
}
