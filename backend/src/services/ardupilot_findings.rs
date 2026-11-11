// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

//! Extraction of operationally-relevant findings from ArduPilot `.bin`
//! (DataFlash) logs. Fed one message at a time from the single streaming pass
//! in [`crate::services::ardupilot_bin`] so the parser's ~2.4 MB peak (on a
//! 141 MB log) is preserved: findings accumulate into a BOUNDED, DEDUPLICATED
//! map keyed by `code`, never one row per log entry (a real log carries tens
//! of thousands of VIBE rows).
//!
//! Three message families are handled:
//!   * `ERR`  — subsystem + error code, aggregated by `(Subsys, ECode)`.
//!   * `EV`   — a curated subset of events that mean something operationally
//!              happened; routine arm/disarm/takeoff/land are excluded (we
//!              already model flight segments from `ARM`).
//!   * `VIBE` — per-IMU vibration + a single cumulative clip counter,
//!              aggregated across the whole flight into at most one finding.
//!
//! # Provenance of the decodings
//! Subsystem/event/error-code names and values are taken verbatim from
//! ArduPilot `libraries/AP_Logger/AP_Logger.h` at tag **`Copter-4.6.3`** (the
//! firmware in the reference fixture: *ArduCopter V4.6.3*). Every mapped value
//! is cited inline against its enum line. Nothing here is written from memory —
//! a mistranslated failsafe is worse than no finding at all.

use std::collections::BTreeMap;

/// Maximum number of distinct `code`s retained per log. A well-behaved log has
/// a handful; the cap is a runaway guard so a corrupt/adversarial log cannot
/// blow up memory. Excess codes are dropped with a `tracing::warn`.
const MAX_DISTINCT_CODES: usize = 200;

/// VIBE severity thresholds, in m/s^2 (peak per-axis vibration per IMU).
///
/// Source: ArduPilot "Measuring Vibration" wiki —
/// <https://ardupilot.org/copter/docs/common-measuring-vibration.html>.
/// That page states vibration levels below ~30 m/s^2 are typically fine, the
/// 30–60 m/s^2 band is a grey zone worth investigating, and levels above
/// ~60 m/s^2 are likely to cause problems. Accelerometer *clipping* (the
/// per-IMU `Clip` counter incrementing) is bad at any vibration level. Whoever
/// retunes these numbers: update them against that wiki page, not from memory.
const VIBE_MEDIUM_MS2: f64 = 30.0;
const VIBE_HIGH_MS2: f64 = 60.0;

/// A single extracted finding, in preview shape. `Serialize + Deserialize`
/// so it round-trips through the browser inside `FlightPreview` (like
/// `trajectory`) and is read back verbatim by the commit handler.
///
/// Offsets are raw microsecond timestamps here (`*_ts_us`); they are converted
/// to segment-arm-relative seconds when a finding is attached to a flight
/// segment in `ardupilot_bin`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LogFinding {
    pub source: String,
    pub code: String,
    pub subsystem: Option<String>,
    pub subsys_id: Option<i32>,
    pub ecode: Option<i32>,
    pub severity: String,
    pub message: String,
    pub occurrence_count: i32,
    /// Seconds relative to segment arm; may be negative for pre-arm ground
    /// events. Filled by `into_preview_findings` from `first_ts_us`.
    pub first_offset_seconds: Option<f32>,
    pub last_offset_seconds: Option<f32>,
    pub resolved: bool,
    /// Raw first/last microsecond timestamps, retained only so offsets can be
    /// computed once the segment arm-time is known. Never persisted (skipped
    /// in serialisation) — the DB stores the derived `*_offset_seconds`.
    #[serde(skip)]
    pub first_ts_us: u64,
    #[serde(skip)]
    pub last_ts_us: u64,
}

const SOURCE_ARDUPILOT_BIN: &str = "ardupilot_bin";

