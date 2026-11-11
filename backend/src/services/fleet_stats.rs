// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

use crate::db::DbPool;
use crate::errors::AppError;
use crate::models::fleet_stats::{
    BatteryModelComparison, BatteryStats, BatteryStatsDetail, BatteryStatsSummary,
    EstimatedRange, HealthSample, HistogramBucket,
};

// Re-export controller + overview functions so existing
// `services::fleet_stats::fn_name` call sites keep working after the file split.
pub use crate::services::fleet_stats_controllers::{get_controller_stats, get_fleet_overview};

const HISTOGRAM_BINS: usize = 10;
const HEALTH_SERIES_LIMIT: i64 = 50;

// TODO: dynamic lifespan estimation using battery_tests regression — currently
// we rely on the static battery_model_specs table. Once battery_tests has
// meaningful data, swap in a regression-based estimator here.
fn clamp_nonneg(v: i32) -> i32 {
    if v < 0 { 0 } else { v }
}

fn estimated_ranges(
    cycle_count: i32,
    nominal_max_cycles: Option<i32>,
    uncertainty: Option<i32>,
) -> (Option<EstimatedRange>, Option<EstimatedRange>) {
    let Some(nominal) = nominal_max_cycles else {
        return (None, None);
    };
    let u = uncertainty.unwrap_or(0);
    let remaining_low = clamp_nonneg(nominal - u - cycle_count);
    let remaining_mid = clamp_nonneg(nominal - cycle_count);
    let remaining_high = clamp_nonneg(nominal + u - cycle_count);

    let max_range_denom = if nominal > 0 { nominal as f32 } else { 1.0 };
    let pct = EstimatedRange {
        low: ((remaining_low as f32 / max_range_denom) * 100.0).round() as i32,
        mid: ((remaining_mid as f32 / max_range_denom) * 100.0).round() as i32,
        high: ((remaining_high as f32 / max_range_denom) * 100.0).round() as i32,
    };
    let cycles = EstimatedRange { low: remaining_low, mid: remaining_mid, high: remaining_high };
    (Some(cycles), Some(pct))
}

// =========================================================================
// Battery per-id stats + detail
// =========================================================================

pub async fn get_battery_stats(
    pool: &DbPool,
    org_id: i32,
    battery_id: i32,
) -> Result<BatteryStats, AppError> {
    let mut stats = sqlx::query_as::<_, BatteryStats>(
        "SELECT \
            b.id AS battery_id, \
            b.serial_number, \
            b.equipment_id, \
            b.compatible_model AS model, \
            b.status, \
            b.cycle_count, \
            b.max_cycles, \
            s.nominal_max_cycles, \
            s.cycle_uncertainty, \
            COALESCE(agg.total_flights, 0) AS total_flights, \
            COALESCE(agg.total_flight_seconds, 0) AS total_flight_seconds, \
            agg.avg_discharge_per_flight, \
            b.last_used, \
            agg_90d.avg_cell_variance_v, \
            agg_90d.avg_min_temp_c, \
            agg_90d.avg_min_battery_pct \
         FROM batteries b \
         LEFT JOIN battery_model_specs s ON s.model = b.compatible_model \
         LEFT JOIN LATERAL ( \
            SELECT \
                COUNT(*)::bigint AS total_flights, \
                COALESCE(SUM(duration_seconds), 0)::bigint AS total_flight_seconds, \
                AVG(CASE WHEN min_battery_pct IS NOT NULL THEN 100 - min_battery_pct END)::real AS avg_discharge_per_flight \
            FROM flights f WHERE f.battery_id = b.id AND f.deleted_at IS NULL \
         ) agg ON TRUE \
         LEFT JOIN LATERAL ( \
            SELECT \
                AVG(cell_variance_v)::real AS avg_cell_variance_v, \
                AVG(battery_temp_c)::real  AS avg_min_temp_c, \
                AVG(min_battery_pct)::real AS avg_min_battery_pct \
            FROM flights f \
            WHERE f.battery_id = b.id AND f.deleted_at IS NULL \
              AND f.flight_datetime > NOW() - INTERVAL '90 days' \
         ) agg_90d ON TRUE \
         WHERE b.id = $1 AND b.org_id = $2",
    )
    .bind(battery_id)
    .bind(org_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| AppError::NotFound(format!("Battery {battery_id} not found")))?;

    let (cycles, pct) =
        estimated_ranges(stats.cycle_count, stats.nominal_max_cycles, stats.cycle_uncertainty);
    stats.estimated_remaining_cycles = cycles;
    stats.estimated_remaining_pct = pct;
    Ok(stats)
}

