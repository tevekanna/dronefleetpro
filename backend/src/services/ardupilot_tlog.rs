// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

#![allow(dead_code)]
//! Standalone MAVLink `.tlog` (telemetry capture) parser for ArduPilot flight
//! logs — PR A of 3. This module is intentionally NOT yet wired into the HTTP
//! layer or the DJI import path; the endpoint, multi-flight return shape and
//! review UI land in PR B, and the `.bin` DataFlash parser + merge logic in
//! PR C. The module-level `allow(dead_code)` reflects that: every public item
//! here is consumed downstream, not in this PR.
//!
//! A `.tlog` is a MAVLink v2 telemetry capture produced by a ground control
//! station (Mission Planner). The on-disk record format is a repeating
//! sequence of an 8-byte big-endian `u64` timestamp (microseconds since the
//! Unix epoch, UTC) immediately followed by exactly one MAVLink frame
//! (v1 magic `0xFE` or v2 magic `0xFD`).
//!
//! One `.tlog` file = one GCS connection session containing 0..N arm/disarm
//! cycles. Each arm→disarm cycle is treated as ONE flight (one
//! [`ArduPilotSegmentPreview`]). Identity is the autopilot board UID (the
//! `aircraft.serial_number` analogue — no body SN is broadcast in MAVLink).

use mavlink::dialects::ardupilotmega::MavMessage;
use mavlink::{MavlinkVersion, Message};

use crate::errors::AppError;
use crate::services::dji_parser::{FlightPreview, TrajectoryPoint};

/// One arm→disarm segment from a `.tlog`, wrapping a [`FlightPreview`] (the
/// same shape the DJI importer produces) plus the two ArduPilot-specific
/// review hints. The flag is advisory for the PR B review UI — no segment is
/// ever silently dropped.
#[derive(Debug, serde::Serialize, Clone)]
pub struct ArduPilotSegmentPreview {
    #[serde(flatten)]
    pub preview: FlightPreview,
    /// Heuristic used by the PR B review UI to pre-select real flights vs
    /// bench/ground tests. True when max relative altitude >= 2.0 m OR
    /// armed duration >= 30 s. Never used to silently drop a segment.
    pub looks_like_flight: bool,
    pub armed_seconds: i32,
    /// Set by the import handler (not the parser): true when a flight with
    /// this segment's flight_guid (or source_filename) already exists, so the
    /// review UI can disable it. Defaults false from the parser.
    #[serde(default)]
    pub already_imported: bool,
}

/// Decoded message payload kinds we care about, tagged with their GCS
/// wall-clock timestamp (microseconds since Unix epoch). Keeping a compact
/// enum rather than the full `MavMessage` keeps memory modest on long
/// captures while still letting us split into segments in a second pass.
#[derive(Debug, Clone)]
enum Ev {
    /// Autopilot (sysid==1 && compid==1) HEARTBEAT armed state.
    Heartbeat { armed: bool },
    /// GLOBAL_POSITION_INT — trajectory point (relative altitude, metres AGL).
    Pos { lat: f64, lon: f64, alt: f64 },
    /// GPS_RAW_INT satellites visible.
    Gps { sats: u8 },
    /// VFR_HUD ground speed (m/s).
    Speed { gs: f32 },
    /// SYS_STATUS battery remaining percent (-1 == unknown).
    Batt { rem: i8 },
    /// BATTERY_STATUS temperature, already validated and converted to Celsius.
    Temp { c: f32 },
    /// HOME_POSITION / GPS_GLOBAL_ORIGIN — home location in WGS84 degrees.
    Home { lat: f64, lon: f64 },
}

/// A single framed MAVLink record located in the byte buffer: the leading
/// 8-byte GCS timestamp plus the frame header fields and payload slice.
struct ParsedFrame<'a> {
    ts_us: u64,
    sysid: u8,
    compid: u8,
    msgid: u32,
    version: MavlinkVersion,
    payload: &'a [u8],
    /// Total bytes consumed by this record (8-byte timestamp + the frame).
    total: usize,
}

