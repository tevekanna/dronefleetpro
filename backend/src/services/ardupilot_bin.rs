// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

//! ArduPilot `.bin` (DataFlash) flight-log parser — PR C of 3. Where the
//! `.tlog` parser reads a MAVLink telemetry stream captured by the ground
//! station, this reads the on-board DataFlash log the autopilot writes to its
//! own storage. The `.bin` is the authoritative source: it is written at the
//! flight controller, so its timing is microsecond-precise and its GPS week/ms
//! fields give an accurate UTC wall-clock anchor (the `.tlog`'s GCS timestamp
//! can drift with the logging PC's clock).
//!
//! The output shape is the shared [`ArduPilotSegmentPreview`] contract, so the
//! import/review/commit pipeline is identical to the `.tlog` path. Identity
//! (board UID, firmware, frame), the segment heuristic and the merge key all
//! reuse the helpers exported by [`crate::services::ardupilot_tlog`] so a
//! `.bin` and a `.tlog` of the same flight resolve to the same airframe and
//! near-identical `flight_guid`s.
//!
//! The parse is a single streaming pass over the DataFlash entries: only the
//! per-segment accumulators (sampled ~1 Hz trajectory + running min/max stats)
//! are retained, never the full multi-million-entry stream.

use std::io::Cursor;

use ardupilot_binlog::Reader;

use crate::errors::AppError;
use crate::services::ardupilot_findings::{into_preview_findings, FindingsAccumulator, LogFinding};
use crate::services::ardupilot_tlog::{
    format_flight_guid, haversine_m, looks_like_flight, parse_boot_banner_uid, us_to_rfc3339,
    ArduPilotSegmentPreview,
};
use crate::services::dji_parser::{FlightPreview, TrajectoryPoint};

/// GPS epoch (1980-01-06) as a Unix timestamp, and the current GPS↔UTC leap-
/// second offset (18 s as of 2017; ArduPilot GPS week/ms are in GPS time).
const GPS_UNIX_EPOCH_S: i64 = 315_964_800;
const GPS_LEAP_SECONDS: i64 = 18;
const SECONDS_PER_WEEK: i64 = 604_800;
/// Trajectory sampling interval — GPS logs at 5–10 Hz; ~1 Hz keeps the stored
/// JSON modest without losing track shape.
const TRAJECTORY_SAMPLE_US: u64 = 1_000_000;

/// Converts an ArduPilot GPS week number + milliseconds-of-week into a Unix
/// timestamp in microseconds (UTC). GPS time is ahead of UTC by the current
/// leap-second count, so we subtract it.
fn gps_week_ms_to_unix_us(gwk: i64, gms: i64) -> i64 {
    let secs = GPS_UNIX_EPOCH_S + gwk * SECONDS_PER_WEEK + gms / 1000 - GPS_LEAP_SECONDS;
    secs * 1_000_000 + (gms % 1000) * 1000
}

/// True when a decoded lat/lon pair is a plausible WGS84 fix (not the
/// null-island 0/0 sentinel emitted before GPS lock, not out of range).
fn valid_coords(lat: f64, lon: f64) -> bool {
    lat.abs() <= 90.0 && lon.abs() <= 180.0 && !(lat == 0.0 && lon == 0.0) && !lat.is_nan()
        && !lon.is_nan()
}

/// Accumulator for one arm→disarm segment, filled incrementally as the stream
/// is walked. All timestamps are raw boot-microseconds (`TimeUS`); the UTC
/// offset is applied once, at preview-build time.
struct SegAcc {
    arm_us: u64,
    disarm_us: Option<u64>,
    trajectory: Vec<TrajectoryPoint>,
    last_sample_us: Option<u64>,
    max_baro_alt: f32,
    max_speed_ms: Option<f32>,
    min_sats: Option<i32>,
    home: Option<(f64, f64)>,
}

impl SegAcc {
    fn new(arm_us: u64) -> Self {
        SegAcc {
            arm_us,
            disarm_us: None,
            trajectory: Vec::new(),
            last_sample_us: None,
            max_baro_alt: 0.0,
            max_speed_ms: None,
            min_sats: None,
            home: None,
        }
    }
}