pub async fn get_battery_stats_detail(
    pool: &DbPool,
    org_id: i32,
    battery_id: i32,
) -> Result<BatteryStatsDetail, AppError> {
    let stats = get_battery_stats(pool, org_id, battery_id).await?;
    let series = sqlx::query_as::<_, HealthSample>(
        "SELECT id AS flight_id, flight_datetime, min_battery_pct, battery_temp_c, \
                cell_variance_v, duration_seconds \
         FROM flights \
         WHERE battery_id = $1 AND org_id = $2 AND deleted_at IS NULL \
         ORDER BY COALESCE(flight_datetime, created_at) DESC \
         LIMIT $3",
    )
    .bind(battery_id)
    .bind(org_id)
    .bind(HEALTH_SERIES_LIMIT)
    .fetch_all(pool)
    .await?;
    // Reverse so the UI gets oldest→newest for plotting.
    let mut series = series;
    series.reverse();
    Ok(BatteryStatsDetail { stats, health_timeseries: series })
}

// =========================================================================
// Battery model comparison
// =========================================================================

pub async fn get_model_comparison(
    pool: &DbPool,
    org_id: i32,
    model: &str,
) -> Result<BatteryModelComparison, AppError> {
    let spec: Option<(i32, i32, Option<f32>)> = sqlx::query_as(
        "SELECT nominal_max_cycles, cycle_uncertainty, nominal_capacity_wh \
         FROM battery_model_specs WHERE model = $1",
    )
    .bind(model)
    .fetch_optional(pool)
    .await?;
    let (nominal_max_cycles, cycle_uncertainty, nominal_capacity_wh) = match spec {
        Some((n, u, c)) => (Some(n), Some(u), c),
        None => (None, None, None),
    };

    let batteries = sqlx::query_as::<_, BatteryStatsSummary>(
        "SELECT \
            b.id AS battery_id, \
            b.serial_number, \
            b.equipment_id, \
            b.compatible_model AS model, \
            b.cycle_count, \
            COALESCE(agg.total_flights, 0) AS total_flights, \
            COALESCE(agg.total_flight_seconds, 0) AS total_flight_seconds, \
            b.last_used \
         FROM batteries b \
         LEFT JOIN LATERAL ( \
            SELECT COUNT(*)::bigint AS total_flights, \
                   COALESCE(SUM(duration_seconds), 0)::bigint AS total_flight_seconds \
            FROM flights f WHERE f.battery_id = b.id AND f.deleted_at IS NULL \
         ) agg ON TRUE \
         WHERE b.org_id = $1 AND b.compatible_model = $2 \
         ORDER BY b.cycle_count DESC",
    )
    .bind(org_id)
    .bind(model)
    .fetch_all(pool)
    .await?;

    let battery_count = batteries.len() as i32;
    let cycle_counts: Vec<i32> = batteries.iter().map(|b| b.cycle_count).collect();
    let avg_cycle_count = if !cycle_counts.is_empty() {
        cycle_counts.iter().copied().sum::<i32>() as f32 / cycle_counts.len() as f32
    } else {
        0.0
    };
    let min_cycle_count = cycle_counts.iter().copied().min().unwrap_or(0);
    let max_cycle_count = cycle_counts.iter().copied().max().unwrap_or(0);

    // Aggregate health metrics across the model's flights in last 90d
    let (avg_discharge, avg_variance): (Option<f32>, Option<f32>) = sqlx::query_as(
        "SELECT AVG(CASE WHEN f.min_battery_pct IS NOT NULL THEN 100 - f.min_battery_pct END)::real, \
                AVG(f.cell_variance_v)::real \
         FROM flights f \
         JOIN batteries b ON b.id = f.battery_id \
         WHERE b.org_id = $1 AND b.compatible_model = $2 \
           AND f.deleted_at IS NULL \
           AND f.flight_datetime > NOW() - INTERVAL '90 days'",
    )
    .bind(org_id)
    .bind(model)
    .fetch_one(pool)
    .await?;

    // Histogram with 10 fixed buckets over [0, range_max]. If no spec, use
    // ceil(max_cycle_count * 1.2) so the highest observed value lands in a bin.
    let range_max = nominal_max_cycles.unwrap_or_else(|| {
        let m = (max_cycle_count as f32 * 1.2).ceil() as i32;
        m.max(HISTOGRAM_BINS as i32)
    });
    let cycle_distribution = bucketize(&cycle_counts, range_max);

    Ok(BatteryModelComparison {
        model: model.to_string(),
        battery_count,
        nominal_max_cycles,
        cycle_uncertainty,
        nominal_capacity_wh,
        avg_cycle_count,
        min_cycle_count,
        max_cycle_count,
        avg_discharge_per_flight: avg_discharge,
        avg_cell_variance_v: avg_variance,
        cycle_distribution,
        batteries,
    })
}