/// Public entry point: parse a `.tlog` byte slice into one preview per
/// arm/disarm segment. Returns every segment found, in file order; the
/// `looks_like_flight` flag is advisory only. An empty `Vec` is returned when
/// the file framed correctly but contained no arm/disarm cycle. A
/// `BadRequest` is returned only when no MAVLink frames could be located at
/// all (i.e. the input is not a telemetry log).
pub fn parse_tlog(bytes: &[u8], filename: &str) -> Result<Vec<ArduPilotSegmentPreview>, AppError> {
    let mut events: Vec<(u64, Ev)> = Vec::new();
    let mut heartbeats: Vec<(u64, bool)> = Vec::new();

    // Global identity / metadata captured from the connection (first wins).
    let mut uid_from_av: Option<String> = None;
    let mut uid_from_banner: Option<String> = None;
    let mut firmware: Option<String> = None;
    let mut frame_label: Option<String> = None;
    let mut mav_type_name: Option<String> = None;
    let mut home_position_seen = false;

    let mut valid_frames: u64 = 0;
    let mut last_ts: u64 = 0;

    // --- Framing walk -------------------------------------------------------
    let mut cursor = 0usize;
    while bytes.len().saturating_sub(cursor) >= 12 {
        let frame = match read_record(bytes, cursor) {
            Some(f) => f,
            // Desync: bad magic byte, or a computed frame that overruns the
            // buffer. Advance one byte and re-attempt the [timestamp + frame]
            // read — telemetry captures occasionally drop bytes. The >= 12
            // loop bound stops a corrupt tail from looping forever.
            None => {
                cursor += 1;
                continue;
            }
        };
        let advance = frame.total;
        valid_frames += 1;
        last_ts = frame.ts_us;

        // Unknown / extension-only messages return Err here — that is normal,
        // not fatal: skip the (structurally valid) frame and keep going.
        if let Ok(msg) = MavMessage::parse(frame.version, frame.msgid, frame.payload) {
            handle_message(
                &msg,
                &frame,
                &mut events,
                &mut heartbeats,
                &mut uid_from_av,
                &mut uid_from_banner,
                &mut firmware,
                &mut frame_label,
                &mut mav_type_name,
                &mut home_position_seen,
            );
        }

        cursor += advance;
    }

    if valid_frames == 0 {
        return Err(AppError::BadRequest(format!(
            "No MAVLink frames found in {filename} — not a telemetry (.tlog) file"
        )));
    }

    // --- Segment splitting --------------------------------------------------
    let mut segments = split_segments(&heartbeats);
    // If the stream ended while still armed, the final segment was closed at
    // the last heartbeat; extend it to the last frame actually seen.
    if let (Some((_, last_armed)), Some(last_seg)) = (heartbeats.last(), segments.last_mut()) {
        if *last_armed && last_ts > last_seg.1 {
            last_seg.1 = last_ts;
        }
    }

    let uid = uid_from_av
        .or(uid_from_banner)
        .unwrap_or_else(|| "unknown-uid".to_string());
    let frame_or_type = frame_label
        .or(mav_type_name)
        .unwrap_or_else(|| "UNKNOWN".to_string());

    let previews = segments
        .iter()
        .enumerate()
        .map(|(i, &(arm_us, disarm_us))| {
            build_segment(
                &events,
                arm_us,
                disarm_us,
                i + 1,
                filename,
                &uid,
                &frame_or_type,
                firmware.as_deref(),
            )
        })
        .collect();

    Ok(previews)
}

