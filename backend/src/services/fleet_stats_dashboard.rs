// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

use chrono::Utc;
use sqlx::Row;
use std::collections::HashMap;

use crate::db::DbPool;
use crate::errors::AppError;
use crate::models::fleet_stats_dashboard::{
    AircraftByModelResponse, AircraftByModelRow, BatteryCycleBucket, BatteryCycleDistribution,
    BatteryNearEolRow, FleetDashboard, FleetDashboardKpis, KpiCard, MaintenanceAlertRow,
    MostUsedAircraftRow,
};
use crate::models::maintenance_schedule::{MaintenanceSchedule, ScheduleStatus};
use crate::services::schedule_calc::{build_with_status, get_fleet_totals, AircraftTotalsRow};

const SPARKLINE_DAYS: usize = 30;
const MOST_USED_LIMIT: usize = 5;
const EOL_LIMIT: usize = 5;
const ALERT_LIMIT: usize = 5;
const TOP_AIRCRAFT_EXTENDED_LIMIT: i64 = 15;
const EOL_THRESHOLD_PCT: f64 = 0.80;

/// Build the full dashboard payload: 5 KPIs + 3 widget lists.
pub async fn get_fleet_dashboard(
    pool: &DbPool,
    org_id: i32,
) -> Result<FleetDashboard, AppError> {
    let kpis = build_kpis(pool, org_id).await?;
    let most_used_aircraft = top_aircraft_by_hours(pool, org_id, MOST_USED_LIMIT as i64).await?;
    let batteries_near_eol_top = top_batteries_near_eol(pool, org_id, EOL_LIMIT as i64).await?;
    let maintenance_alerts = top_maintenance_alerts(pool, org_id, ALERT_LIMIT).await?;

    Ok(FleetDashboard {
        kpis,
        most_used_aircraft,
        batteries_near_eol_top,
        maintenance_alerts,
    })
}

async fn build_kpis(pool: &DbPool, org_id: i32) -> Result<FleetDashboardKpis, AppError> {
    // Counters (single roundtrip).
    let counters = sqlx::query(
        "SELECT \
            (SELECT COUNT(*)::bigint FROM aircraft \
                WHERE org_id = $1 AND status = 'active') AS active_aircraft, \
            (SELECT COUNT(DISTINCT model_id)::bigint FROM aircraft \
                WHERE org_id = $1 AND status = 'active' AND model_id IS NOT NULL) AS distinct_models, \
            (SELECT COUNT(*)::bigint FROM defect_log \
                WHERE org_id = $1 AND status IN ('open','in_progress')) AS open_defects, \
            (SELECT COUNT(*)::bigint FROM batteries \
                WHERE org_id = $1 AND status = 'active' \
                  AND max_cycles > 0 \
                  AND (cycle_count::float / max_cycles) >= $2) AS batteries_near_eol",
    )
    .bind(org_id)
    .bind(EOL_THRESHOLD_PCT)
    .fetch_one(pool)
    .await?;

    let active_aircraft: i64 = counters.get("active_aircraft");
    let distinct_models: i64 = counters.get("distinct_models");
    let open_defects_count: i64 = counters.get("open_defects");
    let batteries_near_eol_count: i64 = counters.get("batteries_near_eol");

    // Flight hours / count for last 30 days.
    let flight_30d = sqlx::query(
        "SELECT \
            COALESCE(SUM(duration_seconds), 0)::float8 / 3600.0 AS hours, \
            COUNT(*)::bigint AS flights \
         FROM flights \
         WHERE org_id = $1 AND deleted_at IS NULL \
           AND flight_datetime >= NOW() - INTERVAL '30 days'",
    )
    .bind(org_id)
    .fetch_one(pool)
    .await?;
    let flight_hours_30d: f64 = flight_30d.get("hours");
    let flight_count_30d: i64 = flight_30d.get("flights");

    // Overdue maintenance count — compute via schedule_calc so the definition
    // matches /maintenance-schedules/dashboard exactly.
    let (overdue_count, _) = overdue_and_warning_schedule_counts(pool, org_id).await?;

    // Sparklines.
    let flight_hours_sparkline = flight_hours_per_day(pool, org_id, SPARKLINE_DAYS).await?;
    let open_defects_sparkline = open_defects_per_day(pool, org_id, SPARKLINE_DAYS).await?;
    // For the remaining KPIs we don't track daily state, so we emit a flat
    // 30-point series equal to the current value. The frontend can render the
    // sparkline area without misleading the reader (a flat line means "no
    // historical series available").
    // TODO: capture daily snapshots of aircraft/battery counts so these
    // sparklines reflect real history.
    let flat_active = vec![active_aircraft as f64; SPARKLINE_DAYS];
    let flat_overdue = vec![overdue_count as f64; SPARKLINE_DAYS];
    let flat_eol = vec![batteries_near_eol_count as f64; SPARKLINE_DAYS];

    let flights_label = if flight_count_30d == 1 { "flight" } else { "flights" };
    let models_label = if distinct_models == 1 { "model" } else { "models" };

    Ok(FleetDashboardKpis {
        active_aircraft: KpiCard {
            value: active_aircraft as f64,
            secondary: Some(format!("{} {}", distinct_models, models_label)),
            severity: "neutral".to_string(),
            sparkline: flat_active,
        },
        flight_hours_30d: KpiCard {
            value: round1(flight_hours_30d),
            secondary: Some(format!("{} {}", flight_count_30d, flights_label)),
            severity: "neutral".to_string(),
            sparkline: flight_hours_sparkline,
        },
        open_defects: KpiCard {
            value: open_defects_count as f64,
            secondary: None,
            severity: if open_defects_count > 0 { "red".to_string() } else { "neutral".to_string() },
            sparkline: open_defects_sparkline,
        },
        overdue_maintenance: KpiCard {
            value: overdue_count as f64,
            secondary: None,
            severity: if overdue_count > 0 { "red".to_string() } else { "neutral".to_string() },
            sparkline: flat_overdue,
        },
        batteries_near_eol: KpiCard {
            value: batteries_near_eol_count as f64,
            secondary: None,
            severity: if batteries_near_eol_count > 0 { "amber".to_string() } else { "neutral".to_string() },
            sparkline: flat_eol,
        },
    })
}