fn bucketize(values: &[i32], range_max: i32) -> Vec<HistogramBucket> {
    let range_max = range_max.max(HISTOGRAM_BINS as i32);
    let bin_size_f = range_max as f32 / HISTOGRAM_BINS as f32;
    let bin_size = bin_size_f.ceil() as i32;
    let mut out: Vec<HistogramBucket> = (0..HISTOGRAM_BINS)
        .map(|i| {
            let lower = i as i32 * bin_size;
            let upper = if i + 1 == HISTOGRAM_BINS { range_max } else { (i as i32 + 1) * bin_size };
            HistogramBucket { lower, upper, count: 0 }
        })
        .collect();
    for &v in values {
        let idx = if v >= range_max {
            HISTOGRAM_BINS - 1
        } else if v <= 0 {
            0
        } else {
            ((v as f32 / bin_size_f).floor() as usize).min(HISTOGRAM_BINS - 1)
        };
        out[idx].count += 1;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimated_ranges_clamps_past_eol_to_zero() {
        let (cycles, pct) = estimated_ranges(250, Some(200), Some(40));
        let cycles = cycles.unwrap();
        // 250 cycles used, nominal 200 ±40. Low=0, mid=0, high=0.
        assert_eq!(cycles.low, 0);
        assert_eq!(cycles.mid, 0);
        assert_eq!(cycles.high, 0);
        let pct = pct.unwrap();
        assert_eq!(pct.low, 0);
        assert_eq!(pct.mid, 0);
        assert_eq!(pct.high, 0);
    }

    #[test]
    fn estimated_ranges_handles_midlife_battery() {
        let (cycles, _) = estimated_ranges(120, Some(200), Some(40));
        let cycles = cycles.unwrap();
        assert_eq!(cycles.low, 40);    // 200 - 40 - 120
        assert_eq!(cycles.mid, 80);    // 200 - 120
        assert_eq!(cycles.high, 120);  // 200 + 40 - 120
    }

    #[test]
    fn estimated_ranges_none_when_no_spec() {
        let (cycles, pct) = estimated_ranges(50, None, None);
        assert!(cycles.is_none());
        assert!(pct.is_none());
    }

    #[test]
    fn bucketize_spreads_values_across_10_bins() {
        let values = [10, 50, 100, 150, 199];
        let buckets = bucketize(&values, 200);
        assert_eq!(buckets.len(), 10);
        // 200/10 = 20 per bin
        assert_eq!(buckets[0].count, 1); // 10 → [0,20)
        assert_eq!(buckets[2].count, 1); // 50 → [40,60)
        assert_eq!(buckets[5].count, 1); // 100 → [100,120)
        assert_eq!(buckets[7].count, 1); // 150 → [140,160)
        assert_eq!(buckets[9].count, 1); // 199 → last bin
    }

    #[test]
    fn bucketize_overflow_goes_to_last_bin() {
        let values = [250];
        let buckets = bucketize(&values, 200);
        assert_eq!(buckets[9].count, 1);
    }
}
