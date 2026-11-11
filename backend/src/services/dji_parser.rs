// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

use crate::errors::AppError;
use crate::services::ardupilot_findings::LogFinding;
use crate::services::sn_matching::EquipmentMatch;

#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
pub struct FlightPreview {
    pub source_filename: String,
    pub flight_guid: Option<String>,
    pub aircraft_name: Option<String>,
    /// Canonical aircraft_models.name resolved from drone_type_code via
    /// `dji_model_mapping::canonical_model_name`. Distinct from
    /// `aircraft_name`, which is the DJI app's operator-set device label
    /// (often a nickname like "Old Faithful" rather than a model). `None` when
    /// the drone-type variant has no unambiguous catalog mapping.
    pub aircraft_model: Option<String>,
    pub aircraft_serial: Option<String>,
    pub flight_start: Option<String>,
    pub flight_end: Option<String>,
    pub duration_seconds: i32,
    pub max_altitude_m: f32,
    pub max_distance_m: f32,
    pub total_distance_m: f32,
    pub max_speed_ms: Option<f32>,
    pub home_lat: Option<f32>,
    pub home_lon: Option<f32>,
    pub satellite_count_min: Option<i32>,
    pub battery_temp_c: Option<f32>,
    pub cell_variance_v: Option<f32>,
    pub min_battery_pct: Option<i32>,
    pub battery_serial: Option<String>,
    pub rc_serial: Option<String>,
    pub app_version: Option<String>,
    pub app_source: Option<String>,
    pub drone_type_code: Option<String>,
    pub photo_count: i32,
    pub video_seconds: i32,
    pub trajectory: Vec<TrajectoryPoint>,
    /// One of: "from_aircraft_db" (resolved at import time by aircraft serial),
    /// "from_mapping" (resolved via drone_type_mappings), "from_recover"
    /// (extracted from the log's own recover.aircraft_name), or "unknown"
    /// (no source could name the aircraft model). Set to `"from_recover"` or
    /// `"unknown"` by the parser; upgraded to the other two values by
    /// handlers::flights::import.
    pub model_detection_status: Option<String>,
    /// Backend-computed match for the log's `aircraft_serial` against the
    /// org's aircraft. `Some` if `find_match` (exact-then-prefix-tolerance)
    /// found a unique row; `None` otherwise. Populated by the import
    /// handler — the parser leaves these as `None`.
    #[serde(default)]
    pub aircraft_match: Option<EquipmentMatch>,
    /// Same shape as `aircraft_match`, for the log's `battery_serial`.
    #[serde(default)]
    pub battery_match: Option<EquipmentMatch>,
    /// Same shape as `aircraft_match`, for the log's `rc_serial`.
    #[serde(default)]
    pub controller_match: Option<EquipmentMatch>,
    /// Operationally-relevant findings extracted from the log (ArduPilot .bin
    /// only today; empty for DJI/CSV/.tlog). Round-trips through the browser
    /// like `trajectory`; persisted by the commit handler into
    /// `flight_log_findings`.
    #[serde(default)]
    pub findings: Vec<LogFinding>,
}

#[derive(Debug, serde::Serialize, serde::Deserialize, Clone)]
pub struct TrajectoryPoint {
    pub lat: f64,
    pub lon: f64,
    pub alt: f64,
    pub t_sec: f64,
}

/// Detects the app source from the raw app_version string.
fn detect_app_source(raw: &str) -> String {
    let lower = raw.to_lowercase();
    if lower.contains("litchi") {
        "Litchi".to_string()
    } else if lower.contains("djifly") || lower.contains("dji fly") {
        "DJIFly".to_string()
    } else if raw.starts_with(|c: char| c.is_ascii_digit()) {
        // DJI GO 4 format like "4.3.52"
        "DJIGO4".to_string()
    } else {
        raw.to_string()
    }
}

/// Cleans an aircraft model name by stripping owner prefixes.
/// DJI logs sometimes include "Owner's DJI Air 2S" format.
fn clean_aircraft_model(raw: &str) -> String {
    let trimmed = raw.trim();
    // Strip common prefixes like "Owner's " before the model name
    if let Some(pos) = trimmed.find("DJI ") {
        trimmed[pos..].to_string()
    } else if let Some(pos) = trimmed.find("Mavic") {
        trimmed[pos..].to_string()
    } else {
        trimmed.to_string()
    }
}

/// True when the parser.details aircraft_name carries a DJI default
/// placeholder rather than a real user-set name (e.g. "choose a name
/// for your inspire" on Inspire 1 DJIGO 3.x logs).
fn is_placeholder_aircraft_name(s: &str) -> bool {
    let l = s.trim().to_lowercase();
    l.is_empty() || l.starts_with("choose a name")
}

/// Source of an aircraft name resolved at parse-time, before the
/// import handler's DB / mapping resolution runs. Surfaces as the
/// `model_detection_status` field on the preview when neither
/// from_aircraft_db nor from_mapping later wins.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NameSource {
    Recover,
    Details,
    Unknown,
}

impl NameSource {
    fn as_status(self) -> &'static str {
        match self {
            NameSource::Recover => "from_recover",
            NameSource::Details => "from_details",
            NameSource::Unknown => "unknown",
        }
    }
}

/// Per-field fallback: fills `current` from `details_value` if `current`
/// is `None` and `details_value` (post-trim) is non-empty. `is_invalid`
/// lets callers reject sentinel values (e.g. "0.0.0" for app_version,
/// "None" for the product_type Debug repr) without writing a custom
/// branch each time. Returns true when the fallback fired so callers
/// can emit a tracing line at the call site with field-specific labels.
fn fallback_from_details(
    current: &mut Option<String>,
    details_value: &str,
    is_invalid: impl Fn(&str) -> bool,
) -> bool {
    if current.is_some() { return false; }
    let v = details_value.trim();
    if v.is_empty() || is_invalid(v) { return false; }
    *current = Some(v.to_string());
    true
}

/// Detects file format and dispatches to the correct parser.
pub async fn parse(bytes: &[u8], filename: &str, api_key: Option<&str>) -> Result<FlightPreview, AppError> {
    let lower = filename.to_lowercase();
    if lower.ends_with(".csv") {
        parse_csv(bytes, filename)
    } else if lower.ends_with(".txt") {
        parse_txt(bytes, filename, api_key).await
    } else {
        Err(AppError::BadRequest(format!("Unsupported file format: {}. Use .csv or .txt", filename)))
    }
}