fn round1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

async fn flight_hours_per_day(
    pool: &DbPool,
    org_id: i32,
    days: usize,
) -> Result<Vec<f64>, AppError> {
    let rows = sqlx::query(
        "WITH days AS ( \
            SELECT generate_series( \
                date_trunc('day', NOW() - ($2::int - 1) * INTERVAL '1 day'), \
                date_trunc('day', NOW()), \
                INTERVAL '1 day' \
            ) AS d \
         ) \
         SELECT days.d::date AS day, \
                COALESCE(SUM(f.duration_seconds), 0)::float8 / 3600.0 AS hours \
         FROM days \
         LEFT JOIN flights f \
            ON f.org_id = $1 \
           AND f.deleted_at IS NULL \
           AND date_trunc('day', f.flight_datetime) = days.d \
         GROUP BY days.d \
         ORDER BY days.d ASC",
    )
    .bind(org_id)
    .bind(days as i32)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| {
            let h: f64 = r.get("hours");
            round1(h)
        })
        .collect())
}

async fn open_defects_per_day(
    pool: &DbPool,
    org_id: i32,
    days: usize,
) -> Result<Vec<f64>, AppError> {
    let rows = sqlx::query(
        "WITH days AS ( \
            SELECT generate_series( \
                date_trunc('day', NOW() - ($2::int - 1) * INTERVAL '1 day'), \
                date_trunc('day', NOW()), \
                INTERVAL '1 day' \
            ) AS d \
         ) \
         SELECT days.d::date AS day, \
                COUNT(d.id)::bigint AS open_count \
         FROM days \
         LEFT JOIN defect_log d \
            ON d.org_id = $1 \
           AND d.defect_date <= (days.d + INTERVAL '1 day')::date \
           AND (d.closed_date IS NULL OR d.closed_date > (days.d + INTERVAL '1 day')::date) \
         GROUP BY days.d \
         ORDER BY days.d ASC",
    )
    .bind(org_id)
    .bind(days as i32)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| {
            let c: i64 = r.get("open_count");
            c as f64
        })
        .collect())
}

async fn overdue_and_warning_schedule_counts(
    pool: &DbPool,
    org_id: i32,
) -> Result<(usize, usize), AppError> {
    let schedules = sqlx::query_as::<_, MaintenanceSchedule>(
        "SELECT * FROM maintenance_schedules WHERE org_id = $1 AND is_active = TRUE",
    )
    .bind(org_id)
    .fetch_all(pool)
    .await?;

    let totals = get_fleet_totals(pool, org_id).await?;
    let map: HashMap<i32, &AircraftTotalsRow> =
        totals.iter().map(|r| (r.aircraft_id, r)).collect();
    let now = Utc::now();

    let items: Vec<_> = schedules
        .into_iter()
        .map(|s| build_with_status(s, &map, now))
        .collect();

    let overdue = items.iter().filter(|i| i.status == ScheduleStatus::Overdue).count();
    let warning = items.iter().filter(|i| i.status == ScheduleStatus::Warning).count();
    Ok((overdue, warning))
}

