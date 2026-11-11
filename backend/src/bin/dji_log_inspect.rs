// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

// Standalone forensic tool: parse a DJI binary .txt log via dji-log-parser
// and dump the key identity / metadata fields as JSON to stdout. Mirrors the
// production parser's invocation pattern (`backend/src/services/dji_parser.rs`)
// so the output reflects what the import path would see.
//
// Usage:
//   dji_log_inspect <path-to-log.txt> [dji-api-key]
//
// If the API key argv is omitted, falls back to env var DJI_API_KEY_GLOBAL.
// API key is only required for log version 13+ (encrypted). Older logs
// (v9, v11) parse without keychains.

use dji_log_parser::DJILog;
use serde_json::json;
use std::env;
use std::fs;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = env::args().collect();
    let path = match args.get(1) {
        Some(p) => p.clone(),
        None => {
            eprintln!("Usage: dji_log_inspect <path-to-log.txt> [dji-api-key]");
            return ExitCode::from(2);
        }
    };
    let api_key: Option<String> = args.get(2).cloned()
        .or_else(|| env::var("DJI_API_KEY_GLOBAL").ok())
        .filter(|s| !s.is_empty());

    let bytes = match fs::read(&path) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("Failed to read {}: {}", path, e);
            return ExitCode::from(1);
        }
    };

    let parser = match DJILog::from_bytes(bytes) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Failed to parse DJI log header: {}", e);
            return ExitCode::from(1);
        }
    };

    let log_version = parser.version;

    let frames = match api_key.as_deref() {
        Some(key) => {
            let keychains = match parser.fetch_keychains(key) {
                Ok(k) => k,
                Err(e) => {
                    eprintln!("Failed to fetch DJI keychains (encrypted log, check API key): {}", e);
                    return ExitCode::from(1);
                }
            };
            match parser.frames(Some(keychains)) {
                Ok(f) => f,
                Err(e) => {
                    eprintln!("Failed to decode frames with keychain: {}", e);
                    return ExitCode::from(1);
                }
            }
        }
        None => {
            match parser.frames(None) {
                Ok(f) => f,
                Err(e) => {
                    eprintln!(
                        "Failed to decode frames without API key (log version {} may be encrypted — provide API key as argv[2] or DJI_API_KEY_GLOBAL): {}",
                        log_version, e
                    );
                    return ExitCode::from(1);
                }
            }
        }
    };

    // Aggregate frame-level data. Same logic shape as services::dji_parser.
    let mut max_altitude: f32 = 0.0;
    let mut max_distance: f32 = 0.0;
    let mut home_lat: Option<f64> = None;
    let mut home_lon: Option<f64> = None;
    let mut first_dt: Option<String> = None;
    let mut recover_aircraft_sn: Option<String> = None;
    let mut recover_aircraft_name: Option<String> = None;
    let mut recover_battery_sn: Option<String> = None;
    let mut recover_rc_sn: Option<String> = None;
    let mut recover_app_version: Option<String> = None;
    let mut osd_drone_type: Option<String> = None;
    let mut photo_count_frames: i32 = 0;
    let mut video_row_count: i32 = 0;

    for frame in &frames {
        let osd = &frame.osd;
        let lat = osd.latitude;
        let lon = osd.longitude;

        if lat.abs() > 0.0001 && lon.abs() > 0.0001 {
            if home_lat.is_none() {
                home_lat = Some(lat);
                home_lon = Some(lon);
            }
            if osd.altitude > max_altitude {
                max_altitude = osd.altitude;
            }
            if let (Some(hlat), Some(hlon)) = (home_lat, home_lon) {
                let d = haversine_m(hlat, hlon, lat, lon);
                if d > max_distance { max_distance = d; }
            }
        }

        let custom = &frame.custom;
        let dt_str = custom.date_time.format("%Y-%m-%dT%H:%M:%SZ").to_string();
        if !dt_str.starts_with("1970") && first_dt.is_none() {
            first_dt = Some(dt_str);
        }

        let recover = &frame.recover;
        if recover_aircraft_sn.is_none() && !recover.aircraft_sn.is_empty() {
            recover_aircraft_sn = Some(recover.aircraft_sn.clone());
        }
        if recover_aircraft_name.is_none() && !recover.aircraft_name.is_empty() {
            recover_aircraft_name = Some(recover.aircraft_name.clone());
        }
        if recover_battery_sn.is_none() && !recover.battery_sn.is_empty() {
            recover_battery_sn = Some(recover.battery_sn.clone());
        }
        if recover_rc_sn.is_none() && !recover.rc_sn.is_empty() {
            recover_rc_sn = Some(recover.rc_sn.clone());
        }
        if recover_app_version.is_none() && !recover.app_version.is_empty() {
            recover_app_version = Some(recover.app_version.clone());
        }

        if osd_drone_type.is_none() {
            let dt = format!("{:?}", osd.drone_type);
            if dt != "None" && !dt.is_empty() {
                osd_drone_type = Some(dt);
            }
        }

        let camera = &frame.camera;
        if camera.is_photo { photo_count_frames += 1; }
        if camera.is_video { video_row_count += 1; }
    }

    let duration_seconds = frames.last().map(|f| f.osd.fly_time as i32).unwrap_or(0);

    let det = &parser.details;
    let details_product_type = format!("{:?}", det.product_type);

    // The "aircraft_type" answer for our forensic purpose: prefer the
    // top-level DJILog.details.product_type, which is what the dji-log-parser
    // crate exposes as the canonical drone type. Fall back to OSD's
    // drone_type when product_type is None.
    let aircraft_type = if details_product_type != "None" {
        details_product_type.clone()
    } else {
        osd_drone_type.clone().unwrap_or_else(|| "Unknown".to_string())
    };

    let aircraft_serial = recover_aircraft_sn.clone()
        .or_else(|| if det.aircraft_sn.is_empty() { None } else { Some(det.aircraft_sn.clone()) });
    let aircraft_name = recover_aircraft_name.clone()
        .or_else(|| if det.aircraft_name.is_empty() { None } else { Some(det.aircraft_name.clone()) });
    let controller_serial = recover_rc_sn.clone()
        .or_else(|| if det.rc_sn.is_empty() { None } else { Some(det.rc_sn.clone()) });
    let battery_serial = recover_battery_sn.clone()
        .or_else(|| if det.battery_sn.is_empty() { None } else { Some(det.battery_sn.clone()) });
    let app_version = recover_app_version.clone()
        .or_else(|| if det.app_version.is_empty() || det.app_version == "0.0.0" { None } else { Some(det.app_version.clone()) });

    let photo_count = if det.capture_num > 0 { det.capture_num } else { photo_count_frames };
    let video_seconds = if det.video_time > 0 { det.video_time as i32 } else { (video_row_count as f32 * 0.1) as i32 };

    let out = json!({
        "log_version":           log_version,
        "aircraft_type":         aircraft_type,
        "details_product_type":  details_product_type,
        "osd_drone_type":        osd_drone_type,
        "aircraft_serial":       aircraft_serial,
        "aircraft_name":         aircraft_name,
        "controller_serial":     controller_serial,
        "battery_serial":        battery_serial,
        "app_version":           app_version,
        "first_flight_datetime": first_dt,
        "duration_seconds":      duration_seconds,
        "max_altitude_m":        max_altitude,
        "max_distance_m":        max_distance,
        "home_lat":              home_lat,
        "home_lon":              home_lon,
        "photo_count":           photo_count,
        "video_seconds":         video_seconds,
        "frame_count":           frames.len(),
    });

    println!("{}", serde_json::to_string_pretty(&out).unwrap());
    ExitCode::from(0)
}

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