/// Parses a binary DJI .txt flight log using dji-log-parser crate.
/// For version 13+ logs, an API key is required to fetch decryption keychains.
async fn parse_txt(bytes: &[u8], filename: &str, api_key: Option<&str>) -> Result<FlightPreview, AppError> {
    use dji_log_parser::DJILog;

    let parser = DJILog::from_bytes(bytes.to_vec())
        .map_err(|e| AppError::BadRequest(format!("Failed to parse DJI log: {}", e)))?;

    tracing::info!("DJI log version: {:?}", parser.version);

    // Fetch keychains if API key is provided (required for version 13+)
    // Note: fetch_keychains is synchronous (blocking HTTP) — run in spawn_blocking
    let frames = if let Some(key) = api_key.filter(|k| !k.is_empty()) {
        let keychains = parser.fetch_keychains(key)
            .map_err(|e| AppError::BadRequest(format!(
                "Failed to fetch DJI keychains (check API key in Settings): {}", e
            )))?;
        parser.frames(Some(keychains))
            .map_err(|e| AppError::BadRequest(format!("Failed to decode DJI log frames: {}", e)))?
    } else {
        // Try without keychains (works for older log versions)
        parser.frames(None)
            .map_err(|e| AppError::BadRequest(format!(
                "Failed to parse DJI log. For encrypted logs (v13+), add a DJI API key in Settings. Error: {}", e
            )))?
    };

    if frames.is_empty() {
        return Err(AppError::BadRequest("No flight data found in log file".into()));
    }

    // Extract data from frames
    // Frame fields are direct structs (not Option) per dji-log-parser v0.5 API
    let mut trajectory: Vec<TrajectoryPoint> = Vec::new();
    // AGL altitude tracking. `osd.height_max` is the crate's running max
    // (Height Above Ground Level). `osd.height` is the per-frame AGL value
    // we track ourselves as a fallback if `height_max` stays stuck at 0.
    let mut max_height_agl: f32 = 0.0;
    let mut max_height_running: f32 = 0.0;
    let mut max_distance: f32 = 0.0;
    let mut total_distance: f32 = 0.0;
    let mut max_speed: f32 = 0.0;
    let mut min_sats: Option<i32> = None;
    let mut min_batt: Option<i32> = None;
    let mut max_temp: Option<f32> = None;
    let mut max_cell_variance: Option<f32> = None;
    // Authoritative home from FrameHome (first frame with valid WGS84 coords)
    let mut frame_home_lat: Option<f32> = None;
    let mut frame_home_lon: Option<f32> = None;
    // True when we saw at least one FrameHome carrying non-zero coords that
    // failed validity (e.g. unscaled-integer sentinel like 45836624). Used to
    // tag the home_source in tracing so we can spot bad firmwares in logs.
    let mut framehome_rejected: bool = false;
    // Fallback home from first valid GPS point
    let mut gps_home_lat: Option<f32> = None;
    let mut gps_home_lon: Option<f32> = None;
    let mut flight_start: Option<String> = None;
    let mut flight_end: Option<String> = None;
    let mut aircraft_name: Option<String> = None;
    let mut aircraft_serial: Option<String> = None;
    let mut battery_serial: Option<String> = None;
    let mut rc_serial: Option<String> = None;
    let mut app_version: Option<String> = None;
    let mut drone_type_code: Option<String> = None;
    // Tracks which source filled `aircraft_name` so model_detection_status
    // reflects the actual provenance after the recover-loop and the
    // parser.details fallback both run.
    let mut name_source = NameSource::Unknown;
    let mut prev_lat: Option<f64> = None;
    let mut prev_lon: Option<f64> = None;
    let mut photo_count: i32 = 0;
    let mut video_row_count: i32 = 0;
    // Every frame's OSD fly_time (motors-on counter) in seconds, collected
    // so the per-log baseline can be taken as the minimum over MOTORS-ON
    // frames only (see `positive_min`). fly_time counts from aircraft
    // power-on, not from the start of this log file, so the per-flight
    // duration is the SPAN of the counter (max - min). Motors-off frames
    // recorded before the props spin up report a zero counter; including
    // them in the minimum would collapse the span back to the absolute
    // endpoint and defeat the carry-over correction. See
    // `compute_duration_seconds`.
    let mut fly_times: Vec<f64> = Vec::with_capacity(frames.len());

    for frame in &frames {
        // OSD data — GPS position, altitude, speed, satellites, fly_time
        let osd = &frame.osd;

        // Record the fly_time counter (outside the GPS gate — the counter
        // is valid without a GPS fix). The motors-on baseline is taken from
        // these afterwards via `positive_min`.
        fly_times.push(osd.fly_time as f64);

        // Altitude tracking lives OUTSIDE the GPS gate — height is
        // barometer-derived and doesn't require a GPS fix. AGL (Above
        // Ground Level), not ASL: `osd.altitude` would be ASL but is
        // computed by the crate as `raw + frame.home.altitude`, which
        // goes negative when the home-altitude fixup is unstable.
        if osd.height_max > max_height_agl {
            max_height_agl = osd.height_max;
        }
        if osd.height > max_height_running {
            max_height_running = osd.height;
        }

        let lat = osd.latitude;
        let lon = osd.longitude;

        if lat.abs() > 0.0001 && lon.abs() > 0.0001 {
            let t_sec = osd.fly_time as f64;

            // Capture the first valid GPS point as a home-point fallback.
            if gps_home_lat.is_none() {
                gps_home_lat = Some(lat as f32);
                gps_home_lon = Some(lon as f32);
            }

            // Total distance between consecutive points
            if let (Some(plat), Some(plon)) = (prev_lat, prev_lon) {
                total_distance += haversine_m(plat, plon, lat, lon);
            }
            prev_lat = Some(lat);
            prev_lon = Some(lon);

            // Max distance from whichever home point we have so far
            // (prefer the authoritative FrameHome value if set).
            let cur_home = frame_home_lat
                .zip(frame_home_lon)
                .or_else(|| gps_home_lat.zip(gps_home_lon));
            if let Some((hlat, hlon)) = cur_home {
                let dist = haversine_m(hlat as f64, hlon as f64, lat, lon);
                if dist > max_distance { max_distance = dist; }
            }

            // Speed from x/y components
            let speed = ((osd.x_speed as f64).powi(2) + (osd.y_speed as f64).powi(2)).sqrt() as f32;
            if speed > max_speed { max_speed = speed; }

            // Satellites
            let sats = osd.gps_num as i32;
            min_sats = Some(min_sats.map_or(sats, |m: i32| m.min(sats)));

            // Trajectory `alt` is AGL (osd.height) to match max_altitude_m.
            trajectory.push(TrajectoryPoint { lat, lon, alt: osd.height as f64, t_sec });
        }

        // Battery — direct struct, not Option
        let battery = &frame.battery;
        let pct = battery.charge_level as i32;
        if pct > 0 {
            min_batt = Some(min_batt.map_or(pct, |m: i32| m.min(pct)));
        }
        if battery.temperature > 0.0 {
            max_temp = Some(max_temp.map_or(battery.temperature, |m: f32| m.max(battery.temperature)));
        }
        // Cell variance: prefer the crate's running-max field; fall back to
        // per-frame deviation. Units are volts.
        let cv = if battery.max_cell_voltage_deviation > 0.0 {
            battery.max_cell_voltage_deviation
        } else {
            battery.cell_voltage_deviation
        };
        if cv > 0.0 {
            max_cell_variance = Some(max_cell_variance.map_or(cv, |m: f32| m.max(cv)));
        }

        // Home point — authoritative coordinates from FrameHome. Validity is
        // strict: must be real lat/lon degrees, not null-island, not NaN.
        // Some firmwares (observed on a Mavic 3 v14 log) emit unscaled raw
        // integers here, which the loose `abs() > 0.0001` check accepted.
        if frame_home_lat.is_none() {
            let hlat = frame.home.latitude;
            let hlon = frame.home.longitude;
            if is_valid_lat_lon(hlat, hlon) {
                frame_home_lat = Some(hlat as f32);
                frame_home_lon = Some(hlon as f32);
            } else if hlat != 0.0 || hlon != 0.0 {
                // Non-zero but out-of-range — a real attempt at a home value
                // that the firmware encoded incorrectly. Worth flagging.
                framehome_rejected = true;
            }
        }

        // Datetime — FrameCustom has date_time: DateTime<Utc>
        let custom = &frame.custom;
        let dt_str = custom.date_time.format("%Y-%m-%dT%H:%M:%SZ").to_string();
        if !dt_str.starts_with("1970") {
            if flight_start.is_none() { flight_start = Some(dt_str.clone()); }
            flight_end = Some(dt_str);
        }

        // Camera events — direct struct
        let camera = &frame.camera;
        if camera.is_photo { photo_count += 1; }
        if camera.is_video { video_row_count += 1; }

        // Recover info — direct struct, aircraft metadata
        let recover = &frame.recover;
        if aircraft_serial.is_none() && !recover.aircraft_sn.is_empty() {
            aircraft_serial = Some(recover.aircraft_sn.clone());
        }
        if aircraft_name.is_none() && !recover.aircraft_name.is_empty() {
            aircraft_name = Some(clean_aircraft_model(&recover.aircraft_name));
            name_source = NameSource::Recover;
        }
        if battery_serial.is_none() && !recover.battery_sn.is_empty() {
            battery_serial = Some(recover.battery_sn.clone());
        }
        if rc_serial.is_none() && !recover.rc_sn.is_empty() {
            rc_serial = Some(recover.rc_sn.clone());
        }
        if app_version.is_none() && !recover.app_version.is_empty() {
            app_version = Some(recover.app_version.clone());
        }

        // Drone type code from OSD
        if drone_type_code.is_none() {
            let dt = format!("{:?}", osd.drone_type);
            if dt != "None" && !dt.is_empty() {
                drone_type_code = Some(dt);
            }
        }
    }

    if trajectory.is_empty() {
        return Err(AppError::BadRequest(
            "No valid GPS data found in DJI log file.".into()
        ));
    }

    // parser.details fallback: older v9 logs (DJIGO 3.x — Inspire 1, Phantom
    // 3/4, M600 etc.) leave per-frame `recover` empty but populate the
    // top-level Details struct with the same SN + metadata fields. Recover
    // wins when both sources have data (the loop above runs first), so this
    // block is a no-op on v11+ logs that already filled everything via
    // recover. On v9 logs it fills the gaps.
    //
    // NOTE: parser.details.camera_sn is populated on v11 logs (Mavic Pro
    // etc.) but no schema entity exists for cameras. Add extraction here
    // when/if a cameras table is introduced — out of scope for this fallback.
    let det = &parser.details;
    if fallback_from_details(&mut aircraft_serial, &det.aircraft_sn, |_| false) {
        tracing::info!(
            target: "dji_parser",
            source = "details",
            field = "aircraft_serial",
            sn = %aircraft_serial.as_deref().unwrap_or(""),
            "filled from parser.details (recover empty)"
        );
    }
    if fallback_from_details(&mut battery_serial, &det.battery_sn, |_| false) {
        tracing::info!(
            target: "dji_parser",
            source = "details",
            field = "battery_serial",
            sn = %battery_serial.as_deref().unwrap_or(""),
            "filled from parser.details (recover empty)"
        );
    }
    if fallback_from_details(&mut rc_serial, &det.rc_sn, |_| false) {
        tracing::info!(
            target: "dji_parser",
            source = "details",
            field = "rc_serial",
            sn = %rc_serial.as_deref().unwrap_or(""),
            "filled from parser.details (recover empty)"
        );
    }
    if fallback_from_details(&mut aircraft_name, &det.aircraft_name, is_placeholder_aircraft_name) {
        name_source = NameSource::Details;
    }
    // app_version on Details is composed from 3 bytes as "X.Y.Z" — a fully
    // unset value is "0.0.0" rather than empty, so reject that sentinel.
    fallback_from_details(&mut app_version, &det.app_version, |s| s == "0.0.0");
    // product_type fallback for drone_type_code. The existing osd.drone_type
    // path emits the Debug repr of `Option<DroneType>` ("Some(Matrice600)" /
    // "None"), while details.product_type is the unwrapped enum. Wrapping
    // here in `Some(...)` keeps drone_type_mappings keys consistent across
    // the two sources, so a v9 fallback and a later v11 import for the
    // same drone produce the same lookup key. ProductType::None → skip.
    {
        let inner = format!("{:?}", det.product_type);
        if inner != "None" {
            fallback_from_details(
                &mut drone_type_code,
                &format!("Some({})", inner),
                |_| false,
            );
        }
    }

    // Resolve canonical aircraft_models name from drone_type_code (if any).
    // Runs AFTER the parser.details fallback so both osd.drone_type (bare
    // variant) and details.product_type (Some-wrapped) inputs are accepted
    // by canonical_model_name's strip logic.
    let aircraft_model: Option<String> = drone_type_code
        .as_deref()
        .and_then(crate::services::dji_model_mapping::canonical_model_name)
        .map(str::to_owned);

    // Duration = the SPAN of the OSD fly_time counter within THIS log, not
    // its endpoint. fly_time counts from aircraft power-on, so a second
    // flight in the same power cycle starts partway up the counter (see
    // compute_duration_seconds). The baseline is the minimum over MOTORS-ON
    // frames only (positive_min): a motors-off frame reporting a zero
    // counter must not collapse the minimum and degrade the span back to
    // the endpoint. The wall clock is a zero-span fallback only — never a
    // ceiling.
    let min_ft_f = positive_min(&fly_times);
    let max_ft_f = fly_times.iter().copied().fold(0.0_f64, f64::max);
    let min_ft = min_ft_f as i64;
    let max_fly_time = max_ft_f as i64;
    let wall_seconds = wall_clock_seconds(flight_start.as_deref(), flight_end.as_deref());
    let duration_seconds = compute_duration_seconds(min_ft, max_fly_time, wall_seconds);
    if min_ft > 0 {
        // The carry-over case this fix exists for: the counter did not
        // start at zero, so an earlier flight of the power cycle bled in.
        tracing::info!(
            target: "dji_parser",
            filename = %filename,
            min_fly_time = min_ft,
            max_fly_time = max_fly_time,
            duration_seconds = duration_seconds,
            "OSD fly_time carry-over: counter did not start at zero; using max-min span"
        );
    }
    if let Some(w) = wall_seconds {
        if (duration_seconds as i64) > w + 5 {
            // Worth seeing in production traces, but NOT a reason to alter
            // the value — the wall clock is a fallback, not a ceiling.
            tracing::warn!(
                target: "dji_parser",
                filename = %filename,
                duration_seconds = duration_seconds,
                wall_seconds = w,
                "duration exceeds wall-clock span by >5s — kept as-is (sparse/1970 timestamps likely)"
            );
        }
    }

    // Rebase the trajectory time axis. t_sec inherits the same power-on
    // carry-over as the duration counter, so a second flight of a power
    // cycle would otherwise plot as starting at t=min_fly_time (e.g.
    // 437 s). Subtract the motors-on baseline (same positive-only minimum
    // used for the duration) so every altitude/time chart starts at zero;
    // clamp at 0 for safety. Done BEFORE the downsample so both operate on
    // the full-resolution series.
    for p in trajectory.iter_mut() {
        p.t_sec = (p.t_sec - min_ft_f).max(0.0);
    }

    // Downsample trajectory to max 500 points
    let trajectory = if trajectory.len() > 500 {
        let step = trajectory.len() / 500;
        trajectory.into_iter().step_by(step).collect()
    } else {
        trajectory
    };

    // Prefer DJILog.details aggregates for photo / video counts. On most
    // firmwares these are per-flight totals, but we've observed Mavic 3 and
    // Neo v14 logs where video_time clearly exceeds the flight duration
    // (e.g. 9442s on a 469s flight) — which is physically impossible for
    // per-flight video. When the video figure fails the sanity check we
    // also distrust capture_num from the same block and fall back to the
    // frame-flag heuristic for both.
    let details = &parser.details;
    // +2 s tolerance covers rounding between duration (int) and video_time.
    let video_plausible = details.video_time <= (duration_seconds as i64) + 2;
    let (photo_count, video_seconds, counts_source) =
        if (details.capture_num > 0 || details.video_time > 0) && video_plausible {
            (
                details.capture_num,
                details.video_time as i32,
                "FrameDetails",
            )
        } else {
            if !video_plausible {
                tracing::warn!(
                    "FrameDetails video_time={}s exceeds flight duration {}s — falling back to frame-count heuristic",
                    details.video_time, duration_seconds
                );
            }
            (photo_count, (video_row_count as f32 * 0.1) as i32, "frame_count")
        };

    let app_source = app_version.as_deref().map(detect_app_source);

    // Resolve home: FrameHome if valid, else first-GPS-point. If FrameHome
    // carried values that failed the WGS84 sanity check, tag the fallback so
    // the firmware/log combination is easy to spot in production traces.
    let (home_lat, home_lon, home_source) = match (frame_home_lat, frame_home_lon) {
        (Some(la), Some(lo)) => (Some(la), Some(lo), "FrameHome"),
        _ => {
            let tag = if framehome_rejected {
                "gps_first_point (framehome_invalid)"
            } else {
                "gps_first_point"
            };
            (gps_home_lat, gps_home_lon, tag)
        }
    };

    // Model detection: at the parser stage we know whether the name came
    // from the per-frame recover block ("from_recover"), the top-level
    // parser.details fallback ("from_details"), or neither ("unknown").
    // Upgraded to "from_aircraft_db" or "from_mapping" by the import
    // handler if those higher-trust sources match first.
    let model_detection_status = Some(name_source.as_status().to_string());

    // Layered AGL altitude fallback. Primary: crate-maintained running max
    // (`osd.height_max`). Fallback 1: per-frame `osd.height` running max we
    // track ourselves. Fallback 2: top-level `parser.details.max_height`
    // (single value for the whole flight). If all three are 0 we emit a
    // warn — earlier the silent zero hid the bug on ~62% of dev imports.
    let (final_max_altitude, altitude_source) = if max_height_agl > 0.0 {
        (max_height_agl, "osd.height_max")
    } else if max_height_running > 0.0 {
        (max_height_running, "osd.height (running max)")
    } else if details.max_height > 0.0 {
        (details.max_height, "details.max_height")
    } else {
        tracing::warn!(
            filename = %filename,
            "max altitude extraction yielded 0 from height_max, height running max, and details.max_height — possible GPS/baro anomaly or unsupported log format"
        );
        (0.0, "none (all-zero)")
    };

    tracing::info!(
        "DJI binary parser: home_source={} counts_source={} altitude_source={} cell_variance_v={:?} photo_count={} video_seconds={}",
        home_source, counts_source, altitude_source, max_cell_variance, photo_count, video_seconds
    );

    Ok(FlightPreview {
        source_filename: filename.to_string(),
        flight_guid: None,
        aircraft_name,
        aircraft_model,
        aircraft_serial,
        battery_serial,
        rc_serial,
        app_version,
        app_source,
        drone_type_code,
        flight_start,
        flight_end,
        duration_seconds,
        max_altitude_m: final_max_altitude,
        max_distance_m: max_distance,
        total_distance_m: total_distance,
        max_speed_ms: if max_speed > 0.0 { Some(max_speed) } else { None },
        home_lat,
        home_lon,
        satellite_count_min: min_sats,
        battery_temp_c: max_temp,
        cell_variance_v: max_cell_variance,
        min_battery_pct: min_batt,
        photo_count,
        video_seconds,
        trajectory,
        model_detection_status,
        aircraft_match: None,
        battery_match: None,
        controller_match: None,
        findings: Vec::new(),
    })
}