pub async fn top_aircraft_by_hours(
    pool: &DbPool,
    org_id: i32,
    limit: i64,
) -> Result<Vec<MostUsedAircraftRow>, AppError> {
    sqlx::query_as::<_, MostUsedAircraftRow>(
        "SELECT \
            a.id AS aircraft_id, \
            a.equipment_id, \
            a.serial_number, \
            am.name AS model_name, \
            COALESCE(SUM(f.duration_seconds), 0)::float8 / 3600.0 AS total_hours, \
            COUNT(f.id)::bigint AS total_flights, \
            MAX(f.flight_datetime) AS last_flight \
         FROM aircraft a \
         LEFT JOIN aircraft_models am ON am.id = a.model_id \
         LEFT JOIN flights f \
            ON f.aircraft_id = a.id AND f.deleted_at IS NULL \
         WHERE a.org_id = $1 AND a.status = 'active' \
         GROUP BY a.id, am.name \
         ORDER BY total_hours DESC NULLS LAST, total_flights DESC \
         LIMIT $2",
    )
    .bind(org_id)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(AppError::from)
}

async fn top_batteries_near_eol(
    pool: &DbPool,
    org_id: i32,
    limit: i64,
) -> Result<Vec<BatteryNearEolRow>, AppError> {
    sqlx::query_as::<_, BatteryNearEolRow>(
        "SELECT \
            b.id AS battery_id, \
            b.equipment_id, \
            b.serial_number, \
            b.compatible_model AS model, \
            b.cycle_count, \
            b.max_cycles, \
            (b.cycle_count::float / NULLIF(b.max_cycles,0))::float8 AS cycle_pct, \
            b.last_used \
         FROM batteries b \
         WHERE b.org_id = $1 \
           AND b.status = 'active' \
           AND b.max_cycles > 0 \
         ORDER BY cycle_pct DESC NULLS LAST \
         LIMIT $2",
    )
    .bind(org_id)
    .bind(limit)
    .fetch_all(pool)
    .await
    .map_err(AppError::from)
}

async fn top_maintenance_alerts(
    pool: &DbPool,
    org_id: i32,
    limit: usize,
) -> Result<Vec<MaintenanceAlertRow>, AppError> {
    let schedules = sqlx::query_as::<_, MaintenanceSchedule>(
        "SELECT * FROM maintenance_schedules WHERE org_id = $1 AND is_active = TRUE",
    )
    .bind(org_id)
    .fetch_all(pool)
    .await?;

    let totals = get_fleet_totals(pool, org_id).await?;
    let map: HashMap<i32, &AircraftTotalsRow> =
        totals.iter().map(|r| (r.aircraft_id, r)).collect();
    let now = Utc::now();

    let mut items: Vec<_> = schedules
        .into_iter()
        .map(|s| build_with_status(s, &map, now))
        .filter(|s| s.status != ScheduleStatus::Ok)
        .collect();

    // Sort: Overdue first, then Warning. Within Overdue, most-overdue first
    // (smallest/most negative next_due_at), within Warning earliest due first.
    items.sort_by(|a, b| {
        use std::cmp::Ordering;
        let rank = |s: ScheduleStatus| match s {
            ScheduleStatus::Overdue => 0,
            ScheduleStatus::Warning => 1,
            ScheduleStatus::Ok => 2,
        };
        rank(a.status).cmp(&rank(b.status)).then_with(|| {
            match (a.schedule.next_due_at, b.schedule.next_due_at) {
                (Some(ad), Some(bd)) => ad.cmp(&bd),
                (Some(_), None) => Ordering::Less,
                (None, Some(_)) => Ordering::Greater,
                (None, None) => b
                    .consumption_pct
                    .partial_cmp(&a.consumption_pct)
                    .unwrap_or(Ordering::Equal),
            }
        })
    });

    items.truncate(limit);

    Ok(items
        .into_iter()
        .map(|it| {
            let days_overdue = it.schedule.next_due_at.map(|due| {
                now.signed_duration_since(due).num_days()
            });
            let status_str = match it.status {
                ScheduleStatus::Overdue => "overdue",
                ScheduleStatus::Warning => "warning",
                ScheduleStatus::Ok => "ok",
            }
            .to_string();
            MaintenanceAlertRow {
                schedule_id: it.schedule.id,
                aircraft_id: it.schedule.aircraft_id,
                aircraft_equipment_id: it.aircraft_equipment_id,
                aircraft_serial_number: it.aircraft_serial_number,
                schedule_name: it.schedule.name,
                status: status_str,
                days_overdue,
                consumption_pct: it.consumption_pct,
                is_critical: it.schedule.is_critical,
            }
        })
        .collect())
}