/// Human name for a `LogErrorSubsystem` value.
/// Verbatim from AP_Logger.h @ Copter-4.6.3, lines 112–144.
fn subsystem_name(subsys: i64) -> Option<&'static str> {
    Some(match subsys {
        1 => "MAIN",                 // AP_Logger.h:113  MAIN = 1
        2 => "RADIO",                // :114  RADIO = 2
        3 => "COMPASS",              // :115  COMPASS = 3
        4 => "OPTFLOW",              // :116  OPTFLOW = 4 (not used)
        5 => "FAILSAFE_RADIO",       // :117  FAILSAFE_RADIO = 5
        6 => "FAILSAFE_BATT",        // :118  FAILSAFE_BATT = 6
        7 => "FAILSAFE_GPS",         // :119  FAILSAFE_GPS = 7 (not used)
        8 => "FAILSAFE_GCS",         // :120  FAILSAFE_GCS = 8
        9 => "FAILSAFE_FENCE",       // :121  FAILSAFE_FENCE = 9
        10 => "FLIGHT_MODE",         // :122  FLIGHT_MODE = 10
        11 => "GPS",                 // :123  GPS = 11
        12 => "CRASH_CHECK",         // :124  CRASH_CHECK = 12
        13 => "FLIP",                // :125  FLIP = 13
        14 => "AUTOTUNE",            // :126  AUTOTUNE = 14 (not used)
        15 => "PARACHUTES",          // :127  PARACHUTES = 15
        16 => "EKFCHECK",            // :128  EKFCHECK = 16
        17 => "FAILSAFE_EKFINAV",    // :129  FAILSAFE_EKFINAV = 17
        18 => "BARO",                // :130  BARO = 18
        19 => "CPU",                 // :131  CPU = 19
        20 => "FAILSAFE_ADSB",       // :132  FAILSAFE_ADSB = 20
        21 => "TERRAIN",             // :133  TERRAIN = 21
        22 => "NAVIGATION",          // :134  NAVIGATION = 22
        23 => "FAILSAFE_TERRAIN",    // :135  FAILSAFE_TERRAIN = 23
        24 => "EKF_PRIMARY",         // :136  EKF_PRIMARY = 24
        25 => "THRUST_LOSS_CHECK",   // :137  THRUST_LOSS_CHECK = 25
        26 => "FAILSAFE_SENSORS",    // :138  FAILSAFE_SENSORS = 26
        27 => "FAILSAFE_LEAK",       // :139  FAILSAFE_LEAK = 27
        28 => "PILOT_INPUT",         // :140  PILOT_INPUT = 28
        29 => "FAILSAFE_VIBE",       // :141  FAILSAFE_VIBE = 29
        30 => "INTERNAL_ERROR",      // :142  INTERNAL_ERROR = 30
        31 => "FAILSAFE_DEADRECKON", // :143  FAILSAFE_DEADRECKON = 31
        _ => return None,
    })
}

/// kebab-case slug for a subsystem, used to build stable `code`s.
fn subsystem_slug(subsys: i64) -> String {
    match subsystem_name(subsys) {
        Some(n) => n.to_ascii_lowercase(),
        None => format!("subsys_{subsys}"),
    }
}

/// Severity + message for an ERR `(Subsys, ECode)` pair with a NON-zero ecode.
///
/// Only pairs GROUNDED in this PR get a specific severity; every other pair
/// gets `medium` and a generic (but truthful) message — we never guess a
/// decoding. `EKF_PRIMARY` is special-cased by the caller before reaching here.
///
/// Grounded `LogErrorCode` provenance (AP_Logger.h @ Copter-4.6.3):
///   * `(6, 1)` FAILSAFE_BATT + ECode 1 → `FAILSAFE_OCCURRED = 1` (:157,
///     section ":155 subsystem specific error codes -- failsafe_thr, batt, gps")
///     → battery failsafe occurred. Severity **high**.
///   * `(25, 1)` THRUST_LOSS_CHECK + ECode 1 → the enum has no thrust-specific
///     section; ECode 1 is the general `FAILSAFE_OCCURRED = 1` (:157) /
///     `FAILED_TO_INITIALISE = 1` (:151) "occurred/triggered" edge → thrust
///     loss detected. Severity **high** (Phase-0 grounded & approved).
fn classify_err_nonzero(subsys: i64, ecode: i64, name: Option<&str>) -> (&'static str, String) {
    let label = name.map(|n| n.to_string()).unwrap_or_else(|| format!("subsystem {subsys}"));
    match (subsys, ecode) {
        // FAILSAFE_BATT occurred — AP_Logger.h:118 + :157.
        (6, 1) => ("high", "Battery failsafe triggered".to_string()),
        // THRUST_LOSS_CHECK triggered — AP_Logger.h:137 + :157.
        (25, 1) => ("high", "Thrust loss detected".to_string()),
        // Everything else: no confident source decoding of this exact
        // (subsys, ecode) → generic message, medium. Never a guessed decode.
        _ => (
            "medium",
            format!("ArduPilot log error: {label} (error code {ecode})"),
        ),
    }
}

