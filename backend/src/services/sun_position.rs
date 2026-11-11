// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

//! Day/night classification for flights based on home position and start time.
//!
//! "Day" = entire flight occurs between sunrise and sunset on the flight date
//!         at home_lat/home_lon.
//! "Night" = entire flight is outside that window.
//! "Mixed" = flight crosses sunset or sunrise.
//!
//! Uses civil definitions (sunrise = sun centre at horizon). EASA strictly
//! defines "night" as between civil twilight evening and morning, which is
//! ~30 min wider than this. Refine to civil twilight if operationally needed.
//!
//! Polar day / polar night (`sun_times` returns None) classifies as
//! `Unknown` so we don't silently mislabel high-latitude operations.

use chrono::{DateTime, Duration as ChronoDuration, Utc};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DayNight {
    Day,
    Night,
    Mixed,
    /// Missing home position, missing start time, or polar day/night.
    Unknown,
}

/// Classify a single flight as day/night/mixed/unknown.
pub fn classify_flight(
    start_time: Option<DateTime<Utc>>,
    duration_seconds: i32,
    home_lat: Option<f64>,
    home_lon: Option<f64>,
) -> DayNight {
    let (Some(start), Some(lat), Some(lon)) = (start_time, home_lat, home_lon) else {
        return DayNight::Unknown;
    };

    let end = start + ChronoDuration::seconds(duration_seconds.into());
    let date = start.date_naive();

    let Some((sunrise, sunset)) = sun_times::sun_times(date, lat, lon, 0.0) else {
        return DayNight::Unknown;
    };

    let start_is_day = start >= sunrise && start <= sunset;
    let end_is_day = end >= sunrise && end <= sunset;

    match (start_is_day, end_is_day) {
        (true, true) => DayNight::Day,
        (false, false) => DayNight::Night,
        _ => DayNight::Mixed,
    }
}

/// Split flight duration into (day_seconds, night_seconds) for accurate
/// stats. The sum equals `duration_seconds` ± 1s rounding. Returns (0, 0)
/// when the flight is `Unknown` (no home / no time / polar day-night).
pub fn split_duration_by_daylight(
    start_time: Option<DateTime<Utc>>,
    duration_seconds: i32,
    home_lat: Option<f64>,
    home_lon: Option<f64>,
) -> (i32, i32) {
    let (Some(start), Some(lat), Some(lon)) = (start_time, home_lat, home_lon) else {
        return (0, 0);
    };

    let end = start + ChronoDuration::seconds(duration_seconds.into());
    let date = start.date_naive();

    let Some((sunrise, sunset)) = sun_times::sun_times(date, lat, lon, 0.0) else {
        return (0, 0);
    };

    let day_start = start.max(sunrise);
    let day_end = end.min(sunset);
    let day_seconds = if day_start < day_end {
        (day_end - day_start).num_seconds().max(0) as i32
    } else {
        0
    };
    let night_seconds = (duration_seconds - day_seconds).max(0);
    (day_seconds, night_seconds)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    // Stockholm, ~59.33 N 18.07 E. Midsummer noon — clearly daytime.
    #[test]
    fn full_day_flight_classified_as_day() {
        let start = Utc.with_ymd_and_hms(2024, 6, 21, 10, 0, 0).unwrap();
        let cls = classify_flight(Some(start), 3600, Some(59.33), Some(18.07));
        assert_eq!(cls, DayNight::Day);
    }

    // Stockholm, winter night.
    #[test]
    fn midnight_flight_classified_as_night() {
        let start = Utc.with_ymd_and_hms(2024, 12, 21, 22, 0, 0).unwrap();
        let cls = classify_flight(Some(start), 3600, Some(59.33), Some(18.07));
        assert_eq!(cls, DayNight::Night);
    }

    #[test]
    fn missing_data_returns_unknown() {
        assert_eq!(
            classify_flight(None, 3600, Some(59.33), Some(18.07)),
            DayNight::Unknown
        );
        assert_eq!(
            classify_flight(Some(Utc::now()), 3600, None, None),
            DayNight::Unknown
        );
    }

    // The day/night split must total to within 1s of the input duration on
    // any flight where classification succeeded.
    #[test]
    fn split_sums_to_duration() {
        let start = Utc.with_ymd_and_hms(2024, 6, 21, 10, 0, 0).unwrap();
        let (d, n) = split_duration_by_daylight(Some(start), 3600, Some(59.33), Some(18.07));
        assert!((d + n - 3600).abs() <= 1, "got d={d} n={n}");
    }

    // Mixed-window flight: starts before sunset, ends after — should
    // classify as Mixed and the split should attribute time to both halves.
    #[test]
    fn flight_crossing_sunset_is_mixed() {
        // 2024-09-22 in Stockholm: sunset is ~16:34 UTC (~18:34 local CEST).
        // Start at 16:00 UTC, fly 60 minutes → spans sunset → Mixed.
        let start = Utc.with_ymd_and_hms(2024, 9, 22, 16, 0, 0).unwrap();
        let cls = classify_flight(Some(start), 3600, Some(59.33), Some(18.07));
        assert_eq!(cls, DayNight::Mixed);
        let (d, n) = split_duration_by_daylight(Some(start), 3600, Some(59.33), Some(18.07));
        assert!(d > 0 && n > 0, "expected both halves > 0, got d={d} n={n}");
        assert!((d + n - 3600).abs() <= 1);
    }
}