/// Reads the 8-byte big-endian timestamp at `pos` followed by one MAVLink
/// frame. Returns `None` on a bad magic byte or a frame that would overrun
/// the buffer (the caller treats that as desync and advances one byte).
fn read_record(buf: &[u8], pos: usize) -> Option<ParsedFrame<'_>> {
    let ts = u64::from_be_bytes(buf.get(pos..pos + 8)?.try_into().ok()?);
    let f = pos + 8;
    match *buf.get(f)? {
        0xFD => {
            // v2: [0]=0xFD [1]=len [2]=incompat [3]=compat [4]=seq [5]=sysid
            //     [6]=compid [7..10]=msgid(LE 3 bytes) [10..]=payload, 2B CRC,
            //     +13B signature if incompat & 0x01.
            if buf.len() < f + 10 {
                return None;
            }
            let payload_len = buf[f + 1] as usize;
            let signed = buf[f + 2] & 0x01 != 0;
            let sysid = buf[f + 5];
            let compid = buf[f + 6];
            let msgid = u32::from_le_bytes([buf[f + 7], buf[f + 8], buf[f + 9], 0]);
            let frame_total = 12 + payload_len + if signed { 13 } else { 0 };
            if buf.len() < f + frame_total {
                return None;
            }
            Some(ParsedFrame {
                ts_us: ts,
                sysid,
                compid,
                msgid,
                version: MavlinkVersion::V2,
                payload: &buf[f + 10..f + 10 + payload_len],
                total: 8 + frame_total,
            })
        }
        0xFE => {
            // v1: [0]=0xFE [1]=len [2]=seq [3]=sysid [4]=compid [5]=msgid(1B)
            //     [6..]=payload, 2B CRC.
            if buf.len() < f + 6 {
                return None;
            }
            let payload_len = buf[f + 1] as usize;
            let sysid = buf[f + 3];
            let compid = buf[f + 4];
            let msgid = buf[f + 5] as u32;
            let frame_total = 8 + payload_len;
            if buf.len() < f + frame_total {
                return None;
            }
            Some(ParsedFrame {
                ts_us: ts,
                sysid,
                compid,
                msgid,
                version: MavlinkVersion::V1,
                payload: &buf[f + 6..f + 6 + payload_len],
                total: 8 + frame_total,
            })
        }
        _ => None,
    }
}

/// Routes one decoded message into the event stream / global metadata.
#[allow(clippy::too_many_arguments)]
fn handle_message(
    msg: &MavMessage,
    frame: &ParsedFrame,
    events: &mut Vec<(u64, Ev)>,
    heartbeats: &mut Vec<(u64, bool)>,
    uid_from_av: &mut Option<String>,
    uid_from_banner: &mut Option<String>,
    firmware: &mut Option<String>,
    frame_label: &mut Option<String>,
    mav_type_name: &mut Option<String>,
    home_position_seen: &mut bool,
) {
    let ts = frame.ts_us;
    match msg {
        // Armed state is ONLY trusted from the autopilot's own heartbeat.
        MavMessage::HEARTBEAT(hb) if frame.sysid == 1 && frame.compid == 1 => {
            let armed = (hb.base_mode.bits() & 0x80) != 0;
            heartbeats.push((ts, armed));
            events.push((ts, Ev::Heartbeat { armed }));
            if mav_type_name.is_none() {
                *mav_type_name = Some(
                    format!("{:?}", hb.mavtype)
                        .trim_start_matches("MAV_TYPE_")
                        .to_string(),
                );
            }
        }
        MavMessage::GLOBAL_POSITION_INT(p) => events.push((
            ts,
            Ev::Pos {
                lat: p.lat as f64 / 1e7,
                lon: p.lon as f64 / 1e7,
                alt: p.relative_alt as f64 / 1000.0,
            },
        )),
        MavMessage::GPS_RAW_INT(g) => events.push((
            ts,
            Ev::Gps {
                sats: g.satellites_visible,
            },
        )),
        MavMessage::VFR_HUD(v) => events.push((ts, Ev::Speed { gs: v.groundspeed })),
        MavMessage::SYS_STATUS(s) => events.push((
            ts,
            Ev::Batt {
                rem: s.battery_remaining,
            },
        )),
        MavMessage::BATTERY_STATUS(b) => {
            // INT16_MAX is the unknown-temperature sentinel; field is centidegC.
            if b.temperature != i16::MAX {
                events.push((
                    ts,
                    Ev::Temp {
                        c: b.temperature as f32 / 100.0,
                    },
                ));
            }
        }
        MavMessage::HOME_POSITION(h) => {
            *home_position_seen = true;
            events.push((
                ts,
                Ev::Home {
                    lat: h.latitude as f64 / 1e7,
                    lon: h.longitude as f64 / 1e7,
                },
            ));
        }
        // GPS_GLOBAL_ORIGIN supplies home only until a real HOME_POSITION is
        // seen — HOME_POSITION is preferred.
        MavMessage::GPS_GLOBAL_ORIGIN(o) if !*home_position_seen => events.push((
            ts,
            Ev::Home {
                lat: o.latitude as f64 / 1e7,
                lon: o.longitude as f64 / 1e7,
            },
        )),
        MavMessage::AUTOPILOT_VERSION(av) => {
            if uid_from_av.is_none() {
                let h = uid_hex(&av.uid2);
                if !h.is_empty() {
                    *uid_from_av = Some(h);
                }
            }
        }
        MavMessage::STATUSTEXT(st) => {
            if let Ok(s) = st.text.to_str() {
                classify_statustext(s, uid_from_banner, firmware, frame_label);
            }
        }
        // SYSTEM_TIME is available for cross-checking but the GCS timestamp is
        // the authoritative time base, so we ignore it here.
        _ => {}
    }
}