/// EV ids that constitute a finding, with `(code_slug, message, severity)`.
/// Everything not listed here is intentionally dropped (routine/config noise).
///
/// Ids verified verbatim against AP_Logger.h `enum class LogEvent` @
/// Copter-4.6.3. Each inclusion is justified; the excluded routine ids
/// (10 ARMED, 15 AUTO_ARMED, 25 SET_HOME, 28 NOT_LANDED, 57 MOTORS_INTERLOCK_
/// ENABLED, 73 LAND_REPO_ACTIVE, plus all takeoff/land/fence-config events)
/// fall through to `None`.
fn classify_event(id: i64) -> Option<(&'static str, &'static str, &'static str)> {
    Some(match id {
        // :32  LOST_GPS = 19 — GPS lost mid-operation. Flight-critical.
        19 => ("ev.lost_gps", "GPS signal lost during flight", "high"),
        // :34  PARACHUTE_RELEASED = 51 — chute fired = emergency/terminal event.
        51 => ("ev.parachute_released", "Parachute released", "critical"),
        // :37  MOTORS_EMERGENCY_STOPPED = 54 — emergency motor stop.
        54 => ("ev.motors_emergency_stopped", "Motors emergency-stopped", "high"),
        // :42  ROTOR_SPEED_BELOW_CRITICAL = 59 (heli) — rotor underspeed, flight-critical.
        59 => (
            "ev.rotor_speed_below_critical",
            "Rotor speed dropped below critical (helicopter)",
            "high",
        ),
        // :43  EKF_ALT_RESET = 60 — estimator altitude reset; instability signal.
        60 => ("ev.ekf_alt_reset", "EKF altitude estimate reset", "medium"),
        // :44  LAND_CANCELLED_BY_PILOT = 61 — pilot aborted an automatic landing.
        61 => ("ev.land_cancelled_by_pilot", "Automatic landing cancelled by pilot", "medium"),
        // :45  EKF_YAW_RESET = 62 — estimator yaw reset; instability signal.
        //      Phase-0 grounded & approved at severity medium.
        62 => ("ev.ekf_yaw_reset", "EKF yaw estimate reset", "medium"),
        _ => return None,
    })
}

/// Per-IMU vibration accumulator: max per-axis vibration and max cumulative
/// clip counter.
#[derive(Default, Clone, Copy)]
struct VibeImu {
    max_x: f64,
    max_y: f64,
    max_z: f64,
    max_clip: i64,
}

/// Accumulates findings across a single streaming pass over a `.bin`.
///
/// ERR/EV findings live in `map` keyed by `code` (bounded at
/// `MAX_DISTINCT_CODES`). VIBE is aggregated separately per IMU and folded into
/// at most one finding by [`Self::finish`].
pub struct FindingsAccumulator {
    map: BTreeMap<String, LogFinding>,
    capped: bool,
    /// Per-IMU vibration state.
    vibe: BTreeMap<i64, VibeImu>,
    /// First/last timestamps where any axis crossed the medium threshold,
    /// so the VIBE finding can carry a meaningful time window.
    vibe_over_first_ts: Option<u64>,
    vibe_over_last_ts: Option<u64>,
}

impl FindingsAccumulator {
    pub fn new() -> Self {
        FindingsAccumulator {
            map: BTreeMap::new(),
            capped: false,
            vibe: BTreeMap::new(),
            vibe_over_first_ts: None,
            vibe_over_last_ts: None,
        }
    }