/// Public entry point: parse a `.bin` DataFlash byte slice into one preview per
/// arm/disarm segment. Mirrors [`crate::services::ardupilot_tlog::parse_tlog`]:
/// returns every segment in file order, `looks_like_flight` advisory only. A
/// `BadRequest` is returned only when no DataFlash entries could be decoded at
/// all (i.e. the input is not a `.bin` log).
pub fn parse_bin(
    bytes: &[u8],
    filename: &str,
) -> Result<Vec<ArduPilotSegmentPreview>, AppError> {
    let mut finished: Vec<SegAcc> = Vec::new();
    let mut current: Option<SegAcc> = None;
    let mut prev_armed = false;

    // Boot→UTC offset (µs), derived from the first GPS entry carrying a real
    // week number. Constant for the whole log, so once known it applies to
    // every TimeUS (before and after the anchor).
    let mut utc_offset_us: Option<i64> = None;
    // Most-recent BARO relative altitude, used as the trajectory sample alt.
    let mut last_baro_alt: Option<f64> = None;
    let mut last_timeus: u64 = 0;
    let mut any_entry = false;

    // Shared identity across all segments (first occurrence of each wins).
    let mut uid: Option<String> = None;
    let mut firmware: Option<String> = None;
    let mut frame_label: Option<String> = None;

    // Findings accumulate across the WHOLE pass, NOT gated on arm state:
    // operationally-relevant anomalies (compass fault, EKF reset) routinely
    // occur pre-arm during ground checks and must not be dropped. Bounded and
    // deduplicated inside the accumulator — never one entry per log row.
    let mut findings_acc = FindingsAccumulator::new();

    let mut reader = Reader::new(Cursor::new(bytes));
    for result in &mut reader {
        // The reader self-recovers on corruption; a yielded Err is a single
        // undecodable entry, not a fatal stream error — skip it.
        let entry = match result {
            Ok(e) => e,
            Err(_) => continue,
        };
        any_entry = true;
        if let Some(ts) = entry.timestamp_usec {
            if ts > last_timeus {
                last_timeus = ts;
            }
        }

        match entry.name.as_str() {
            "MSG" => {
                if let Some(text) = entry.get_str("Message") {
                    let t = text.trim();
                    if uid.is_none() {
                        if let Some(u) = parse_boot_banner_uid(t) {
                            uid = Some(u);
                        }
                    }
                    // Firmware banner, e.g. "ArduCopter V4.6.3 (92b0cd78)".
                    if firmware.is_none() && t.starts_with("Ardu") {
                        firmware = Some(t.to_string());
                    }
                    // Frame banner, e.g. "Frame: QUAD/X".
                    if frame_label.is_none() {
                        if let Some(rest) = t.strip_prefix("Frame: ") {
                            let v = rest.trim();
                            if !v.is_empty() {
                                frame_label = Some(v.to_string());
                            }
                        }
                    }
                }
            }
            "GPS" => {
                let ts = match entry.timestamp_usec {
                    Some(t) => t,
                    None => continue,
                };
                // Wall-clock anchor: first GPS with a real week number fixes
                // the boot→UTC offset for the entire log.
                if utc_offset_us.is_none() {
                    if let (Some(gwk), Some(gms)) =
                        (entry.get_i64("GWk"), entry.get_i64("GMS"))
                    {
                        if gwk > 0 {
                            let gps_unix_us = gps_week_ms_to_unix_us(gwk, gms);
                            utc_offset_us = Some(gps_unix_us - ts as i64);
                        }
                    }
                }

                if let Some(seg) = current.as_mut() {
                    let status = entry.get_i64("Status").unwrap_or(0);
                    if status < 3 {
                        continue; // no 3D fix — skip
                    }
                    let (lat_i, lon_i) = (entry.get_i64("Lat"), entry.get_i64("Lng"));
                    if let (Some(la), Some(lo)) = (lat_i, lon_i) {
                        let lat = la as f64 * 1e-7;
                        let lon = lo as f64 * 1e-7;
                        if !valid_coords(lat, lon) {
                            continue;
                        }
                        // First valid fix in the segment defines home.
                        if seg.home.is_none() {
                            seg.home = Some((lat, lon));
                        }
                        if let Some(spd) = entry.get_f64("Spd") {
                            let s = spd as f32;
                            seg.max_speed_ms =
                                Some(seg.max_speed_ms.map_or(s, |m: f32| m.max(s)));
                        }
                        if let Some(ns) = entry.get_i64("NSats") {
                            let n = ns as i32;
                            seg.min_sats = Some(seg.min_sats.map_or(n, |m: i32| m.min(n)));
                        }
                        // Sample the trajectory at ~1 Hz.
                        let take = match seg.last_sample_us {
                            None => true,
                            Some(prev) => ts.saturating_sub(prev) >= TRAJECTORY_SAMPLE_US,
                        };
                        if take {
                            seg.last_sample_us = Some(ts);
                            let alt =
                                last_baro_alt.or_else(|| entry.get_f64("Alt")).unwrap_or(0.0);
                            let t_sec = ts.saturating_sub(seg.arm_us) as f64 / 1e6;
                            seg.trajectory.push(TrajectoryPoint {
                                lat,
                                lon,
                                alt,
                                t_sec,
                            });
                        }
                    }
                }
            }
            "BARO" => {
                if let Some(alt) = entry.get_f64("Alt") {
                    last_baro_alt = Some(alt);
                    if let Some(seg) = current.as_mut() {
                        if alt as f32 > seg.max_baro_alt {
                            seg.max_baro_alt = alt as f32;
                        }
                    }
                }
            }
            // Findings extraction — whole-log, not gated on `current`. All
            // fields read via the crate's Option-returning getters; labels are
            // FMT-declared and vary by firmware, so nothing is assumed present.
            "ERR" => {
                if let (Some(subsys), Some(ecode)) = (entry.get_i64("Subsys"), entry.get_i64("ECode"))
                {
                    let ts = entry.timestamp_usec.unwrap_or(last_timeus);
                    findings_acc.on_err(subsys, ecode, ts);
                }
            }
            "EV" => {
                if let Some(id) = entry.get_i64("Id") {
                    let ts = entry.timestamp_usec.unwrap_or(last_timeus);
                    findings_acc.on_event(id, ts);
                }
            }
            "VIBE" => {
                // Real schema (ArduCopter 4.6.3): [TimeUS, IMU, VibeX, VibeY,
                // VibeZ, Clip] — a SINGLE cumulative Clip counter per IMU
                // instance. Defensive: IMU defaults to 0 if absent.
                let ts = entry.timestamp_usec.unwrap_or(last_timeus);
                let imu = entry.get_i64("IMU").unwrap_or(0);
                findings_acc.on_vibe(
                    imu,
                    entry.get_f64("VibeX"),
                    entry.get_f64("VibeY"),
                    entry.get_f64("VibeZ"),
                    entry.get_i64("Clip"),
                    ts,
                );
            }
            "ARM" => {
                let ts = entry.timestamp_usec.unwrap_or(last_timeus);
                let armed = entry
                    .get_i64("ArmState")
                    .map(|v| v != 0)
                    .unwrap_or(prev_armed);
                if armed && !prev_armed {
                    current = Some(SegAcc::new(ts));
                } else if !armed && prev_armed {
                    if let Some(mut seg) = current.take() {
                        seg.disarm_us = Some(ts);
                        finished.push(seg);
                    }
                }
                prev_armed = armed;
            }
            _ => {}
        }
    }

    // Stream ended while still armed — close the final segment at the last
    // TimeUS actually seen (same semantics as split_segments).
    if let Some(mut seg) = current.take() {
        seg.disarm_us = Some(last_timeus.max(seg.arm_us));
        finished.push(seg);
    }

    if !any_entry {
        return Err(AppError::BadRequest(format!(
            "No DataFlash entries found in {filename} — not an ArduPilot .bin log"
        )));
    }

    let uid = uid.unwrap_or_else(|| "unknown-uid".to_string());
    let frame_or_type = frame_label.unwrap_or_else(|| "UNKNOWN".to_string());
    // Degraded mode: no GPS week anywhere → offset 0, times relative to the
    // Unix epoch. The log still parses; flight_start is epoch-based.
    let offset = utc_offset_us.unwrap_or(0);

    // Fold accumulated findings. They belong to the log session; a .bin is
    // overwhelmingly a single flight. When a log DOES carry multiple
    // arm/disarm segments, per-finding attribution by timestamp is a follow-up
    // — for now all findings attach to the first segment, with a warning so the
    // behaviour is visible. Offsets are computed relative to that segment's arm.
    let raw_findings = findings_acc.finish();
    if finished.len() > 1 && !raw_findings.is_empty() {
        tracing::warn!(
            target: "ardupilot_findings",
            segments = finished.len(),
            findings = raw_findings.len(),
            "multi-segment .bin: attaching all findings to the first segment"
        );
    }

    let previews = finished
        .iter()
        .enumerate()
        .map(|(i, seg)| {
            let seg_findings = if i == 0 {
                into_preview_findings(raw_findings.clone(), seg.arm_us)
            } else {
                Vec::new()
            };
            build_preview(
                seg,
                i + 1,
                filename,
                &uid,
                &frame_or_type,
                firmware.as_deref(),
                offset,
                seg_findings,
            )
        })
        .collect();

    Ok(previews)
}