/// True if (lat, lon) look like valid WGS84 degrees — within Earth bounds,
/// not both zero (null island), and not NaN.
fn is_valid_lat_lon(lat: f64, lon: f64) -> bool {
    !lat.is_nan()
        && !lon.is_nan()
        && lat.abs() <= 90.0
        && lon.abs() <= 180.0
        && (lat != 0.0 || lon != 0.0)
}

/// Combines a date string and a time-of-day string into a UTC ISO-8601
/// timestamp (e.g. `2026-04-12T10:04:45.740Z`). Returns `None` if either
/// input fails to parse under any of the supported formats.
///
/// Timezone handling: the input is parsed as naive wall-clock and tagged
/// as UTC without offset adjustment. DJI Fly's `CUSTOM.date [local]` +
/// `CUSTOM.updateTime [local]` pair is in the exporter's local time, but
/// the log carries no offset and we have no per-org timezone setting —
/// tagging UTC keeps chronological ordering consistent across imports.
///
/// Date format ambiguity: both `m/d/Y` and `d/m/Y` are accepted; `m/d/Y`
/// is tried first since that matches DJI Fly's default (US/English)
/// locale. European exports using `d/m/Y` will work when the day is > 12
/// (since `m/d/Y` rejects month > 12) but can be ambiguous when both
/// fields are ≤ 12 — no reliable way to disambiguate without more info.
fn compose_datetime_to_utc_iso(date_s: &str, time_s: &str) -> Option<String> {
    use chrono::{NaiveDate, NaiveDateTime, NaiveTime};

    let date = NaiveDate::parse_from_str(date_s, "%Y-%m-%d")
        .or_else(|_| NaiveDate::parse_from_str(date_s, "%m/%d/%Y"))
        .or_else(|_| NaiveDate::parse_from_str(date_s, "%d/%m/%Y"))
        .or_else(|_| NaiveDate::parse_from_str(date_s, "%Y/%m/%d"))
        .ok()?;

    let time = NaiveTime::parse_from_str(time_s, "%I:%M:%S%.f %p")
        .or_else(|_| NaiveTime::parse_from_str(time_s, "%I:%M:%S %p"))
        .or_else(|_| NaiveTime::parse_from_str(time_s, "%H:%M:%S%.f"))
        .or_else(|_| NaiveTime::parse_from_str(time_s, "%H:%M:%S"))
        .ok()?;

    let dt = NaiveDateTime::new(date, time);
    Some(format!("{}Z", dt.format("%Y-%m-%dT%H:%M:%S%.3f")))
}