// =========================================================================
// Aircraft tab
// =========================================================================

pub async fn get_aircraft_by_model(
    pool: &DbPool,
    org_id: i32,
) -> Result<AircraftByModelResponse, AppError> {
    let by_model = sqlx::query_as::<_, AircraftByModelRow>(
        "SELECT \
            am.id AS model_id, \
            COALESCE(am.name, '(no model)') AS model_name, \
            COUNT(DISTINCT a.id)::bigint AS aircraft_count, \
            COALESCE(SUM(f.duration_seconds), 0)::float8 / 3600.0 AS total_hours, \
            COUNT(f.id)::bigint AS total_flights, \
            ( \
                SELECT COUNT(*)::bigint \
                FROM defect_log d \
                JOIN aircraft a2 ON a2.id = d.aircraft_id \
                WHERE a2.org_id = $1 \
                  AND d.status IN ('open','in_progress') \
                  AND a2.model_id IS NOT DISTINCT FROM am.id \
            ) AS open_defects_count \
         FROM aircraft a \
         LEFT JOIN aircraft_models am ON am.id = a.model_id \
         LEFT JOIN flights f \
            ON f.aircraft_id = a.id AND f.deleted_at IS NULL \
         WHERE a.org_id = $1 AND a.status = 'active' \
         GROUP BY am.id, am.name \
         ORDER BY aircraft_count DESC, total_hours DESC",
    )
    .bind(org_id)
    .fetch_all(pool)
    .await?;

    let top_aircraft_by_hours =
        top_aircraft_by_hours(pool, org_id, TOP_AIRCRAFT_EXTENDED_LIMIT).await?;

    Ok(AircraftByModelResponse {
        by_model,
        top_aircraft_by_hours,
    })
}

// =========================================================================
// Battery cycle distribution
// =========================================================================

pub async fn get_battery_cycle_distribution(
    pool: &DbPool,
    org_id: i32,
) -> Result<BatteryCycleDistribution, AppError> {
    // Buckets: 0-25%, 25-50%, 50-75%, 75-90%, 90%+ (5 buckets).
    let rows = sqlx::query(
        "SELECT \
            CASE \
                WHEN ratio < 0.25 THEN 0 \
                WHEN ratio < 0.50 THEN 1 \
                WHEN ratio < 0.75 THEN 2 \
                WHEN ratio < 0.90 THEN 3 \
                ELSE 4 \
            END AS bucket, \
            COUNT(*)::bigint AS c \
         FROM ( \
            SELECT (cycle_count::float / NULLIF(max_cycles, 0)) AS ratio \
            FROM batteries \
            WHERE org_id = $1 AND status = 'active' AND max_cycles > 0 \
         ) sub \
         WHERE ratio IS NOT NULL \
         GROUP BY 1",
    )
    .bind(org_id)
    .fetch_all(pool)
    .await?;

    let mut counts = [0_i64; 5];
    for r in &rows {
        let b: i32 = r.get("bucket");
        let c: i64 = r.get("c");
        if (0..5).contains(&b) {
            counts[b as usize] = c;
        }
    }

    let bucket_defs: [(i32, i32, &str); 5] = [
        (0, 25, "0-25%"),
        (25, 50, "25-50%"),
        (50, 75, "50-75%"),
        (75, 90, "75-90%"),
        (90, 100, "90%+"),
    ];

    let buckets: Vec<BatteryCycleBucket> = bucket_defs
        .iter()
        .enumerate()
        .map(|(i, (lo, hi, label))| BatteryCycleBucket {
            label: label.to_string(),
            lower_pct: *lo,
            upper_pct: *hi,
            count: counts[i],
        })
        .collect();

    let total_active: i64 = counts.iter().sum();

    Ok(BatteryCycleDistribution {
        buckets,
        total_active,
    })
}