    /// Insert-or-aggregate a keyed finding. Returns a mutable ref to the stored
    /// finding, or `None` if the distinct-code cap was hit (caller drops it).
    fn upsert<'a>(
        map: &'a mut BTreeMap<String, LogFinding>,
        capped: &mut bool,
        code: String,
        ts: u64,
        build: impl FnOnce() -> LogFinding,
    ) -> Option<&'a mut LogFinding> {
        if let Some(existing) = map.get_mut(&code) {
            existing.occurrence_count += 1;
            existing.first_ts_us = existing.first_ts_us.min(ts);
            existing.last_ts_us = existing.last_ts_us.max(ts);
            return Some(map.get_mut(&code).unwrap());
        }
        if map.len() >= MAX_DISTINCT_CODES {
            if !*capped {
                *capped = true;
                tracing::warn!(
                    target: "ardupilot_findings",
                    cap = MAX_DISTINCT_CODES,
                    "distinct-code cap reached; dropping further findings for this log"
                );
            }
            return None;
        }
        map.insert(code.clone(), build());
        map.get_mut(&code)
    }

    /// Handle an `ERR` entry. `ecode == 0` is the CLEARED edge (see
    /// `LogErrorCode::ERROR_RESOLVED / FAILSAFE_RESOLVED = 0`, AP_Logger.h:150,
    /// :156): it resolves the matching subsystem's finding(s) rather than
    /// creating its own. A cleared edge with no preceding occurrence in the log
    /// is recorded as an explicit `resolved`, low finding.
    pub fn on_err(&mut self, subsys: i64, ecode: i64, ts: u64) {
        let name = subsystem_name(subsys);

        // ECode 0 = condition cleared.
        if ecode == 0 {
            // Resolve any earlier non-zero occurrence(s) for this subsystem.
            let mut matched = false;
            for f in self.map.values_mut() {
                if f.subsys_id == Some(subsys as i32) && f.ecode != Some(0) {
                    f.resolved = true;
                    f.last_ts_us = f.last_ts_us.max(ts);
                    matched = true;
                }
            }
            if matched {
                return;
            }
            // Resolved-only: the fault edge (if any) predates logging. Record it
            // explicitly and say so, so we don't imply we know more than we do.
            let label = name.map(|n| n.to_string()).unwrap_or_else(|| format!("subsystem {subsys}"));
            let code = format!("err.{}.0", subsystem_slug(subsys));
            Self::upsert(&mut self.map, &mut self.capped, code, ts, || LogFinding {
                source: SOURCE_ARDUPILOT_BIN.to_string(),
                code: format!("err.{}.0", subsystem_slug(subsys)),
                subsystem: name.map(str::to_string),
                subsys_id: Some(subsys as i32),
                ecode: Some(0),
                severity: "low".to_string(),
                message: format!(
                    "{label} reported cleared/OK; only the cleared edge appears in this log \
                     (any preceding fault predates logging)"
                ),
                occurrence_count: 1,
                first_offset_seconds: None,
                last_offset_seconds: None,
                resolved: true,
                first_ts_us: ts,
                last_ts_us: ts,
            });
            return;
        }

        // EKF_PRIMARY (subsys 24): ECode is the primary CORE INDEX, not an
        // error code (AP_Logger.h:136). Render as informational 'low' — never
        // as an error.
        if subsys == 24 {
            let code = format!("err.ekf_primary.{ecode}");
            Self::upsert(&mut self.map, &mut self.capped, code.clone(), ts, || LogFinding {
                source: SOURCE_ARDUPILOT_BIN.to_string(),
                code,
                subsystem: Some("EKF_PRIMARY".to_string()),
                subsys_id: Some(24),
                ecode: Some(ecode as i32),
                severity: "low".to_string(),
                message: format!("EKF primary core switched to core {ecode}"),
                occurrence_count: 1,
                first_offset_seconds: None,
                last_offset_seconds: None,
                resolved: false,
                first_ts_us: ts,
                last_ts_us: ts,
            });
            return;
        }

        let (severity, message) = classify_err_nonzero(subsys, ecode, name);
        let code = format!("err.{}.{ecode}", subsystem_slug(subsys));
        let sev = severity.to_string();
        Self::upsert(&mut self.map, &mut self.capped, code.clone(), ts, || LogFinding {
            source: SOURCE_ARDUPILOT_BIN.to_string(),
            code,
            subsystem: name.map(str::to_string),
            subsys_id: Some(subsys as i32),
            ecode: Some(ecode as i32),
            severity: sev,
            message,
            occurrence_count: 1,
            first_offset_seconds: None,
            last_offset_seconds: None,
            resolved: false,
            first_ts_us: ts,
            last_ts_us: ts,
        });
    }

    /// Handle an `EV` entry. Only curated ids produce a finding.
    pub fn on_event(&mut self, id: i64, ts: u64) {
        let Some((code, message, severity)) = classify_event(id) else {
            return;
        };
        Self::upsert(&mut self.map, &mut self.capped, code.to_string(), ts, || LogFinding {
            source: SOURCE_ARDUPILOT_BIN.to_string(),
            code: code.to_string(),
            subsystem: None,
            subsys_id: None,
            ecode: None,
            severity: severity.to_string(),
            message: message.to_string(),
            occurrence_count: 1,
            first_offset_seconds: None,
            last_offset_seconds: None,
            resolved: false,
            first_ts_us: ts,
            last_ts_us: ts,
        });
    }

    /// Handle a `VIBE` entry. Real schema (ArduCopter 4.6.3):
    /// `[TimeUS, IMU, VibeX, VibeY, VibeZ, Clip]` — a SINGLE cumulative `Clip`
    /// counter per IMU instance. Aggregated per IMU; folded into one finding by
    /// `finish`. Never one finding per row (the reference log has 40,032 rows).
    pub fn on_vibe(
        &mut self,
        imu: i64,
        vx: Option<f64>,
        vy: Option<f64>,
        vz: Option<f64>,
        clip: Option<i64>,
        ts: u64,
    ) {
        let slot = self.vibe.entry(imu).or_default();
        let mut peak = f64::MIN;
        if let Some(v) = vx {
            slot.max_x = slot.max_x.max(v);
            peak = peak.max(v);
        }
        if let Some(v) = vy {
            slot.max_y = slot.max_y.max(v);
            peak = peak.max(v);
        }
        if let Some(v) = vz {
            slot.max_z = slot.max_z.max(v);
            peak = peak.max(v);
        }
        if let Some(c) = clip {
            slot.max_clip = slot.max_clip.max(c);
        }
        if peak >= VIBE_MEDIUM_MS2 {
            self.vibe_over_first_ts.get_or_insert(ts);
            self.vibe_over_last_ts = Some(ts);
        }
    }

    /// Fold the per-IMU VIBE state into at most one finding and return the
    /// complete finding list. Raw timestamps are still in `*_ts_us`; the caller
    /// converts them to segment-arm-relative offsets.
    pub fn finish(mut self) -> Vec<LogFinding> {
        if let Some(vibe) = self.build_vibe_finding() {
            // VIBE always emitted regardless of the ERR/EV code cap.
            self.map.entry(vibe.code.clone()).or_insert(vibe);
        }
        self.map.into_values().collect()
    }

    fn build_vibe_finding(&self) -> Option<LogFinding> {
        if self.vibe.is_empty() {
            return None;
        }
        let mut peak_axis = "";
        let mut peak_val = f64::MIN;
        let mut peak_imu = 0i64;
        let mut any_clip = 0i64;
        let mut clip_imu = 0i64;
        for (imu, v) in &self.vibe {
            for (axis, val) in [("VibeX", v.max_x), ("VibeY", v.max_y), ("VibeZ", v.max_z)] {
                if val > peak_val {
                    peak_val = val;
                    peak_axis = axis;
                    peak_imu = *imu;
                }
            }
            if v.max_clip > any_clip {
                any_clip = v.max_clip;
                clip_imu = *imu;
            }
        }

        // Clipping is bad at any vibration level; otherwise band by peak axis.
        let severity = if any_clip > 0 {
            "high"
        } else if peak_val > VIBE_HIGH_MS2 {
            "high"
        } else if peak_val >= VIBE_MEDIUM_MS2 {
            "medium"
        } else {
            return None; // below threshold, no clipping → not a finding
        };

        let clip_note = if any_clip > 0 {
            format!("; accelerometer clipping detected ({any_clip} on IMU {clip_imu})")
        } else {
            "; no accelerometer clipping".to_string()
        };
        let code = if severity == "high" { "vibe.high" } else { "vibe.medium" };

        Some(LogFinding {
            source: SOURCE_ARDUPILOT_BIN.to_string(),
            code: code.to_string(),
            subsystem: Some("VIBE".to_string()),
            subsys_id: None,
            ecode: None,
            severity: severity.to_string(),
            message: format!(
                "Vibration: peak {peak_axis} {peak_val:.1} m/s^2 (IMU {peak_imu}){clip_note}"
            ),
            occurrence_count: 1,
            first_offset_seconds: None,
            last_offset_seconds: None,
            resolved: false,
            first_ts_us: self.vibe_over_first_ts.unwrap_or(0),
            last_ts_us: self.vibe_over_last_ts.unwrap_or(0),
        })
    }
}