/// Builds one [`ArduPilotSegmentPreview`] for the arm/disarm window
/// `[arm_us, disarm_us]` from the decoded event stream.
#[allow(clippy::too_many_arguments)]
fn build_segment(
    events: &[(u64, Ev)],
    arm_us: u64,
    disarm_us: u64,
    index: usize,
    filename: &str,
    uid: &str,
    frame_or_type: &str,
    firmware: Option<&str>,
) -> ArduPilotSegmentPreview {
    // Home in effect for the segment: the most recent Home event at or before
    // disarm (home is often set just after arming, but may predate it).
    let home = events
        .iter()
        .filter(|(ts, ev)| *ts <= disarm_us && matches!(ev, Ev::Home { .. }))
        .filter_map(|(_, ev)| match ev {
            Ev::Home { lat, lon } => Some((*lat, *lon)),
            _ => None,
        })
        .last();

    let mut trajectory: Vec<TrajectoryPoint> = Vec::new();
    let mut max_altitude_m: f32 = 0.0;
    let mut max_speed_ms: Option<f32> = None;
    let mut min_sats: Option<i32> = None;
    let mut min_batt: Option<i32> = None;
    let mut max_temp: Option<f32> = None;

    for (ts, ev) in events.iter().filter(|(ts, _)| *ts >= arm_us && *ts <= disarm_us) {
        match ev {
            Ev::Pos { lat, lon, alt } => {
                let t_sec = (*ts - arm_us) as f64 / 1e6;
                if *alt as f32 > max_altitude_m {
                    max_altitude_m = *alt as f32;
                }
                trajectory.push(TrajectoryPoint {
                    lat: *lat,
                    lon: *lon,
                    alt: *alt,
                    t_sec,
                });
            }
            Ev::Speed { gs } => {
                max_speed_ms = Some(max_speed_ms.map_or(*gs, |m: f32| m.max(*gs)));
            }
            Ev::Gps { sats } => {
                let s = *sats as i32;
                min_sats = Some(min_sats.map_or(s, |m: i32| m.min(s)));
            }
            Ev::Batt { rem } => {
                if *rem >= 0 {
                    let r = *rem as i32;
                    min_batt = Some(min_batt.map_or(r, |m: i32| m.min(r)));
                }
            }
            Ev::Temp { c } => {
                max_temp = Some(max_temp.map_or(*c, |m: f32| m.max(*c)));
            }
            _ => {}
        }
    }

    // Distances derived from the accumulated trajectory + segment home.
    let mut total_distance_m: f32 = 0.0;
    let mut max_distance_m: f32 = 0.0;
    let mut prev: Option<(f64, f64)> = None;
    for p in &trajectory {
        if let Some((plat, plon)) = prev {
            total_distance_m += haversine_m(plat, plon, p.lat, p.lon);
        }
        if let Some((hlat, hlon)) = home {
            let d = haversine_m(hlat, hlon, p.lat, p.lon);
            if d > max_distance_m {
                max_distance_m = d;
            }
        }
        prev = Some((p.lat, p.lon));
    }

    let duration_seconds = (disarm_us.saturating_sub(arm_us) / 1_000_000) as i32;
    let armed_seconds = duration_seconds;
    let looks_like_flight = looks_like_flight(max_altitude_m, armed_seconds);

    let preview = FlightPreview {
        source_filename: format!("{filename}#seg{index}"),
        flight_guid: Some(format_flight_guid(uid, (arm_us / 1_000_000) as i64)),
        aircraft_name: Some(format!("ArduPilot {frame_or_type}")),
        aircraft_model: None,
        aircraft_serial: Some(uid.to_string()),
        flight_start: us_to_rfc3339(arm_us),
        flight_end: us_to_rfc3339(disarm_us),
        duration_seconds,
        max_altitude_m,
        max_distance_m,
        total_distance_m,
        max_speed_ms,
        home_lat: home.map(|(la, _)| la as f32),
        home_lon: home.map(|(_, lo)| lo as f32),
        satellite_count_min: min_sats,
        battery_temp_c: max_temp,
        cell_variance_v: None,
        min_battery_pct: min_batt,
        battery_serial: None,
        rc_serial: None,
        app_version: firmware.map(str::to_string),
        app_source: Some("ArduPilot".to_string()),
        drone_type_code: Some(frame_or_type.to_string()),
        photo_count: 0,
        video_seconds: 0,
        trajectory,
        model_detection_status: Some("unknown".to_string()),
        aircraft_match: None,
        battery_match: None,
        controller_match: None,
        // .tlog findings are out of scope for this PR (ArduPilot .bin only).
        findings: Vec::new(),
    };

    ArduPilotSegmentPreview {
        preview,
        looks_like_flight,
        armed_seconds,
        already_imported: false,
    }
}