/// Parses a flight timestamp in either ISO form this module writes —
/// `%Y-%m-%dT%H:%M:%SZ` (the binary path) or `%Y-%m-%dT%H:%M:%S%.3fZ`
/// (the CSV path via `compose_datetime_to_utc_iso`) — and returns the
/// non-negative wall-clock span, in whole seconds, between `start` and
/// `end`. Returns `None` if either side is missing or unparseable, or if
/// `end` precedes `start`.
///
/// It tolerates: an optional fractional-seconds part, an optional trailing
/// `Z`, and a space separator in place of `T` (some CSV exports carry a
/// pre-combined `YYYY-MM-DD HH:MM:SS` datetime column through verbatim).
///
/// IMPORTANT: this value is only ever used as a *fallback* duration when
/// the fly_time counter yields a zero span — never as a ceiling. See
/// `compute_duration_seconds`.
fn wall_clock_seconds(start: Option<&str>, end: Option<&str>) -> Option<i64> {
    use chrono::NaiveDateTime;

    fn parse_one(s: &str) -> Option<NaiveDateTime> {
        // Strip the trailing UTC marker, then try the fractional and
        // whole-second variants for both the `T` and space separators.
        let t = s.trim().trim_end_matches(['Z', 'z']);
        NaiveDateTime::parse_from_str(t, "%Y-%m-%dT%H:%M:%S%.f")
            .or_else(|_| NaiveDateTime::parse_from_str(t, "%Y-%m-%dT%H:%M:%S"))
            .or_else(|_| NaiveDateTime::parse_from_str(t, "%Y-%m-%d %H:%M:%S%.f"))
            .or_else(|_| NaiveDateTime::parse_from_str(t, "%Y-%m-%d %H:%M:%S"))
            .ok()
    }

    let s = parse_one(start?)?;
    let e = parse_one(end?)?;
    let diff = (e - s).num_seconds();
    if diff < 0 { None } else { Some(diff) }
}

/// Computes a flight's duration in seconds from the minimum and maximum of
/// the OSD `fly_time` counter seen in a single log file, with a wall-clock
/// fallback.
///
/// DJI's `OSD.flyTime` is a motors-on counter measured from aircraft
/// POWER-ON, not from the start of the individual log file. If a pilot
/// lands and takes off again without cutting power (and the app opens a
/// fresh log), that new log begins with `fly_time` already equal to the
/// airborne time accumulated so far in the power cycle. Taking the last
/// frame's absolute value therefore sums every earlier flight of the power
/// cycle into this one. The correct per-flight duration is the SPAN of the
/// counter within this log — `max - min` — not its endpoint.
///
/// The wall-clock span is a FALLBACK only, used when the counter span is
/// zero (single-frame or flat-counter logs). It is deliberately NOT used
/// as a ceiling: some logs carry valid timestamps on only a handful of
/// frames (the rest decode as 1970), giving a wall span of a few seconds
/// for a genuine multi-minute flight. Clamping the real counter delta down
/// to that bogus wall span would discard good data. Do NOT "tighten" this
/// into a clamp later.
fn compute_duration_seconds(min_fly_time: i64, max_fly_time: i64, wall_seconds: Option<i64>) -> i32 {
    let delta = (max_fly_time - min_fly_time).max(0);
    if delta > 0 {
        return delta as i32;
    }
    match wall_seconds {
        Some(w) if w > 0 => w as i32,
        _ => 0,
    }
}