/// Builds one [`ArduPilotSegmentPreview`] from a filled accumulator, applying
/// the boot→UTC `offset` and mirroring the `.tlog` parser's field set + units.
#[allow(clippy::too_many_arguments)]
fn build_preview(
    seg: &SegAcc,
    index: usize,
    filename: &str,
    uid: &str,
    frame_or_type: &str,
    firmware: Option<&str>,
    offset: i64,
    findings: Vec<LogFinding>,
) -> ArduPilotSegmentPreview {
    let disarm_us = seg.disarm_us.unwrap_or(seg.arm_us);

    // Distances from the accumulated trajectory + segment home.
    let mut total_distance_m: f32 = 0.0;
    let mut max_distance_m: f32 = 0.0;
    let mut prev: Option<(f64, f64)> = None;
    for p in &seg.trajectory {
        if let Some((plat, plon)) = prev {
            total_distance_m += haversine_m(plat, plon, p.lat, p.lon);
        }
        if let Some((hlat, hlon)) = seg.home {
            let d = haversine_m(hlat, hlon, p.lat, p.lon);
            if d > max_distance_m {
                max_distance_m = d;
            }
        }
        prev = Some((p.lat, p.lon));
    }

    let duration_seconds = (disarm_us.saturating_sub(seg.arm_us) / 1_000_000) as i32;
    let armed_seconds = duration_seconds;
    let looks = looks_like_flight(seg.max_baro_alt, armed_seconds);

    // Apply the boot→UTC offset once. Clamp at 0 so a degraded-mode negative
    // never underflows the u64 formatter.
    let arm_utc_us = (seg.arm_us as i64 + offset).max(0);
    let disarm_utc_us = (disarm_us as i64 + offset).max(0);

    let preview = FlightPreview {
        source_filename: format!("{filename}#seg{index}"),
        flight_guid: Some(format_flight_guid(uid, arm_utc_us / 1_000_000)),
        aircraft_name: Some(format!("ArduPilot {frame_or_type}")),
        aircraft_model: None,
        aircraft_serial: Some(uid.to_string()),
        flight_start: us_to_rfc3339(arm_utc_us as u64),
        flight_end: us_to_rfc3339(disarm_utc_us as u64),
        duration_seconds,
        max_altitude_m: seg.max_baro_alt,
        max_distance_m,
        total_distance_m,
        max_speed_ms: seg.max_speed_ms,
        home_lat: seg.home.map(|(la, _)| la as f32),
        home_lon: seg.home.map(|(_, lo)| lo as f32),
        satellite_count_min: seg.min_sats,
        battery_temp_c: None,
        cell_variance_v: None,
        min_battery_pct: None,
        battery_serial: None,
        rc_serial: None,
        app_version: firmware.map(str::to_string),
        app_source: Some("ArduPilot".to_string()),
        drone_type_code: Some(frame_or_type.to_string()),
        photo_count: 0,
        video_seconds: 0,
        trajectory: seg.trajectory.clone(),
        model_detection_status: Some("unknown".to_string()),
        aircraft_match: None,
        battery_match: None,
        controller_match: None,
        findings,
    };

    ArduPilotSegmentPreview {
        preview,
        looks_like_flight: looks,
        armed_seconds,
        already_imported: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gps_week_ms_maps_to_expected_unix_time() {
        // Sample from a real CubeOrangePlus log: GWk 2426, GMS 286_736_000
        // → 2026-07-08T07:38:38Z, i.e. Unix 1_783_496_318.
        let us = gps_week_ms_to_unix_us(2426, 286_736_000);
        assert_eq!(us / 1_000_000, 1_783_496_318);
        // Whole-second GMS → zero sub-second remainder.
        assert_eq!(us % 1_000_000, 0);
    }

    #[test]
    fn gps_week_ms_carries_sub_second_millis() {
        // 250 ms past the whole second should surface as 250_000 µs.
        let us = gps_week_ms_to_unix_us(2426, 286_736_250);
        assert_eq!(us % 1_000_000, 250_000);
    }

    #[test]
    fn valid_coords_rejects_null_island_and_out_of_range() {
        assert!(valid_coords(59.3293, 18.0686));
        assert!(!valid_coords(0.0, 0.0));
        assert!(!valid_coords(91.0, 18.0));
        assert!(!valid_coords(59.0, 181.0));
    }
}