impl Default for FindingsAccumulator {
    fn default() -> Self {
        Self::new()
    }
}

/// Convert accumulated findings to preview shape, filling `*_offset_seconds`
/// relative to `arm_us`. Offsets may be negative for pre-arm/ground events —
/// that is intentional and truthful (e.g. a compass fault during pre-flight
/// checks precedes arm).
pub fn into_preview_findings(mut findings: Vec<LogFinding>, arm_us: u64) -> Vec<LogFinding> {
    for f in &mut findings {
        f.first_offset_seconds = Some(offset_seconds(f.first_ts_us, arm_us));
        f.last_offset_seconds = Some(offset_seconds(f.last_ts_us, arm_us));
    }
    // Stable, useful ordering for the detail view: earliest first.
    findings.sort_by(|a, b| {
        a.first_offset_seconds
            .partial_cmp(&b.first_offset_seconds)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.code.cmp(&b.code))
    });
    findings
}

fn offset_seconds(ts_us: u64, arm_us: u64) -> f32 {
    (ts_us as i64 - arm_us as i64) as f32 / 1_000_000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn err_aggregation_increments_count_not_rows() {
        let mut acc = FindingsAccumulator::new();
        // FAILSAFE_BATT occurred twice.
        acc.on_err(6, 1, 1_000_000);
        acc.on_err(6, 1, 2_000_000);
        let out = acc.finish();
        let batt: Vec<_> = out.iter().filter(|f| f.code == "err.failsafe_batt.1").collect();
        assert_eq!(batt.len(), 1, "one row for a repeated (subsys, ecode)");
        assert_eq!(batt[0].occurrence_count, 2);
        assert_eq!(batt[0].severity, "high");
        assert_eq!(batt[0].subsys_id, Some(6));
        assert_eq!(batt[0].ecode, Some(1));
        assert!(!batt[0].resolved);
    }

    #[test]
    fn ecode_zero_resolves_prior_occurrence() {
        let mut acc = FindingsAccumulator::new();
        acc.on_err(6, 1, 1_000_000); // occurred
        acc.on_err(6, 0, 3_000_000); // cleared
        let out = acc.finish();
        // The cleared edge resolves the occurrence; it does NOT add its own row.
        assert_eq!(out.iter().filter(|f| f.subsys_id == Some(6)).count(), 1);
        let batt = out.iter().find(|f| f.code == "err.failsafe_batt.1").unwrap();
        assert!(batt.resolved, "cleared edge must mark the occurrence resolved");
        assert!(out.iter().all(|f| f.code != "err.failsafe_batt.0"));
    }

    #[test]
    fn resolved_only_err_produces_low_resolved_finding() {
        let mut acc = FindingsAccumulator::new();
        // COMPASS cleared with no preceding occurrence in the log (the fixture case).
        acc.on_err(3, 0, 15_351_048);
        let out = acc.finish();
        let compass = out.iter().find(|f| f.code == "err.compass.0").unwrap();
        assert_eq!(compass.severity, "low");
        assert!(compass.resolved);
        assert_eq!(compass.subsys_id, Some(3));
        assert_eq!(compass.ecode, Some(0));
        assert!(
            compass.message.contains("only the cleared edge"),
            "message must not imply we know the fault edge: {}",
            compass.message
        );
    }

    #[test]
    fn ekf_primary_is_informational_not_error() {
        let mut acc = FindingsAccumulator::new();
        acc.on_err(24, 1, 17_221_282);
        let out = acc.finish();
        let ekf = out.iter().find(|f| f.subsys_id == Some(24)).unwrap();
        assert_eq!(ekf.severity, "low");
        assert!(ekf.message.contains("EKF primary core switched to core 1"));
        assert!(!ekf.resolved);
    }

    #[test]
    fn event_curated_subset_filters_routine_ids() {
        let mut acc = FindingsAccumulator::new();
        // Routine ids from the fixture — must NOT produce findings.
        for id in [10, 15, 25, 28, 57, 73] {
            acc.on_event(id, 1_000_000);
        }
        // Curated: EKF_YAW_RESET three times.
        acc.on_event(62, 2_000_000);
        acc.on_event(62, 3_000_000);
        acc.on_event(62, 4_000_000);
        let out = acc.finish();
        assert_eq!(out.len(), 1, "only the curated event should survive");
        let yaw = &out[0];
        assert_eq!(yaw.code, "ev.ekf_yaw_reset");
        assert_eq!(yaw.occurrence_count, 3);
        assert_eq!(yaw.severity, "medium");
    }

    #[test]
    fn vibe_thousands_of_rows_produce_at_most_one_finding() {
        let mut acc = FindingsAccumulator::new();
        // Simulate three IMUs, thousands of samples. IMU 2 exceeds the high
        // threshold on Z; no clipping — mirrors the fixture.
        for i in 0..10_000u64 {
            let ts = 400_000_000 + i * 2500;
            acc.on_vibe(0, Some(15.0), Some(33.0), Some(40.0), Some(0), ts);
            acc.on_vibe(1, Some(16.0), Some(33.0), Some(40.0), Some(0), ts);
            acc.on_vibe(2, Some(22.0), Some(41.0), Some(125.5), Some(0), ts);
        }
        let out = acc.finish();
        let vibe: Vec<_> = out.iter().filter(|f| f.subsystem.as_deref() == Some("VIBE")).collect();
        assert_eq!(vibe.len(), 1, "at most one VIBE finding regardless of row count");
        assert_eq!(vibe[0].severity, "high"); // VibeZ 125.5 > 60
        assert_eq!(vibe[0].code, "vibe.high");
        assert!(vibe[0].message.contains("VibeZ 125.5"));
        assert!(vibe[0].message.contains("no accelerometer clipping"));
    }

    #[test]
    fn vibe_clipping_forces_high_regardless_of_level() {
        let mut acc = FindingsAccumulator::new();
        // Low vibration but clipping occurred → high.
        acc.on_vibe(0, Some(10.0), Some(12.0), Some(14.0), Some(5), 400_000_000);
        let out = acc.finish();
        let vibe = out.iter().find(|f| f.subsystem.as_deref() == Some("VIBE")).unwrap();
        assert_eq!(vibe.severity, "high");
        assert!(vibe.message.contains("clipping detected"));
    }

    #[test]
    fn vibe_below_threshold_no_finding() {
        let mut acc = FindingsAccumulator::new();
        acc.on_vibe(0, Some(10.0), Some(12.0), Some(20.0), Some(0), 400_000_000);
        let out = acc.finish();
        assert!(out.iter().all(|f| f.subsystem.as_deref() != Some("VIBE")));
    }

    #[test]
    fn distinct_code_cap_is_enforced() {
        let mut acc = FindingsAccumulator::new();
        // Push well past the cap with distinct subsystem/ecode pairs.
        for s in 1..=60i64 {
            for c in 1..=10i64 {
                acc.on_err(s, c, 1_000_000);
            }
        }
        let out = acc.finish();
        // ERR/EV codes are capped; VIBE (none here) excluded.
        assert!(out.len() <= MAX_DISTINCT_CODES, "cap must bound the map: got {}", out.len());
    }

    #[test]
    fn offsets_are_arm_relative_and_may_be_negative() {
        let mut acc = FindingsAccumulator::new();
        acc.on_err(3, 0, 15_000_000); // pre-arm ground event
        acc.on_err(6, 1, 400_000_000); // in-flight
        let arm_us = 353_332_002;
        let out = into_preview_findings(acc.finish(), arm_us);
        let compass = out.iter().find(|f| f.code == "err.compass.0").unwrap();
        assert!(
            compass.first_offset_seconds.unwrap() < 0.0,
            "pre-arm event has a negative offset"
        );
        let batt = out.iter().find(|f| f.code == "err.failsafe_batt.1").unwrap();
        assert!(batt.first_offset_seconds.unwrap() > 0.0);
    }
}