/// Below this threshold a fly_time / t_sec reading counts as "motors off".
/// fly_time is f32 seconds decoded from decisecond ticks, so the smallest
/// genuine motors-on value is ~0.1 s; 0.05 sits safely under that and above
/// any float noise around a true zero.
const FLY_TIME_EPSILON: f64 = 0.05;

/// The smallest MOTORS-ON value in `values` — the minimum taken over entries
/// greater than `FLY_TIME_EPSILON`. Returns 0.0 when no entry clears the
/// threshold.
///
/// This is the baseline for the carry-over duration correction. fly_time is
/// a motors-on counter measured from aircraft POWER-ON, and a log recorded
/// before the props spin up contains frames whose counter still reads zero.
/// A naive minimum over ALL frames would collapse to one of those zeros, and
/// the span (`max - min`) would silently degrade back to the absolute
/// endpoint — reintroducing the exact carry-over bug the span exists to
/// remove. Excluding sub-epsilon values keeps the baseline anchored to the
/// first airborne reading. When nothing is airborne (all zeros) the baseline
/// is 0.0, which leaves the endpoint-as-span behaviour unchanged.
fn positive_min(values: &[f64]) -> f64 {
    values
        .iter()
        .copied()
        .filter(|v| *v > FLY_TIME_EPSILON)
        .fold(None, |acc: Option<f64>, v| Some(acc.map_or(v, |m: f64| m.min(v))))
        .unwrap_or(0.0)
}