/// Splits an ordered sequence of autopilot heartbeats `(timestamp_us, armed)`
/// into `(arm_us, disarm_us)` segments. A disarmed→armed transition opens a
/// segment; armed→disarmed closes it. If the stream ends while still armed,
/// the final segment is closed at the last heartbeat timestamp (the caller
/// may extend this to the last frame actually seen).
fn split_segments(heartbeats: &[(u64, bool)]) -> Vec<(u64, u64)> {
    let mut segments = Vec::new();
    let mut prev_armed = false;
    let mut arm_open: Option<u64> = None;
    let mut last_ts = 0u64;

    for &(ts, armed) in heartbeats {
        last_ts = ts;
        if armed && !prev_armed {
            arm_open = Some(ts);
        } else if !armed && prev_armed {
            if let Some(arm) = arm_open.take() {
                segments.push((arm, ts));
            }
        }
        prev_armed = armed;
    }
    // Stream ended while still armed — close at the last heartbeat seen.
    if let Some(arm) = arm_open {
        segments.push((arm, last_ts));
    }
    segments
}

/// Advisory heuristic: a segment looks like a real flight when it climbed at
/// least 2 m AGL or stayed armed at least 30 s. Never used to drop a segment.
///
/// `pub` so the `.bin` parser (PR C) applies the identical pre-select
/// heuristic — the review UI must behave the same for both log formats.
pub fn looks_like_flight(max_altitude_m: f32, armed_seconds: i32) -> bool {
    max_altitude_m >= 2.0 || armed_seconds >= 30
}

/// Lowercase-hex encodes a board UID byte array after trimming trailing zero
/// bytes (`AUTOPILOT_VERSION.uid2` is right-zero-padded). Returns an empty
/// string when every byte is zero.
fn uid_hex(bytes: &[u8]) -> String {
    let end = bytes.iter().rposition(|&b| b != 0).map_or(0, |i| i + 1);
    bytes[..end].iter().map(|b| format!("{b:02x}")).collect()
}

/// Format-independent merge key reused by the `.bin` importer in PR C.
/// `pub` so a `.bin` and a `.tlog` of the same flight produce guids that
/// differ only within the merge window — the precondition for the merge.
pub fn format_flight_guid(uid: &str, arm_unix_seconds: i64) -> String {
    format!("ardupilot:{uid}:{arm_unix_seconds}")
}

/// Parses a boot-banner STATUSTEXT of the form `<BoardName> <8hex> <8hex>
/// <8hex>` (e.g. `CubeOrangePlus 04030201 08070605 0c0b0a09`) into a single
/// lowercase-hex board UID. Returns `None` if the shape doesn't match.
///
/// The banner prints each of the three 32-bit UID words in big-endian
/// (word) order, whereas `AUTOPILOT_VERSION.uid2` carries the raw
/// little-endian board UID bytes (encoded by [`uid_hex`]). To make the two
/// sources agree, each 8-hex-char word is byte-reversed here before the
/// words are concatenated. So `04030201 08070605 0c0b0a09` becomes
/// `01020304 05060708 090a0b0c` → `0102030405060708090a0b0c`, which is
/// exactly `uid_hex(uid2)` for the same board.
///
/// `pub` so the `.bin` parser derives the same board UID from the DataFlash
/// `MSG` boot banner; with the byte-order normalisation, `.tlog` (uid2 or
/// banner) and `.bin` (banner) all resolve to the same `aircraft_serial`.
pub fn parse_boot_banner_uid(text: &str) -> Option<String> {
    let tokens: Vec<&str> = text.split_whitespace().collect();
    if tokens.len() < 4 {
        return None;
    }
    let tail = &tokens[tokens.len() - 3..];
    if !tail
        .iter()
        .all(|t| t.len() == 8 && t.chars().all(|c| c.is_ascii_hexdigit()))
    {
        return None;
    }
    // Byte-reverse each 32-bit word: split into four 2-hex byte pairs and
    // emit them in reverse order, so the banner's big-endian word form
    // matches uid2's raw little-endian byte order.
    let uid: String = tail
        .iter()
        .flat_map(|word| {
            let bytes: Vec<&str> = (0..4).map(|i| &word[i * 2..i * 2 + 2]).collect();
            bytes.into_iter().rev()
        })
        .collect();
    Some(uid.to_lowercase())
}

/// Classifies a STATUSTEXT line into the boot-banner UID, the firmware string,
/// or the frame string (first occurrence of each wins).
fn classify_statustext(
    s: &str,
    uid_from_banner: &mut Option<String>,
    firmware: &mut Option<String>,
    frame_label: &mut Option<String>,
) {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return;
    }
    if uid_from_banner.is_none() {
        if let Some(uid) = parse_boot_banner_uid(trimmed) {
            *uid_from_banner = Some(uid);
        }
    }
    // Firmware banner: "ArduCopter V4.6.3 (92b0cd78)", "ArduPlane ...", etc.
    if firmware.is_none() && trimmed.starts_with("Ardu") {
        *firmware = Some(trimmed.to_string());
    }
    // Frame banner: "Frame: QUAD/X".
    if frame_label.is_none() {
        if let Some(rest) = trimmed.strip_prefix("Frame: ") {
            let v = rest.trim();
            if !v.is_empty() {
                *frame_label = Some(v.to_string());
            }
        }
    }
}

/// Converts a Unix-epoch microsecond timestamp into an RFC 3339 / ISO 8601 UTC
/// string. Returns `None` if the value is out of chrono's representable range.
/// `pub` so the `.bin` parser formats its GPS-anchored UTC timestamps
/// identically.
pub fn us_to_rfc3339(ts_us: u64) -> Option<String> {
    let secs = (ts_us / 1_000_000) as i64;
    let nsub = ((ts_us % 1_000_000) * 1000) as u32;
    chrono::DateTime::<chrono::Utc>::from_timestamp(secs, nsub).map(|dt| dt.to_rfc3339())
}