/// Haversine distance in meters between two lat/lon points.
fn haversine_m(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f32 {
    let r = 6_371_000.0_f64;
    let dlat = (lat2 - lat1).to_radians();
    let dlon = (lon2 - lon1).to_radians();
    let a = (dlat / 2.0).sin().powi(2)
        + lat1.to_radians().cos() * lat2.to_radians().cos()
        * (dlon / 2.0).sin().powi(2);
    let c = 2.0 * a.sqrt().atan2((1.0 - a).sqrt());
    (r * c) as f32
}

/// True if the lowercased header name contains a common unit marker in
/// brackets or parens — `[s]`, `(m)`, `[mph]`, etc. Used as a tiebreaker
/// when multiple columns match a pattern and more than one parses as
/// numeric (e.g. DJI Fly emits both `OSD.flyTime` and `OSD.flyTime [s]`).
fn has_unit_marker(name: &str) -> bool {
    const MARKERS: &[&str] = &[
        "[s]", "(s)", "[m]", "(m)", "[v]", "(v)", "[a]", "(a)",
        "[mph]", "(mph)", "[ft]", "(ft)", "[mah]", "(mah)",
        "[°c]", "(°c)", "[deg]", "(deg)", "[%]", "(%)",
    ];
    MARKERS.iter().any(|m| name.contains(m))
}

/// Finds the best matching column index from a priority-ordered list of
/// patterns. Returns `(index, lowercased_header_name)`.
///
/// When `sample` (the first data row) is provided, candidates within a
/// pattern are re-ranked: columns whose first-row value parses as f64 win
/// over text columns. If multiple numeric columns match, a header unit
/// marker like `[s]` tiebreaks. If no candidate parses as numeric on the
/// sample row (typical for string columns like aircraft_name), the first
/// header-order match wins — preserving the original behavior so string
/// matches are unaffected.
fn find_col(
    headers: &csv::StringRecord,
    patterns: &[&str],
    sample: Option<&csv::StringRecord>,
) -> Option<(usize, String)> {
    for pattern in patterns {
        let candidates: Vec<(usize, String)> = headers.iter()
            .enumerate()
            .filter_map(|(i, h)| {
                let l = h.to_lowercase();
                if l.contains(pattern) { Some((i, l)) } else { None }
            })
            .collect();
        if candidates.is_empty() { continue; }

        // Only override header order when a candidate is unambiguously
        // numeric *and* carries an explicit unit marker like `[s]` or `(m)`.
        // A bare numeric value isn't enough — serial numbers such as
        // "5380000000001" also parse as f64, and preferring them over the
        // header-order first match (e.g. `FlyControllerSerialNumber`) would
        // break pattern-based disambiguation for string-typed columns.
        if let Some(row) = sample {
            let is_num = |idx: usize| row.get(idx)
                .and_then(|v| v.trim().parse::<f64>().ok())
                .is_some();
            if let Some(c) = candidates.iter().find(|(i, name)| is_num(*i) && has_unit_marker(name)) {
                return Some(c.clone());
            }
        }
        // Default: first header-order match wins — preserves the original
        // behavior for string columns (serials, names) and for sensor columns
        // that only appear once in the CSV.
        return candidates.into_iter().next();
    }
    None
}

/// Parses a DJI CSV export file.
/// Supports DJI Fly app exports (osd.*, battery.*, recover.*, custom.*),
/// DJI GO 4 exports, AirData exports, Litchi exports, and generic CSV formats.
fn parse_csv(bytes: &[u8], filename: &str) -> Result<FlightPreview, AppError> {
    let content = std::str::from_utf8(bytes)
        .map_err(|_| AppError::BadRequest("File is not valid UTF-8 text".into()))?;

    // Strip an optional leading `sep=,` directive that DJI Fly's CSV export
    // (and Excel-compatible CSVs generally) emit on the first line.
    let content_stripped = match content.strip_prefix("sep=") {
        Some(rest) => match rest.find('\n') {
            Some(idx) => &rest[idx + 1..],
            None => rest,
        },
        None => content,
    };

    let mut reader = csv::ReaderBuilder::new()
        .flexible(true)
        .trim(csv::Trim::All)
        .from_reader(content_stripped.as_bytes());

    let headers = reader.headers()
        .map_err(|e| AppError::BadRequest(format!("Cannot read CSV headers: {}", e)))?
        .clone();

    // Peek the first data row up-front. `find_col` uses it to disambiguate
    // between text and numeric columns that share a substring — e.g. DJI Fly
    // emits both `OSD.flyTime` ("0m 0.0s", text) and `OSD.flyTime [s]`
    // ("0.0", numeric). The row is threaded back into the main iteration
    // after column detection completes.
    let mut records_iter = reader.into_records();
    let first_record: Option<csv::StringRecord> = match records_iter.next() {
        Some(Ok(r)) => Some(r),
        Some(Err(e)) => return Err(AppError::BadRequest(format!("CSV parse error: {}", e))),
        None => None,
    };
    let sample = first_record.as_ref();

    // --- Column detection (priority order: most specific DJI names first) ---

    let col_lat = find_col(&headers, &[
        "osd.latitude", "latitude", "gps:lat", "lat",
    ], sample).map(|(i, _)| i);

    let col_lon = find_col(&headers, &[
        "osd.longitude", "longitude", "gps:lon", "lon", "lng",
    ], sample).map(|(i, _)| i);

    // Altitude + unit: detect feet vs meters from the matched header name.
    // Priority keeps DJI-native column names first.
    let (col_alt, alt_unit_factor) = match find_col(&headers, &[
        "osd.altitude", "altitude(m)", "altitude", "osd.height", "height", "alt",
    ], sample) {
        Some((i, name)) => {
            let is_ft = name.contains("(ft)") || name.contains("[ft]");
            if is_ft {
                tracing::info!("CSV altitude column '{}' uses feet; applying ft→m conversion", name);
            }
            (Some(i), if is_ft { 0.3048_f64 } else { 1.0 })
        }
        None => (None, 1.0),
    };

    // Speed + unit: detect mph vs m/s from the matched header name.
    let (col_speed, speed_unit_factor) = match find_col(&headers, &[
        "osd.xspeed", "speed(m/s)", "speed(mps)", "speed", "groundspeed",
    ], sample) {
        Some((i, name)) => {
            let is_mph = name.contains("(mph)") || name.contains("[mph]");
            if is_mph {
                tracing::info!("CSV speed column '{}' uses mph; applying mph→m/s conversion", name);
            }
            (Some(i), if is_mph { 0.44704_f64 } else { 1.0 })
        }
        None => (None, 1.0),
    };

    let col_sats = find_col(&headers, &[
        "osd.gpsnum", "satellites", "satellite", "gps_satellites", "gpsnum",
    ], sample).map(|(i, _)| i);

    // Time — detect units from matched column name. `find_col` with `sample`
    // prefers the numeric `OSD.flyTime [s]` over the text `OSD.flyTime`.
    let col_time_match = find_col(&headers, &[
        "osd.flytime", "flytime(s)", "flytime",
        "time(millisecond)", "time_ms", "timems",
        "time(s)", "time",
    ], sample);
    if let Some((_, ref name)) = col_time_match {
        tracing::info!("CSV time column selected: '{}'", name);
    }
    let (col_time, time_is_ms) = match col_time_match {
        Some((i, name)) => {
            let is_ms = name.contains("millisecond") || name.contains("time_ms") || name.contains("timems");
            (Some(i), is_ms)
        }
        None => (None, false),
    };

    let col_temp = find_col(&headers, &[
        "battery.temperature", "battery.maxtemperature",
        "temperature(°c)", "temperature", "temp",
    ], sample).map(|(i, _)| i);

    let col_batt = find_col(&headers, &[
        "battery.chargelevel", "battery(%)", "battery_percent",
        "chargelevel", "remainpowerpercent", "remainingpercent",
    ], sample).map(|(i, _)| i);

    // Cell voltage deviation — units are volts in every DJI/Litchi export
    // we've seen. Matching order keeps the "max" variant first so it wins
    // when both columns are present in the same file.
    let col_cell_variance = find_col(&headers, &[
        "maxcellvoltagedeviation", "cellvoltagedeviation",
    ], sample).map(|(i, _)| i);

    // Metadata columns (string values)
    // NOTE: `custom.updatetime` contains "datetime" as a substring, so we
    // reject that match explicitly — it is a per-row time-of-day, not a full
    // timestamp, and downstream code casts flight_start to timestamptz.
    // (Defense in depth: even when the split-datetime composer below kicks
    // in, we still don't want `updatetime` to sneak into the combined
    // matcher.)
    let col_datetime = find_col(&headers, &[
        "custom.datetime", "datetime", "timestamp", "date/time",
    ], sample)
    .filter(|(_, name)| !name.contains("updatetime"))
    .map(|(i, _)| i);

    // Split date + time columns (DJI Fly export pattern:
    // `CUSTOM.date [local]` + `CUSTOM.updateTime [local]`). Only probed
    // when no combined datetime column matched above.
    //
    // Timezone assumption: the headers are tagged `[local]` but the log
    // carries no explicit timezone or UTC-offset field, and there is no
    // per-org timezone setting configured. We therefore parse as naive
    // wall-clock time and **tag as UTC** — this keeps deduplication and
    // chronological ordering consistent between imports, but the absolute
    // timestamp may be off by the pilot's UTC offset. A `tracing::warn!`
    // on parse makes this visible in production traces.
    let (col_date_part, col_time_part) = if col_datetime.is_none() {
        let date_col = headers.iter().enumerate().find(|(_, h)| {
            let l = h.to_lowercase();
            (l.contains("custom.date") || l.starts_with("date") || l == "date")
                && !l.contains("datetime")
                && !l.contains("updatetime")
        }).map(|(i, _)| i);
        let time_col = headers.iter().enumerate().find(|(_, h)| {
            let l = h.to_lowercase();
            l.contains("custom.updatetime") || l.contains("updatetime")
        }).map(|(i, _)| i);
        if let (Some(d), Some(t)) = (date_col, time_col) {
            tracing::warn!(
                "CSV composing flight_start from split columns '{}' + '{}'; assuming UTC (no per-org timezone configured)",
                headers.get(d).unwrap_or(""), headers.get(t).unwrap_or("")
            );
        }
        (date_col, time_col)
    } else {
        (None, None)
    };

    let col_aircraft_name = find_col(&headers, &[
        "recover.aircraftname", "aircraft_name", "aircraftname", "drone_name", "planename",
    ], sample).map(|(i, _)| i);

    let col_aircraft_sn = find_col(&headers, &[
        "recover.aircraftsn", "aircraft_sn", "aircraftsn", "drone_sn", "serial", "flycontrollerserialnumber",
    ], sample).map(|(i, _)| i);

    let col_battery_sn = find_col(&headers, &[
        "recover.batterysn", "batterysn", "battery_sn", "batteryserialnum", "center_battery.serialno",
    ], sample).map(|(i, _)| i);

    let col_rc_sn = find_col(&headers, &[
        "recover.rcsn", "rcsn", "rc_sn", "remoteserialnumber",
    ], sample).map(|(i, _)| i);

    let col_app_version = find_col(&headers, &[
        "recover.appversion", "appversion", "app_version",
    ], sample).map(|(i, _)| i);

    let col_drone_type = find_col(&headers, &[
        "osd.dronetype", "dronetype", "drone_type",
    ], sample).map(|(i, _)| i);

    let col_is_photo = find_col(&headers, &[
        "camera.isphoto", "isphoto",
    ], sample).map(|(i, _)| i);

    let col_is_video = find_col(&headers, &[
        "camera.isvideo", "isvideo",
    ], sample).map(|(i, _)| i);

    // --- Row-by-row parsing ---

    let get_str = |record: &csv::StringRecord, col: Option<usize>| -> Option<String> {
        col.and_then(|i| record.get(i))
           .map(|v| v.trim().to_string())
           .filter(|v| !v.is_empty())
    };

    let mut trajectory: Vec<TrajectoryPoint> = Vec::new();
    let mut max_altitude: f32 = 0.0;
    let mut max_distance: f32 = 0.0;
    let mut total_distance: f32 = 0.0;
    let mut prev_lat: Option<f64> = None;
    let mut prev_lon: Option<f64> = None;
    let mut max_speed: f32 = 0.0;
    let mut min_sats: Option<i32> = None;
    let mut min_batt: Option<i32> = None;
    let mut max_temp: Option<f32> = None;
    let mut max_cell_variance: Option<f32> = None;
    let mut home_lat: Option<f32> = None;
    let mut home_lon: Option<f32> = None;
    // Per-row time axis for every accepted row, collected so the baseline
    // can be taken as the minimum over MOTORS-ON rows only (see
    // `positive_min`). Duration is the SPAN (max - min), not the endpoint —
    // the same power-on carry-over reasoning as the binary path. A
    // motors-off row reporting a zero time counter must not collapse the
    // minimum. See `compute_duration_seconds`.
    let mut t_secs: Vec<f64> = Vec::new();
    let mut aircraft_name: Option<String> = None;
    let mut aircraft_serial: Option<String> = None;
    let mut battery_serial: Option<String> = None;
    let mut rc_serial: Option<String> = None;
    let mut app_version: Option<String> = None;
    let mut drone_type_code: Option<String> = None;
    let mut photo_count: i32 = 0;
    let mut video_row_count: i32 = 0;
    let mut flight_start: Option<String> = None;
    let mut flight_end: Option<String> = None;
    let mut row_count: u32 = 0;

    // Chain the peeked first row back in front of the remaining iterator.
    let chained = first_record.into_iter().map(Ok).chain(records_iter);
    for result in chained {
        let record = result.map_err(|e| AppError::BadRequest(format!("CSV parse error: {}", e)))?;

        let get_f64 = |col: Option<usize>| -> Option<f64> {
            col.and_then(|i| record.get(i))
               .and_then(|v| v.trim().parse::<f64>().ok())
        };

        // Skip rows with missing or near-zero lat/lon (catches text rows like "EngineStart")
        let lat = match get_f64(col_lat) {
            Some(v) if v.abs() > 0.0001 => v,
            _ => continue,
        };
        let lon = match get_f64(col_lon) {
            Some(v) if v.abs() > 0.0001 => v,
            _ => continue,
        };
        let alt = get_f64(col_alt).unwrap_or(0.0) * alt_unit_factor;

        // Time: convert to seconds based on detected unit
        let raw_time = get_f64(col_time).unwrap_or(0.0);
        let t_sec = if time_is_ms { raw_time / 1000.0 } else { raw_time };

        // First valid GPS row sets home position
        if row_count == 0 {
            home_lat = Some(lat as f32);
            home_lon = Some(lon as f32);
        }

        // Metadata — capture first non-empty, update last for datetime
        if aircraft_name.is_none() {
            aircraft_name = get_str(&record, col_aircraft_name);
        }
        if aircraft_serial.is_none() {
            aircraft_serial = get_str(&record, col_aircraft_sn);
        }
        // Datetime handling: prefer a pre-combined datetime column; fall back
        // to composing from split date + time columns (DJI Fly pattern).
        let dt_value: Option<String> = if col_datetime.is_some() {
            get_str(&record, col_datetime)
        } else if let (Some(d_idx), Some(t_idx)) = (col_date_part, col_time_part) {
            let ds = record.get(d_idx).map(|s| s.trim()).unwrap_or("");
            let ts = record.get(t_idx).map(|s| s.trim()).unwrap_or("");
            if !ds.is_empty() && !ts.is_empty() {
                compose_datetime_to_utc_iso(ds, ts)
            } else {
                None
            }
        } else {
            None
        };
        if let Some(dt) = dt_value {
            if !dt.starts_with("1970") {
                if flight_start.is_none() {
                    flight_start = Some(dt.clone());
                }
                flight_end = Some(dt);
            }
        }
        if battery_serial.is_none() {
            battery_serial = get_str(&record, col_battery_sn);
        }
        if rc_serial.is_none() {
            rc_serial = get_str(&record, col_rc_sn);
        }
        if app_version.is_none() {
            app_version = get_str(&record, col_app_version);
        }
        if drone_type_code.is_none() {
            drone_type_code = get_str(&record, col_drone_type);
        }
        // Photo/video counting
        if let Some(v) = get_str(&record, col_is_photo) {
            if v == "true" || v == "1" { photo_count += 1; }
        }
        if let Some(v) = get_str(&record, col_is_video) {
            if v == "true" || v == "1" { video_row_count += 1; }
        }

        trajectory.push(TrajectoryPoint { lat, lon, alt, t_sec });

        // Cumulative distance between consecutive points
        if let (Some(plat), Some(plon)) = (prev_lat, prev_lon) {
            total_distance += haversine_m(plat, plon, lat, lon);
        }
        prev_lat = Some(lat);
        prev_lon = Some(lon);

        // Stats aggregation
        if alt as f32 > max_altitude { max_altitude = alt as f32; }
        if let (Some(hlat), Some(hlon)) = (home_lat, home_lon) {
            let dist = haversine_m(hlat as f64, hlon as f64, lat, lon);
            if dist > max_distance { max_distance = dist; }
        }
        if let Some(s) = get_f64(col_speed) {
            let s_abs = (s.abs() * speed_unit_factor) as f32;
            if s_abs > max_speed { max_speed = s_abs; }
        }
        if let Some(sa) = get_f64(col_sats) {
            let sa_i = sa as i32;
            min_sats = Some(min_sats.map_or(sa_i, |m: i32| m.min(sa_i)));
        }
        if let Some(b) = get_f64(col_batt) {
            let b_i = b as i32;
            min_batt = Some(min_batt.map_or(b_i, |m: i32| m.min(b_i)));
        }
        if let Some(t) = get_f64(col_temp) {
            max_temp = Some(max_temp.map_or(t as f32, |m: f32| m.max(t as f32)));
        }
        if let Some(cv) = get_f64(col_cell_variance) {
            if cv > 0.0 {
                let cv32 = cv as f32;
                max_cell_variance = Some(max_cell_variance.map_or(cv32, |m: f32| m.max(cv32)));
            }
        }

        t_secs.push(t_sec);
        row_count += 1;
    }

    if trajectory.is_empty() {
        return Err(AppError::BadRequest(
            "No valid GPS data found in CSV. Make sure the file contains latitude/longitude columns.".into()
        ));
    }

    // Rebase the trajectory time axis by the per-log MOTORS-ON minimum
    // before downsampling — same power-on carry-over reasoning as the
    // binary path. Taking the baseline over positive values only stops a
    // motors-off row (zero time counter) collapsing the minimum and
    // degrading the span back to the endpoint. Clamp at 0 for safety.
    let min_ts = positive_min(&t_secs);
    let max_ts = t_secs.iter().copied().fold(0.0_f64, f64::max);
    for p in trajectory.iter_mut() {
        p.t_sec = (p.t_sec - min_ts).max(0.0);
    }

    // Downsample trajectory to max 500 points for preview
    let trajectory = if trajectory.len() > 500 {
        let step = trajectory.len() / 500;
        trajectory.into_iter().step_by(step).collect()
    } else {
        trajectory
    };

    // Duration = the SPAN of the per-row time axis (max - min), not its
    // endpoint. The datetime difference (via wall_clock_seconds) is the
    // zero-span fallback — the same behaviour the old inline parse gave,
    // now expressed through the wall_seconds argument — and is never a
    // ceiling.
    let wall_seconds = wall_clock_seconds(flight_start.as_deref(), flight_end.as_deref());
    let duration_seconds = compute_duration_seconds(min_ts as i64, max_ts as i64, wall_seconds);
    if min_ts > 0.0 {
        tracing::info!(
            target: "dji_parser",
            filename = %filename,
            min_time_sec = min_ts,
            max_time_sec = max_ts,
            duration_seconds = duration_seconds,
            "CSV time-axis carry-over: axis did not start at zero; using max-min span"
        );
    }
    if let Some(w) = wall_seconds {
        if (duration_seconds as i64) > w + 5 {
            tracing::warn!(
                target: "dji_parser",
                filename = %filename,
                duration_seconds = duration_seconds,
                wall_seconds = w,
                "duration exceeds wall-clock span by >5s — kept as-is (sparse/1970 timestamps likely)"
            );
        }
    }

    let app_source = app_version.as_deref().map(detect_app_source);

    let model_detection_status = if aircraft_name.is_some() {
        Some("from_recover".to_string())
    } else {
        Some("unknown".to_string())
    };

    // Canonical aircraft_models name from drone_type_code, when the CSV
    // carried an osd.drone_type column. Matches the binary-path logic.
    let aircraft_model: Option<String> = drone_type_code
        .as_deref()
        .and_then(crate::services::dji_model_mapping::canonical_model_name)
        .map(str::to_owned);

    tracing::info!(
        "CSV parser: app_source={:?} alt_factor={} speed_factor={} cell_variance_v={:?}",
        app_source, alt_unit_factor, speed_unit_factor, max_cell_variance
    );

    Ok(FlightPreview {
        source_filename: filename.to_string(),
        flight_guid: None,
        aircraft_name,
        aircraft_model,
        aircraft_serial,
        flight_start,
        flight_end,
        duration_seconds,
        max_altitude_m: max_altitude,
        max_distance_m: max_distance,
        total_distance_m: total_distance,
        max_speed_ms: if max_speed > 0.0 { Some(max_speed) } else { None },
        home_lat,
        home_lon,
        satellite_count_min: min_sats,
        battery_temp_c: max_temp,
        cell_variance_v: max_cell_variance,
        min_battery_pct: min_batt,
        battery_serial,
        rc_serial,
        app_version,
        app_source,
        drone_type_code,
        photo_count,
        video_seconds: (video_row_count as f32 * 0.1) as i32,
        trajectory,
        model_detection_status,
        aircraft_match: None,
        battery_match: None,
        controller_match: None,
        findings: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_fills_when_current_is_none_and_value_non_empty() {
        let mut current: Option<String> = None;
        let fired = fallback_from_details(&mut current, "0670000002", |_| false);
        assert!(fired);
        assert_eq!(current.as_deref(), Some("0670000002"));
    }

    #[test]
    fn fallback_skips_when_current_is_some() {
        let mut current = Some("recover-sn".to_string());
        let fired = fallback_from_details(&mut current, "details-sn", |_| false);
        assert!(!fired);
        assert_eq!(current.as_deref(), Some("recover-sn"));
    }

    #[test]
    fn fallback_skips_when_value_is_empty_or_whitespace() {
        let mut current: Option<String> = None;
        assert!(!fallback_from_details(&mut current, "", |_| false));
        assert!(current.is_none());
        assert!(!fallback_from_details(&mut current, "   ", |_| false));
        assert!(current.is_none());
    }

    #[test]
    fn fallback_skips_when_is_invalid_returns_true() {
        let mut current: Option<String> = None;
        let fired = fallback_from_details(&mut current, "0.0.0", |s| s == "0.0.0");
        assert!(!fired);
        assert!(current.is_none());
    }

    #[test]
    fn fallback_trims_whitespace_before_storing() {
        let mut current: Option<String> = None;
        fallback_from_details(&mut current, "  M61TEST0000041  ", |_| false);
        assert_eq!(current.as_deref(), Some("M61TEST0000041"));
    }

    #[test]
    fn placeholder_aircraft_name_recognised() {
        assert!(is_placeholder_aircraft_name(""));
        assert!(is_placeholder_aircraft_name("   "));
        assert!(is_placeholder_aircraft_name("choose a name for your inspire"));
        assert!(is_placeholder_aircraft_name("Choose a name for your Mavic"));
        assert!(!is_placeholder_aircraft_name("Mavic Pro"));
        assert!(!is_placeholder_aircraft_name("DJI Inspire 1"));
    }

    #[test]
    fn placeholder_aircraft_name_rejected_in_fallback() {
        let mut name: Option<String> = None;
        let fired = fallback_from_details(
            &mut name,
            "choose a name for your inspire",
            is_placeholder_aircraft_name,
        );
        assert!(!fired);
        assert!(name.is_none());
    }

    #[test]
    fn name_source_status_strings_match_consumer_contract() {
        // Values consumed by handlers/flights/import.rs step 3 — keep in sync.
        assert_eq!(NameSource::Recover.as_status(), "from_recover");
        assert_eq!(NameSource::Details.as_status(), "from_details");
        assert_eq!(NameSource::Unknown.as_status(), "unknown");
    }

    #[test]
    fn v9_scenario_fills_all_three_sns_from_details() {
        // Simulates a v9 log where per-frame recover yielded nothing, and
        // parser.details has the full SN set. Fixture values copied from
        // the 17-41-17.txt diagnostic.
        let mut aircraft: Option<String> = None;
        let mut battery: Option<String> = None;
        let mut rc: Option<String> = None;
        assert!(fallback_from_details(&mut aircraft, "0670000002", |_| false));
        assert!(fallback_from_details(&mut battery, "M61TEST0000041", |_| false));
        assert!(fallback_from_details(&mut rc, "03L0000003", |_| false));
        assert_eq!(aircraft.as_deref(), Some("0670000002"));
        assert_eq!(battery.as_deref(), Some("M61TEST0000041"));
        assert_eq!(rc.as_deref(), Some("03L0000003"));
    }

    // ------------------------------------------------------------------
    // Duration: OSD fly_time is a power-on counter, so per-flight duration
    // is the span (max - min), not the endpoint. Fixtures use real DEV
    // figures. The wall clock is a fallback, never a ceiling.
    // ------------------------------------------------------------------

    #[test]
    fn duration_carry_over_uses_span_not_endpoint() {
        // DEV flight 188: second flight of a power cycle, counter 437 → 846.
        // Duration is the 409 s span, not the 846 endpoint.
        assert_eq!(compute_duration_seconds(437, 846, Some(410)), 409);
    }

    #[test]
    fn duration_fresh_power_cycle() {
        // Counter starts near zero on the first flight of a power cycle.
        assert_eq!(compute_duration_seconds(0, 409, Some(410)), 409);
    }

    #[test]
    fn duration_chained_m600_flight() {
        // DEV flight 84 read 347 total; the real span is 162 → 347 = 185 s.
        assert_eq!(compute_duration_seconds(162, 347, Some(184)), 185);
    }

    #[test]
    fn duration_wall_clock_is_not_a_ceiling() {
        // DEV flights 189/190: only a few frames carry real timestamps (the
        // rest decode as 1970), giving a bogus 4 s wall span for a 437 s
        // flight. The counter delta must win — the wall clock is a fallback,
        // never a ceiling.
        assert_eq!(compute_duration_seconds(0, 437, Some(4)), 437);
    }

    #[test]
    fn duration_zero_delta_falls_back_to_wall_clock() {
        assert_eq!(compute_duration_seconds(0, 0, Some(120)), 120);
    }

    #[test]
    fn duration_zero_delta_and_no_wall_clock_is_zero() {
        assert_eq!(compute_duration_seconds(0, 0, None), 0);
    }

    // ------------------------------------------------------------------
    // positive_min: the baseline for the span must ignore motors-off
    // frames (a zero counter recorded before the props spin up). If one
    // of those zeros were allowed to set the minimum in a carry-over log,
    // the span would collapse back to the absolute endpoint — the very
    // bug the span correction removes.
    // ------------------------------------------------------------------

    #[test]
    fn positive_min_ignores_motors_off_zeros_in_carry_over_log() {
        // Second flight of a power cycle: counter already at ~163 once
        // airborne, with motors-off zero frames at the head of the log.
        assert_eq!(positive_min(&[0.0, 0.0, 163.1, 347.4]), 163.1);
    }

    #[test]
    fn positive_min_fresh_power_cycle_takes_first_airborne_value() {
        assert_eq!(positive_min(&[0.0, 0.0, 0.1, 162.8]), 0.1);
    }

    #[test]
    fn positive_min_all_motors_off_is_zero() {
        assert_eq!(positive_min(&[0.0, 0.0]), 0.0);
    }

    #[test]
    fn wall_clock_seconds_parses_both_iso_forms() {
        // Binary-path format: no fractional seconds, trailing Z.
        assert_eq!(
            wall_clock_seconds(
                Some("2026-07-09T07:36:24Z"),
                Some("2026-07-09T07:42:24Z"),
            ),
            Some(360)
        );
        // CSV-path format: fractional seconds, trailing Z.
        assert_eq!(
            wall_clock_seconds(
                Some("2026-07-09T07:36:24.740Z"),
                Some("2026-07-09T07:36:34.740Z"),
            ),
            Some(10)
        );
        // Garbage or missing input → None (so it can't act as a fallback).
        assert_eq!(
            wall_clock_seconds(Some("not-a-date"), Some("2026-07-09T07:36:24Z")),
            None
        );
        assert_eq!(wall_clock_seconds(None, Some("2026-07-09T07:36:24Z")), None);
        // end before start → None (never a negative duration).
        assert_eq!(
            wall_clock_seconds(
                Some("2026-07-09T07:42:24Z"),
                Some("2026-07-09T07:36:24Z"),
            ),
            None
        );
    }

    #[test]
    fn v11_scenario_recover_already_filled_so_details_is_noop() {
        // Simulates a v11 log where recover ran first and populated all
        // three SNs. The fallback must NOT overwrite even if details has
        // values (recover wins invariant).
        let mut aircraft = Some("recover-aircraft-sn".to_string());
        let mut battery = Some("recover-battery-sn".to_string());
        let mut rc = Some("recover-rc-sn".to_string());
        let f1 = fallback_from_details(&mut aircraft, "details-aircraft-sn", |_| false);
        let f2 = fallback_from_details(&mut battery, "details-battery-sn", |_| false);
        let f3 = fallback_from_details(&mut rc, "details-rc-sn", |_| false);
        assert!(!f1 && !f2 && !f3);
        assert_eq!(aircraft.as_deref(), Some("recover-aircraft-sn"));
        assert_eq!(battery.as_deref(), Some("recover-battery-sn"));
        assert_eq!(rc.as_deref(), Some("recover-rc-sn"));
    }
}