/// Haversine distance in metres between two WGS84 points. `pub` so the
/// `.bin` parser computes max/total distance with the identical formula.
pub fn haversine_m(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f32 {
    let r = 6_371_000.0_f64;
    let dlat = (lat2 - lat1).to_radians();
    let dlon = (lon2 - lon1).to_radians();
    let a = (dlat / 2.0).sin().powi(2)
        + lat1.to_radians().cos() * lat2.to_radians().cos() * (dlon / 2.0).sin().powi(2);
    let c = 2.0 * a.sqrt().atan2((1.0 - a).sqrt());
    (r * c) as f32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segments_single_arm_disarm() {
        let hb = vec![(100, false), (200, true), (300, true), (400, false)];
        assert_eq!(split_segments(&hb), vec![(200, 400)]);
    }

    #[test]
    fn segments_multiple_disjoint() {
        let hb = vec![
            (100, false),
            (200, true),
            (300, false),
            (500, true),
            (600, false),
        ];
        assert_eq!(split_segments(&hb), vec![(200, 300), (500, 600)]);
    }

    #[test]
    fn segments_ends_while_armed_closes_at_last_timestamp() {
        let hb = vec![(100, false), (200, true), (300, true)];
        assert_eq!(split_segments(&hb), vec![(200, 300)]);
    }

    #[test]
    fn segments_empty_when_never_armed() {
        let hb = vec![(100, false), (200, false)];
        assert!(split_segments(&hb).is_empty());
    }

    #[test]
    fn looks_like_flight_thresholds() {
        // High enough altitude, short duration.
        assert!(looks_like_flight(5.0, 10));
        // Low altitude, long duration.
        assert!(looks_like_flight(0.5, 45));
        // Low altitude, short duration — bench/ground test.
        assert!(!looks_like_flight(0.5, 5));
        // Exactly at the altitude threshold, zero duration.
        assert!(looks_like_flight(2.0, 0));
    }

    #[test]
    fn uid_hex_trims_trailing_zeros() {
        let raw = [
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0, 0, 0, 0, 0,
            0,
        ];
        assert_eq!(uid_hex(&raw), "0102030405060708090a0b0c");
    }

    #[test]
    fn uid_hex_all_zero_is_empty() {
        assert_eq!(uid_hex(&[0u8; 18]), "");
    }

    #[test]
    fn flight_guid_format_matches_contract() {
        let guid = format_flight_guid("0102030405060708090a0b0c", 1_717_500_000);
        // Equivalent to /^ardupilot:[0-9a-f]+:\d+$/.
        let mut parts = guid.splitn(3, ':');
        assert_eq!(parts.next(), Some("ardupilot"));
        let uid = parts.next().unwrap();
        let secs = parts.next().unwrap();
        assert!(!uid.is_empty() && uid.chars().all(|c| c.is_ascii_hexdigit() && !c.is_uppercase()));
        assert!(!secs.is_empty() && secs.chars().all(|c| c.is_ascii_digit()));
    }

    #[test]
    fn boot_banner_uid_parsed() {
        // Each 32-bit word is byte-reversed so the banner's big-endian word
        // form matches uid2's raw little-endian byte order.
        assert_eq!(
            parse_boot_banner_uid("CubeOrangePlus 04030201 08070605 0c0b0a09"),
            Some("0102030405060708090a0b0c".to_string())
        );
        // Not a banner — too few hex words.
        assert_eq!(parse_boot_banner_uid("ArduCopter V4.6.3 (92b0cd78)"), None);
        assert_eq!(parse_boot_banner_uid("Frame: QUAD/X"), None);
    }

    #[test]
    fn boot_banner_uid_matches_uid2_byte_order() {
        // The same physical board: uid2 raw little-endian bytes vs. the
        // big-endian-word boot banner. After normalisation both encodings
        // must produce the identical aircraft_serial.
        let uid2 = [
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c,
        ];
        assert_eq!(
            parse_boot_banner_uid("X 04030201 08070605 0c0b0a09"),
            Some(uid_hex(&uid2))
        );
    }

    #[test]
    fn haversine_zero_for_identical_points() {
        let d = haversine_m(59.3293, 18.0686, 59.3293, 18.0686);
        assert!(d.abs() < 0.001, "expected ~0, got {d}");
    }

    #[test]
    fn haversine_known_baseline() {
        // 0.001 deg of latitude is ~111.2 m anywhere on Earth.
        let d = haversine_m(59.0, 18.0, 59.001, 18.0);
        assert!((d - 111.2).abs() < 1.0, "expected ~111 m, got {d}");
    }
}
