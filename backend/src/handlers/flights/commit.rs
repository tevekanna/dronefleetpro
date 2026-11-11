// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;

use crate::auth::middleware::{require_role, Claims};
use crate::db::DbPool;
use crate::errors::AppError;
use crate::models::aircraft::Aircraft;
use crate::models::battery::Battery;
use crate::models::controller::Controller;
use crate::models::flight::Flight;
use crate::services::ardupilot_findings::LogFinding;
use crate::services::audit;
use crate::services::equipment_ids::next_equipment_id;
use crate::services::fc_sn_classifier::{
    classify_commit_fc_sn, is_fc_sn_prone_model, FcSnCommitDecision,
};
use crate::services::sn_matching::find_match;

/// Resolves (log_source, log_precedence) for a committed flight.
/// Precedence: explicit body log_source wins; else ArduPilot app_source
/// implies the telemetry tier; else the legacy DJI default.
/// NULL precedence for DJI/manual; 10 = ardupilot_tlog; 20 = ardupilot_bin.
pub fn resolve_log_source(body_log_source: Option<&str>, app_source: &str) -> (String, Option<i16>) {
    let ls = body_log_source
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .unwrap_or_else(|| {
            if app_source == "ArduPilot" {
                "ardupilot_tlog".to_string()
            } else {
                "dji_import".to_string()
            }
        });
    let prec = match ls.as_str() {
        "ardupilot_tlog" => Some(10),
        "ardupilot_bin" => Some(20),
        _ => None,
    };
    (ls, prec)
}

/// Parses an ArduPilot `flight_guid` of the form `ardupilot:{uid}:{arm_secs}`
/// into its board UID and arm-time (Unix seconds). Returns `None` for a guid
/// that isn't ArduPilot-shaped or whose arm-time isn't an integer.
fn parse_ardupilot_guid(guid: &str) -> Option<(String, i64)> {
    let mut parts = guid.splitn(3, ':');
    if parts.next()? != "ardupilot" {
        return None;
    }
    let uid = parts.next()?;
    let arm = parts.next()?;
    if uid.is_empty() {
        return None;
    }
    let arm_secs: i64 = arm.parse().ok()?;
    Some((uid.to_string(), arm_secs))
}

/// Merge window: a `.bin` and a `.tlog` of the same flight arm within a couple
/// of seconds of each other (the two logs start at slightly different points).
/// Ten seconds is generous enough to bridge that skew without colliding
/// distinct flights of the same airframe.
fn within_arm_window(a: i64, b: i64) -> bool {
    (a - b).abs() <= 10
}

/// Identifier source for an aircraft auto-registered/linked from a flight log.
fn aircraft_log_identifier_source(is_ardupilot: bool) -> &'static str {
    if is_ardupilot {
        crate::services::aircraft_identifiers::SOURCE_ARDUPILOT_UID
    } else {
        "fc_sn"
    }
}

/// 409 body returned by the pre-flight FC-SN refusal path. The `kind`
/// discriminator is hard-coded so frontends can branch off it without
/// peeking at status codes alone (other 409s on this endpoint may exist).
#[derive(Serialize)]
struct SuspiciousFcSnResponse {
    kind: &'static str,
    sn: String,
    model_name: String,
    candidates: Vec<FcSnCandidate>,
}

/// Verified aircraft of the same model as the unmatched SN. Frontend
/// picks `serial_number` / `equipment_id` / `nickname` for display.
#[derive(Serialize, sqlx::FromRow)]
struct FcSnCandidate {
    id: i32,
    serial_number: String,
    equipment_id: Option<String>,
    nickname: Option<String>,
}

/// Read-only fetch of verified aircraft of the same model. Used by both
/// the pre-flight refusal path and the sqlx::test integration tests.
async fn list_fc_sn_candidates(
    pool: &DbPool,
    org_id: i32,
    model_id: i32,
) -> Result<Vec<FcSnCandidate>, AppError> {
    let candidates = sqlx::query_as::<_, FcSnCandidate>(
        "SELECT id, serial_number, equipment_id, nickname FROM aircraft \
         WHERE org_id = $1 AND model_id = $2 AND needs_manual_sn = false \
         ORDER BY equipment_id NULLS LAST, serial_number",
    )
    .bind(org_id)
    .bind(model_id)
    .fetch_all(pool)
    .await?;
    Ok(candidates)
}

/// Validate that the operator-chosen aircraft is a legitimate link target
/// for the unmatched SN. Returns `Ok(target_model_id)` on success.
///
/// Errors:
///  * `BadRequest` — aircraft not found in this org, model mismatch, or
///    target itself is in needs_manual_sn=true (linking would chain ghosts).
async fn verify_link_target(
    pool: &DbPool,
    org_id: i32,
    link_id: i32,
    expected_model_id: Option<i32>,
) -> Result<Option<i32>, AppError> {
    use sqlx::Row;
    let row = sqlx::query(
        "SELECT model_id, needs_manual_sn FROM aircraft \
         WHERE id = $1 AND org_id = $2",
    )
    .bind(link_id)
    .bind(org_id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| {
        AppError::BadRequest(format!(
            "aircraft id {link_id} not found in this organisation"
        ))
    })?;
    let target_model_id: Option<i32> = row.try_get("model_id").ok().flatten();
    let target_needs_manual_sn: bool = row.get("needs_manual_sn");

    if target_model_id != expected_model_id {
        return Err(AppError::BadRequest(
            "target aircraft model does not match the log's model".into(),
        ));
    }
    if target_needs_manual_sn {
        return Err(AppError::BadRequest(
            "cannot link FC-SN to an aircraft that itself needs_manual_sn=true".into(),
        ));
    }
    Ok(target_model_id)
}

/// Sub-branch A side effects: write the `fc_sn` identifier row, link the
/// flight, and audit `LINK_FC_SN_IDENTIFIER`. Caller has already verified
/// the link target via `verify_link_target`.
#[allow(clippy::too_many_arguments)]
async fn perform_link_existing(
    pool: &DbPool,
    org_id: i32,
    user_id: i32,
    user_code: &str,
    link_id: i32,
    new_serial: &str,
    model_name: &str,
    model_id: Option<i32>,
    flight_id: i32,
    source: &str,
) -> Result<(), AppError> {
    let observed_in = format!("flight:{flight_id}");
    let _ = crate::services::aircraft_identifiers::link_identifier(
        pool,
        link_id,
        org_id,
        new_serial,
        source,
        Some(&observed_in),
        Some(user_id),
    )
    .await;

    sqlx::query("UPDATE flights SET aircraft_id = $1 WHERE id = $2")
        .bind(link_id)
        .bind(flight_id)
        .execute(pool)
        .await?;

    audit::log_custom_action(
        pool,
        org_id,
        user_id,
        user_code,
        "LINK_FC_SN_IDENTIFIER",
        "aircraft",
        link_id,
        serde_json::json!({
            "sn": new_serial,
            "source": source,
            "flight_id": flight_id,
            "model_id": model_id,
            "model_name": model_name,
        }),
    )
    .await?;
    Ok(())
}

/// Persist the log findings carried on the preview into `flight_log_findings`,
/// replacing any existing findings for the flight first. The DELETE makes this
/// idempotent and is what keeps the `.bin` UPGRADE path (which re-sources a
/// flight from a higher-precision log) from leaving stale/duplicated findings.
///
/// Findings originate from our own parser and are well-formed; a failure here
/// is genuinely exceptional, so it propagates rather than being swallowed.
async fn replace_flight_findings(
    pool: &DbPool,
    org_id: i32,
    flight_id: i32,
    preview: &serde_json::Value,
) -> Result<usize, AppError> {
    let findings: Vec<LogFinding> = match preview.get("findings") {
        Some(v) if !v.is_null() => serde_json::from_value(v.clone())
            .map_err(|e| AppError::BadRequest(format!("invalid findings payload: {e}")))?,
        _ => Vec::new(),
    };

    sqlx::query("DELETE FROM flight_log_findings WHERE flight_id = $1 AND org_id = $2")
        .bind(flight_id)
        .bind(org_id)
        .execute(pool)
        .await?;

    for f in &findings {
        sqlx::query(
            "INSERT INTO flight_log_findings \
             (org_id, flight_id, source, code, subsystem, subsys_id, ecode, severity, message, \
              occurrence_count, first_offset_seconds, last_offset_seconds, resolved) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13) \
             ON CONFLICT (flight_id, code) DO NOTHING",
        )
        .bind(org_id)
        .bind(flight_id)
        .bind(&f.source)
        .bind(&f.code)
        .bind(&f.subsystem)
        .bind(f.subsys_id)
        .bind(f.ecode)
        .bind(&f.severity)
        .bind(&f.message)
        .bind(f.occurrence_count)
        .bind(f.first_offset_seconds)
        .bind(f.last_offset_seconds)
        .bind(f.resolved)
        .execute(pool)
        .await?;
    }
    Ok(findings.len())
}

pub async fn commit_imported_flight(
    claims: Claims,
    State(pool): State<DbPool>,
    Json(body): Json<serde_json::Value>,
) -> Result<Response, AppError> {
    require_role(
        &claims,
        &["admin", "safety_officer", "technician", "pilot"],
    )?;

    let preview = body.get("preview")
        .ok_or_else(|| AppError::BadRequest("Missing preview data".into()))?;

    let source_filename = preview["source_filename"].as_str().unwrap_or("").to_string();
    let aircraft_id: Option<i32> = body["aircraft_id"].as_i64().map(|v| v as i32);
    let pilot_id: Option<i32> = body["pilot_id"].as_i64().map(|v| v as i32);
    let battery_id: Option<i32> = body["battery_id"].as_i64().map(|v| v as i32);
    let easa_category = body["easa_category"].as_str().unwrap_or("").to_string();
    let mission_type = body["mission_type"].as_str().map(|s| s.to_string());
    let remarks = body["remarks"].as_str().map(|s| s.to_string());
    let preflight_done = body["preflight_done"].as_bool().unwrap_or(false);

    // ArduPilot vs DJI: the same commit endpoint persists a different
    // log_source / log_precedence / flight_guid and a different aircraft
    // identifier source when the preview came from an ArduPilot .tlog.
    let app_source = preview["app_source"].as_str().unwrap_or("");
    let is_ardupilot = app_source == "ArduPilot";
    let (log_source, log_precedence) = resolve_log_source(body["log_source"].as_str(), app_source);
    let flight_guid: Option<String> = if is_ardupilot {
        preview["flight_guid"].as_str().filter(|s| !s.is_empty()).map(|s| s.to_string())
    } else {
        None
    };

    // FC-SN flow knobs — see services::fc_sn_classifier.
    // All three default to absent; old clients still parse.
    let link_existing_aircraft_id: Option<i32> =
        body["link_existing_aircraft_id"].as_i64().map(|v| v as i32);
    let force_register_new: bool = body["force_register_new"].as_bool().unwrap_or(false);
    let force_register_reason: Option<String> =
        body["force_register_reason"].as_str().map(|s| s.to_string());

    // Pre-flight FC-SN classification. Runs BEFORE the flight INSERT so a
    // refusal is a clean no-op on the caller side. Only fires for prone
    // models — `find_match` is a cheap SELECT we re-run inside the
    // aircraft branch later if we don't refuse here.
    //
    // Note: classify_commit_fc_sn validates `force_register_reason` length
    // up front, so a malformed force-register attempt errors out before
    // any DB work happens.
    let fc_sn_decision = classify_commit_fc_sn(
        preview["aircraft_model"].as_str().unwrap_or(""),
        link_existing_aircraft_id,
        force_register_new,
        force_register_reason.as_deref(),
    )?;

    if matches!(fc_sn_decision, FcSnCommitDecision::RefusePreflight) {
        let preview_sn = preview["aircraft_serial"].as_str().unwrap_or("");
        let preview_model_name = preview["aircraft_model"].as_str().unwrap_or("");
        if !preview_sn.is_empty() {
            let existing = find_match(&pool, "aircraft", claims.org_id, preview_sn).await?;
            if existing.is_none() {
                // No match → refuse. Candidates list is empty when the
                // model isn't in our catalog yet; the operator still
                // sees the 409 and has to opt-in via force_register_new.
                let preview_model_id = crate::services::aircraft_models::resolve_by_name_ci(
                    &pool,
                    claims.org_id,
                    preview_model_name,
                )
                .await?;
                let candidates = match preview_model_id {
                    Some(mid) => list_fc_sn_candidates(&pool, claims.org_id, mid).await?,
                    None => Vec::new(),
                };
                tracing::info!(
                    target: "fc_sn_classifier",
                    model = %preview_model_name,
                    sn = %preview_sn,
                    candidate_count = candidates.len(),
                    "refusing FC-SN-prone auto-register; returning 409 with candidates"
                );
                let resp = SuspiciousFcSnResponse {
                    kind: "suspicious_fc_sn",
                    sn: preview_sn.to_string(),
                    model_name: preview_model_name.to_string(),
                    candidates,
                };
                return Ok((StatusCode::CONFLICT, Json(resp)).into_response());
            }
        }
    }

    tracing::info!("commit_imported_flight: rc_serial={:?} battery_serial={:?} aircraft_serial={:?}",
        preview["rc_serial"].as_str(),
        preview["battery_serial"].as_str(),
        preview["aircraft_serial"].as_str()
    );

    if easa_category.is_empty() {
        return Err(AppError::BadRequest("EASA category is required".into()));
    }

    let duration = preview["duration_seconds"].as_i64().unwrap_or(0) as i32;
    let max_alt = preview["max_altitude_m"].as_f64().unwrap_or(0.0) as f32;
    let max_dist = preview["max_distance_m"].as_f64().unwrap_or(0.0) as f32;
    let total_dist = preview["total_distance_m"].as_f64().unwrap_or(0.0) as f32;
    let max_speed = preview["max_speed_ms"].as_f64().map(|v| v as f32);
    let home_lat = preview["home_lat"].as_f64().map(|v| v as f32);
    let home_lon = preview["home_lon"].as_f64().map(|v| v as f32);
    let min_sats = preview["satellite_count_min"].as_i64().map(|v| v as i32);
    let batt_temp = preview["battery_temp_c"].as_f64().map(|v| v as f32);
    let cell_variance = preview["cell_variance_v"].as_f64().map(|v| v as f32);
    let min_batt = preview["min_battery_pct"].as_i64().map(|v| v as i32);
    let aircraft_name = preview["aircraft_name"].as_str().map(|s| s.to_string());
    let aircraft_serial = preview["aircraft_serial"].as_str().map(|s| s.to_string());
    let trajectory = preview.get("trajectory").cloned();
    let flight_start = preview["flight_start"].as_str()
        .filter(|s| !s.starts_with("1970") && !s.is_empty())
        .map(|s| s.to_string());
    let flight_end = preview["flight_end"].as_str()
        .filter(|s| !s.starts_with("1970") && !s.is_empty())
        .map(|s| s.to_string());

    // ArduPilot best-effort merge, before the INSERT. A .bin is authoritative
    // and always wins over a .tlog of the same flight (precedence 20 > 10).
    // When a flight with the same board UID and an arm-time within a 10 s
    // window already exists:
    //   * incoming precedence higher  → UPGRADE the existing row in place:
    //     replace only the telemetry/log fields, preserving the operator's
    //     pilot / EASA / mission fields and the equipment links + flight id.
    //   * incoming precedence equal-or-lower (a re-import of the same .bin, or
    //     a .tlog arriving after its .bin) → reject as an already-imported
    //     duplicate (the previous hard-reject behaviour).
    // No matching flight → fall through to the normal INSERT below.
    if let Some(g) = flight_guid.as_deref() {
        if let Some((uid, incoming_arm)) = parse_ardupilot_guid(g) {
            let like = format!("ardupilot:{uid}:%");
            let candidates = sqlx::query_as::<_, (i32, Option<i16>, Option<String>)>(
                "SELECT id, log_precedence, flight_guid FROM flights \
                 WHERE org_id = $1 AND deleted_at IS NULL AND flight_guid LIKE $2",
            )
            .bind(claims.org_id)
            .bind(&like)
            .fetch_all(&pool)
            .await?;

            // Closest existing arm-time within the merge window wins.
            let mut best: Option<(i32, i16)> = None;
            let mut best_delta = i64::MAX;
            for (id, prec, guid_opt) in candidates {
                if let Some((_, existing_arm)) =
                    guid_opt.as_deref().and_then(parse_ardupilot_guid)
                {
                    if within_arm_window(incoming_arm, existing_arm) {
                        let delta = (incoming_arm - existing_arm).abs();
                        if delta < best_delta {
                            best_delta = delta;
                            best = Some((id, prec.unwrap_or(0)));
                        }
                    }
                }
            }

            if let Some((existing_id, existing_prec)) = best {
                let incoming_prec = log_precedence.unwrap_or(0);
                if incoming_prec <= existing_prec {
                    return Err(AppError::BadRequest(
                        "This flight segment has already been imported.".into(),
                    ));
                }

                // UPGRADE path — bin wins. Snapshot before/after for the audit
                // diff; the UPDATE touches only telemetry/log columns, so
                // pilot_id, easa_category, mission_type, remarks, preflight_done
                // and the aircraft/battery/controller links are preserved.
                let before = sqlx::query_as::<_, Flight>(
                    "SELECT * FROM flights WHERE id = $1 AND org_id = $2",
                )
                .bind(existing_id)
                .bind(claims.org_id)
                .fetch_one(&pool)
                .await?;

                sqlx::query(
                    r#"UPDATE flights SET
                         duration_seconds = $1, max_altitude_m = $2, max_distance_m = $3,
                         total_distance_m = $4, max_speed_ms = $5, home_lat = $6, home_lon = $7,
                         satellite_count_min = $8, battery_temp_c = $9, cell_variance_v = $10,
                         min_battery_pct = $11, trajectory_json = $12,
                         flight_start = $13::timestamptz, flight_end = $14::timestamptz,
                         flight_datetime = COALESCE($15::timestamptz, NOW()),
                         source_filename = $16, aircraft_name = $17, aircraft_serial = $18,
                         log_source = $19, log_precedence = $20, flight_guid = $21
                       WHERE id = $22 AND org_id = $23"#,
                )
                .bind(duration)
                .bind(max_alt)
                .bind(max_dist)
                .bind(total_dist)
                .bind(max_speed)
                .bind(home_lat)
                .bind(home_lon)
                .bind(min_sats)
                .bind(batt_temp)
                .bind(cell_variance)
                .bind(min_batt)
                .bind(
                    trajectory
                        .clone()
                        .map(|t| serde_json::to_value(t).unwrap_or(serde_json::Value::Null)),
                )
                .bind(&flight_start)
                .bind(&flight_end)
                .bind(&flight_start)
                .bind(&source_filename)
                .bind(&aircraft_name)
                .bind(&aircraft_serial)
                .bind(&log_source)
                .bind(log_precedence)
                .bind(&flight_guid)
                .bind(existing_id)
                .bind(claims.org_id)
                .execute(&pool)
                .await
                .map_err(|e| {
                    tracing::error!("commit_imported_flight upgrade error: {:?}", e);
                    AppError::Database(e)
                })?;

                let after = sqlx::query_as::<_, Flight>(
                    "SELECT * FROM flights WHERE id = $1 AND org_id = $2",
                )
                .bind(existing_id)
                .bind(claims.org_id)
                .fetch_one(&pool)
                .await?;

                // Adjust the linked aircraft's duration counter for the delta.
                // total_flights is unchanged — it is the same flight, re-sourced
                // from a higher-precision log. Best-effort, matching the INSERT
                // path's non-transactional counter bump.
                if let Some(ac_id) = after.aircraft_id {
                    let delta = after.duration_seconds - before.duration_seconds;
                    let res = sqlx::query(
                        "UPDATE aircraft SET
                           total_seconds = total_seconds + $1,
                           last_flight = GREATEST(COALESCE(last_flight, 'epoch'::timestamptz), $2)
                         WHERE id = $3 AND org_id = $4",
                    )
                    .bind(delta)
                    .bind(after.flight_datetime)
                    .bind(ac_id)
                    .bind(claims.org_id)
                    .execute(&pool)
                    .await;
                    if let Err(e) = res {
                        tracing::error!(
                            error = ?e,
                            aircraft_id = %ac_id,
                            flight_id = %existing_id,
                            "failed to adjust aircraft counters after .bin upgrade"
                        );
                    }
                }

                audit::log_update_diff(
                    &pool,
                    claims.org_id,
                    claims.user_id,
                    &claims.user_code,
                    "flights",
                    existing_id,
                    &before,
                    &after,
                )
                .await;

                // Re-source findings from the higher-precision .bin: DELETE the
                // .tlog's findings and re-insert, so they don't go stale.
                replace_flight_findings(&pool, claims.org_id, existing_id, preview).await?;

                return Ok(Json(serde_json::json!({
                    "id": existing_id,
                    "message": "Flight upgraded to .bin (higher-precision log)",
                    "upgraded": true,
                }))
                .into_response());
            }
        }
    }

    let row = sqlx::query(
        r#"INSERT INTO flights
           (org_id, pilot_id, aircraft_id, battery_id, controller_id, source_filename, duration_seconds,
            max_altitude_m, max_distance_m, total_distance_m, max_speed_ms, home_lat, home_lon,
            satellite_count_min, battery_temp_c, cell_variance_v, min_battery_pct,
            aircraft_name, aircraft_serial, easa_category, mission_type,
            remarks, preflight_done, log_source, trajectory_json,
            flight_datetime, flight_start, flight_end, flight_guid, log_precedence)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22,$23,$24,$25,
            COALESCE($26::timestamptz, NOW()), $27::timestamptz, $28::timestamptz, $29, $30)
           RETURNING id"#
    )
    .bind(claims.org_id)
    .bind(pilot_id)
    .bind(aircraft_id)
    .bind(battery_id)
    .bind(body["controller_id"].as_i64().map(|v| v as i32))
    .bind(&source_filename)
    .bind(duration)
    .bind(max_alt)
    .bind(max_dist)
    .bind(total_dist)
    .bind(max_speed)
    .bind(home_lat)
    .bind(home_lon)
    .bind(min_sats)
    .bind(batt_temp)
    .bind(cell_variance)
    .bind(min_batt)
    .bind(&aircraft_name)
    .bind(&aircraft_serial)
    .bind(&easa_category)
    .bind(&mission_type)
    .bind(&remarks)
    .bind(preflight_done)
    .bind(&log_source)
    .bind(trajectory.map(|t| serde_json::to_value(t).unwrap_or(serde_json::Value::Null)))
    .bind(&flight_start)
    .bind(&flight_start)
    .bind(&flight_end)
    .bind(&flight_guid)
    .bind(log_precedence)
    .fetch_one(&pool)
    .await
    .map_err(|e| {
        tracing::error!("commit_imported_flight error: {:?}", e);
        AppError::Database(e)
    })?;

    use sqlx::Row;
    let flight_id: i32 = row.get("id");

    // Denormalize aircraft name if aircraft_id provided
    if let Some(ac_id) = aircraft_id {
        let _ = sqlx::query(
            "UPDATE flights SET aircraft_name = COALESCE(aircraft_name, \
                (SELECT COALESCE(a.nickname, am.name, a.serial_number) \
                 FROM aircraft a LEFT JOIN aircraft_models am ON a.model_id = am.id \
                 WHERE a.id = $1 AND a.org_id = $2)) WHERE id = $3"
        )
        .bind(ac_id)
        .bind(claims.org_id)
        .bind(flight_id)
        .execute(&pool)
        .await;
    }

    // Persist log findings carried on the preview (ArduPilot .bin today; empty
    // for DJI/CSV/.tlog). Independent of the equipment auto-register below.
    replace_flight_findings(&pool, claims.org_id, flight_id, preview).await?;

    // Auto-register aircraft if requested and not already present
    if body["auto_register_aircraft"].as_bool().unwrap_or(false) {
        let new_serial = body["new_aircraft_serial"].as_str().unwrap_or("");
        // new_aircraft_name is the DJI app's operator-set device label (e.g.
        // "Old Faithful"). Used ONLY as the nickname, never as a catalog key —
        // operators commonly rename to non-model strings.
        let new_name = body["new_aircraft_name"].as_str().unwrap_or("");
        // new_aircraft_model is the canonical aircraft_models.name resolved
        // by the parser from drone_type_code (osd.drone_type /
        // details.product_type). Used as the catalog lookup key. Empty/
        // unknown → model_id = NULL so an admin can resolve manually.
        let new_model = body["new_aircraft_model"].as_str().unwrap_or("");
        if !new_serial.is_empty() {
            // Labeled block lets SUB-BRANCH A short-circuit the find_match
            // + auto-register flow without re-indenting the existing code.
            // Break exits this block; control resumes at the battery section.
            'aircraft_branch: {
            // SUB-BRANCH A: operator picked an existing aircraft from the
            // 409 candidate list. Verify the link target, write an fc_sn
            // identifier, link the flight, audit LINK_FC_SN_IDENTIFIER —
            // then skip the rest of the aircraft branch entirely.
            if let Some(link_id) = link_existing_aircraft_id {
                let preview_model_id: Option<i32> = if new_model.trim().is_empty() {
                    None
                } else {
                    crate::services::aircraft_models::resolve_by_name_ci(
                        &pool, claims.org_id, new_model.trim(),
                    )
                    .await?
                };
                verify_link_target(&pool, claims.org_id, link_id, preview_model_id).await?;
                perform_link_existing(
                    &pool,
                    claims.org_id,
                    claims.user_id,
                    &claims.user_code,
                    link_id,
                    new_serial,
                    new_model,
                    preview_model_id,
                    flight_id,
                    aircraft_log_identifier_source(is_ardupilot),
                )
                .await?;
                break 'aircraft_branch;
            }

            // Exact-then-prefix match via the same helper the import-preview
            // handler uses, so the preview's `aircraft_match` indicator
            // can never diverge from what we actually do here. PrefixAmbiguous
            // and "no match" both surface as None and fall through to
            // auto-register; tracing happens inside find_match.
            let existing_id = find_match(&pool, "aircraft", claims.org_id, new_serial)
                .await?
                .map(|m| m.id);

            if let Some(ac_id) = existing_id {
                // Link flight to the matched aircraft so the import does not
                // leave the flight orphaned even when SN was truncated.
                let _ = sqlx::query("UPDATE flights SET aircraft_id = $1 WHERE id = $2")
                    .bind(ac_id)
                    .bind(flight_id)
                    .execute(&pool)
                    .await;
            } else {
                // Strict catalog match against the canonical model name from
                // the parser. No create — if the drone-type variant has no
                // mapping or the catalog row is missing, model_id ends up
                // NULL and an admin can fix it via the aircraft-edit UI.
                // This is intentional: the previous resolve_or_create path
                // accepted operator-typed device labels and created junk
                // catalog rows on every novel name.
                let model_id: Option<i32> = if new_model.trim().is_empty() {
                    None
                } else {
                    crate::services::aircraft_models::resolve_by_name_ci(
                        &pool, claims.org_id, new_model.trim(),
                    ).await?
                };

                // Generate the next UAV-NNN id so auto-registered aircraft
                // get the same identifier shape as QR-batch and manually-
                // created rows. Race profile matches batch_import (see
                // services::equipment_ids).
                let equipment_id =
                    next_equipment_id(&pool, claims.org_id, "aircraft", "UAV-").await?;
                tracing::info!(
                    target: "auto_register",
                    sn = %new_serial,
                    equipment_id = %equipment_id,
                    table = "aircraft",
                    "auto-registered new equipment from flight import"
                );

                // Insert, then re-SELECT with the JOIN so the audit row
                // carries the canonical `model` name alongside `model_id`.
                //
                // `needs_manual_sn = TRUE` because the SN didn't match any
                // existing aircraft (canonical body SN, identifiers table,
                // or 2-char prefix tolerance). On Inspire 1 / Matrice 600
                // / similar the log carries the flight-controller SN, not
                // the chassis body SN, so this row may turn out to be a
                // duplicate of an already-registered aircraft. Operators
                // confirm or merge via the pending-review UI; the flag is
                // auto-cleared by the aircraft-update handler when the SN
                // is hand-corrected.
                use sqlx::Row;
                let inserted = sqlx::query(
                    "INSERT INTO aircraft (org_id, serial_number, equipment_id, model_id, nickname, status, needs_manual_sn) \
                     VALUES ($1,$2,$3,$4,$5,'active',TRUE) RETURNING id"
                )
                .bind(claims.org_id)
                .bind(new_serial)
                .bind(&equipment_id)
                .bind(model_id)
                .bind(new_name)
                .fetch_one(&pool)
                .await?;
                let new_ac_id: i32 = inserted.get("id");

                let ac_row = sqlx::query_as::<_, Aircraft>(
                    "SELECT a.*, am.name AS model FROM aircraft a \
                     LEFT JOIN aircraft_models am ON a.model_id = am.id \
                     WHERE a.id = $1 AND a.org_id = $2"
                )
                .bind(new_ac_id)
                .bind(claims.org_id)
                .fetch_one(&pool)
                .await?;

                let _ = sqlx::query(
                    "UPDATE flights SET aircraft_id = $1, aircraft_name = $2 WHERE id = $3"
                )
                .bind(new_ac_id)
                .bind(new_name)
                .bind(flight_id)
                .execute(&pool)
                .await;

                audit::log_insert_full(&pool, claims.org_id, claims.user_id, &claims.user_code, "aircraft", new_ac_id, &ac_row).await;

                // Best-effort: record identifier rows so future imports of
                // the same aircraft match via the identifiers table even
                // when the log SN differs from the body SN (DJI Inspire 1
                // / Matrice scenario where the FC SN in the log shares no
                // common prefix with the body SN). Failures are logged but
                // not propagated — the flight + aircraft inserts above are
                // the primary side-effects; identifier rows are best-effort
                // denormalisation that the next import can rebuild from
                // the same log file.
                if is_ardupilot {
                    // ArduPilot board UID is a stable serial-number analogue,
                    // not an FC SN — record it under 'ardupilot_uid' so future
                    // .tlog imports of the same airframe match via the
                    // identifiers table. needs_manual_sn stays TRUE (above) so
                    // the operator still confirms/merges to a physical airframe
                    // via the pending-review flow, consistent with DJI.
                    let observed_in = format!("flight:{flight_id}");
                    let _ = crate::services::aircraft_identifiers::link_identifier(
                        &pool,
                        new_ac_id,
                        claims.org_id,
                        new_serial,
                        crate::services::aircraft_identifiers::SOURCE_ARDUPILOT_UID,
                        Some(&observed_in),
                        Some(claims.user_id),
                    )
                    .await;
                } else {
                    crate::services::aircraft_identifiers::record_auto_register_identifiers(
                        &pool,
                        new_ac_id,
                        claims.org_id,
                        new_serial,
                        aircraft_name.as_deref(),
                        preview["app_source"].as_str(),
                        flight_id,
                        Some(claims.user_id),
                        // from_unmatched_log = true: the SN reached this branch
                        // because find_match returned None, i.e. it didn't match
                        // any existing aircraft body SN, identifier, or prefix.
                        // Classify as fc_sn so the pending-review UI can route
                        // it through the FC-vs-body confirmation flow.
                        true,
                    ).await;
                }

                // Seed EASA default maintenance schedules for the newly
                // auto-registered aircraft. The resolve_or_create path
                // above always populates model_id, so the function
                // typically returns Inserted(n); SkippedNoTemplates is
                // possible if the model is brand-new and has no templates
                // seeded yet. Wrapped in a short-lived transaction so
                // partial inserts never leak on failure.
                let mut seed_tx = pool.begin().await?;
                crate::services::maintenance_defaults::insert_defaults_for_aircraft(
                    &mut seed_tx,
                    claims.org_id,
                    new_ac_id,
                    ac_row.model_id,
                    &claims.user_code,
                )
                .await?;
                seed_tx.commit().await?;

                // SUB-BRANCH B: extra audit when the operator explicitly
                // forced auto-register past the FC-SN refusal. The standard
                // INSERT audit a few lines up already captures the row;
                // this entry adds the operator's written reason under a
                // distinctive action name. Gated on is_fc_sn_prone_model
                // so the audit stays truthful when the flag is set on a
                // non-prone model (a no-op there — the preflight never
                // refused, so there was nothing to override).
                if force_register_new && is_fc_sn_prone_model(new_model) {
                    audit::log_custom_action(
                        &pool,
                        claims.org_id,
                        claims.user_id,
                        &claims.user_code,
                        "FORCE_REGISTER_FC_SN_PRONE",
                        "aircraft",
                        new_ac_id,
                        serde_json::json!({
                            "sn": new_serial,
                            "model_name": new_model,
                            "reason": force_register_reason,
                        }),
                    )
                    .await?;
                }
            }
            } // close 'aircraft_branch
        }
    }

    // Auto-register battery if requested
    tracing::info!("Checking battery registration...");
    if body["auto_register_battery"].as_bool().unwrap_or(false) {
        let bat_serial = body["new_battery_serial"].as_str().unwrap_or("");
        // batteries.compatible_model is free-text; the frontend now sends
        // the canonical aircraft model (Mavic Pro / Mavic Air / …) rather
        // than the DJI app's device label that previously leaked in via
        // line 248 of ImportFlightLogModal. Fall back to new_aircraft_model
        // if the frontend's older shape (no new_battery_model) is in flight.
        let bat_model = body["new_battery_model"]
            .as_str()
            .filter(|s| !s.is_empty())
            .or_else(|| body["new_aircraft_model"].as_str())
            .unwrap_or("");
        if !bat_serial.is_empty() {
            // Same helper as the aircraft branch + preview — see services::sn_matching.
            let existing_bat_id = find_match(&pool, "batteries", claims.org_id, bat_serial)
                .await?
                .map(|m| m.id);

            if let Some(bat_id) = existing_bat_id {
                let _ = sqlx::query("UPDATE flights SET battery_id = $1 WHERE id = $2")
                    .bind(bat_id)
                    .bind(flight_id)
                    .execute(&pool)
                    .await;
            } else {
                // Auto-registered batteries are inserted with aircraft_id =
                // NULL. Pre-012 we tried to link by free-text model name on
                // aircraft.model, but that column was dropped by migration
                // 012 and the lookup was fragile anyway (multiple aircraft
                // can share a model; first-match was arbitrary). Operators
                // can link the battery to an aircraft later via the battery
                // edit UI.
                let equipment_id =
                    next_equipment_id(&pool, claims.org_id, "batteries", "BAT-").await?;
                tracing::info!(
                    target: "auto_register",
                    sn = %bat_serial,
                    equipment_id = %equipment_id,
                    table = "batteries",
                    "auto-registered new equipment from flight import"
                );

                // needs_manual_sn = TRUE: the log SN didn't match any
                // existing battery. dji-log-parser SN truncation and
                // firmware-side SNs both produce ghost rows; the
                // pending-review flow (downstream PR) lets operators
                // confirm or merge. Until that ships, operators dedupe
                // manually.
                // Opportunistic structured-compat match (migration 052): exact
                // case-insensitive name match within the org. Unmatched stays NULL —
                // auto-register must never fail on an unknown model label.
                let compat_model_id: Option<i32> = if bat_model.trim().is_empty() {
                    None
                } else {
                    sqlx::query_scalar(
                        "SELECT id FROM aircraft_models WHERE org_id = $1 AND LOWER(btrim(name)) = LOWER(btrim($2)) LIMIT 1",
                    )
                    .bind(claims.org_id)
                    .bind(bat_model)
                    .fetch_optional(&pool)
                    .await?
                };

                let bat_row = sqlx::query_as::<_, Battery>(
                    "INSERT INTO batteries (org_id, serial_number, equipment_id, compatible_model, compatible_model_id, status, needs_manual_sn) \
                     VALUES ($1,$2,$3,$4,$5,'active',TRUE) RETURNING *"
                )
                .bind(claims.org_id)
                .bind(bat_serial)
                .bind(&equipment_id)
                .bind(bat_model)
                .bind(compat_model_id)
                .fetch_one(&pool)
                .await?;

                let new_bat_id = bat_row.id;
                let _ = sqlx::query(
                    "UPDATE flights SET battery_id = $1 WHERE id = $2"
                )
                .bind(new_bat_id)
                .bind(flight_id)
                .execute(&pool)
                .await;

                audit::log_insert_full(&pool, claims.org_id, claims.user_id, &claims.user_code, "batteries", new_bat_id, &bat_row).await;

                // Best-effort identifier rows so future imports of the
                // same battery via its body SN can match through
                // battery_identifiers (mig 030). No alias parameter — DJI
                // logs don't carry an operator-set name for batteries.
                // from_unmatched_log = true → SN classified as fc_sn.
                crate::services::battery_identifiers::record_auto_register_identifiers(
                    &pool,
                    new_bat_id,
                    claims.org_id,
                    bat_serial,
                    None,
                    preview["app_source"].as_str(),
                    flight_id,
                    Some(claims.user_id),
                    true,
                ).await;
            }
        }
    }

    // Auto-register controller if serial present
    tracing::info!("Checking controller registration...");
    if let Some(rc_serial) = preview["rc_serial"].as_str().filter(|s| !s.is_empty()) {
        // Same helper as the aircraft / battery branches + preview.
        let existing_rc_id = find_match(&pool, "remote_controllers", claims.org_id, rc_serial)
            .await?
            .map(|m| m.id);

        let rc_id = if let Some(id) = existing_rc_id {
            id
        } else {
            // Try to find matching aircraft for linking. Route through
            // find_match so Inspire-class FC-SN logs (where the log's
            // aircraft_serial is the FC SN, not the body SN) still resolve
            // via aircraft_identifiers. The previous direct
            // LOWER(TRIM(serial_number)) lookup missed those — observed in a
            // production database: a controller was left orphaned
            // (aircraft_id = NULL) even though its flights pointed at an
            // existing aircraft.
            let aircraft_id_for_rc: Option<i32> = match preview["aircraft_serial"].as_str() {
                Some(s) if !s.is_empty() => find_match(&pool, "aircraft", claims.org_id, s)
                    .await?
                    .map(|m| m.id),
                _ => None,
            };

            let equipment_id =
                next_equipment_id(&pool, claims.org_id, "remote_controllers", "CTL-").await?;
            tracing::info!(
                target: "auto_register",
                sn = %rc_serial,
                equipment_id = %equipment_id,
                table = "remote_controllers",
                "auto-registered new equipment from flight import"
            );

            // needs_manual_sn = TRUE: the log SN didn't match any existing
            // controller (canonical body SN, identifiers table, or 2-char
            // prefix tolerance). On M600 / Inspire-class hardware the log
            // carries the FC SN, not the chassis body SN, so this row may
            // turn out to be a duplicate of an already-registered controller.
            // A downstream PR will surface these as pending-review for
            // operator confirm/merge; until then operators dedupe manually.
            let new_rc = sqlx::query_as::<_, Controller>(
                "INSERT INTO remote_controllers (org_id, serial_number, equipment_id, status, aircraft_id, needs_manual_sn)
                 VALUES ($1, $2, $3, 'active', $4, TRUE) RETURNING *"
            )
            .bind(claims.org_id)
            .bind(rc_serial)
            .bind(&equipment_id)
            .bind(aircraft_id_for_rc)
            .fetch_one(&pool)
            .await?;

            let new_rc_id = new_rc.id;
            audit::log_insert_full(&pool, claims.org_id, claims.user_id, &claims.user_code, "remote_controllers", new_rc_id, &new_rc).await;

            // Best-effort identifier rows so future imports of the same
            // controller via its body SN can match through
            // controller_identifiers (mig 030). No alias parameter — DJI
            // logs don't carry an operator-set name for controllers (unlike
            // aircraft, where aircraft_name carries the DJI app device
            // label). from_unmatched_log = true → SN classified as fc_sn
            // so the pending-review flow can route it through the
            // FC-vs-body confirmation UI when that ships.
            crate::services::controller_identifiers::record_auto_register_identifiers(
                &pool,
                new_rc_id,
                claims.org_id,
                rc_serial,
                None,
                preview["app_source"].as_str(),
                flight_id,
                Some(claims.user_id),
                true,
            ).await;

            new_rc_id
        };

        // Update flight with controller_id
        let _ = sqlx::query("UPDATE flights SET controller_id = $1 WHERE id = $2")
            .bind(rc_id)
            .bind(flight_id)
            .execute(&pool)
            .await;
    }

    // Save drone type mapping for learning system — keyed by model_id.
    // Resolution is strict: we look up the canonical model name (from the
    // parser's drone_type_code mapping) against the catalog, and skip the
    // mapping save entirely on miss. Previously this path used
    // resolve_or_create on `preview.aircraft_name` (the DJI app's
    // operator-set label), which created junk catalog rows for every novel
    // device name and defeated the auto-register fix above.
    if let (Some(type_code), Some(app_src), Some(model_name)) = (
        preview["drone_type_code"].as_str().filter(|s| !s.is_empty()),
        preview["app_source"].as_str().filter(|s| !s.is_empty()),
        preview["aircraft_model"].as_str().filter(|s| !s.is_empty()),
    ) {
        let mapping_model_id = crate::services::aircraft_models::resolve_by_name_ci(
            &pool, claims.org_id, model_name,
        ).await?;
        if let Some(mid) = mapping_model_id {
            let _ = sqlx::query(
                "INSERT INTO drone_type_mappings (org_id, app_source, drone_type_code, model_id, confirmed_by_user)
                 VALUES ($1, $2, $3, $4, true)
                 ON CONFLICT (org_id, app_source, drone_type_code)
                 DO UPDATE SET model_id = EXCLUDED.model_id,
                               confirmed_by_user = true,
                               updated_at = NOW()"
            )
            .bind(claims.org_id)
            .bind(app_src)
            .bind(type_code)
            .bind(mid)
            .execute(&pool)
            .await;
        }
    }

    // Refetch after side-effect UPDATEs above (aircraft/battery/controller auto-link)
    // so the audit snapshot reflects the final committed state.
    match sqlx::query_as::<_, Flight>("SELECT * FROM flights WHERE id = $1")
        .bind(flight_id)
        .fetch_one(&pool)
        .await
    {
        Ok(full) => {
            // Bump the denormalised aircraft counters using the final
            // aircraft_id from the refetch (auto-register may have
            // rewritten it after the original INSERT, so bumping any
            // earlier would credit the wrong row). Best-effort: a
            // counter UPDATE failure must not roll back a successful
            // import — the surrounding side-effect UPDATEs in this
            // handler all use the same non-transactional pattern. We
            // log the error so the drift is visible in ops.
            if let Some(ac_id) = full.aircraft_id {
                let res = sqlx::query(
                    "UPDATE aircraft SET
                       total_flights = total_flights + 1,
                       total_seconds = total_seconds + $1,
                       last_flight   = GREATEST(COALESCE(last_flight, 'epoch'::timestamptz), $2)
                     WHERE id = $3 AND org_id = $4"
                )
                .bind(full.duration_seconds)
                .bind(full.flight_datetime)
                .bind(ac_id)
                .bind(claims.org_id)
                .execute(&pool)
                .await;

                if let Err(e) = res {
                    tracing::error!(
                        error = ?e,
                        aircraft_id = %ac_id,
                        flight_id = %full.id,
                        "failed to bump aircraft flight counters after DJI commit"
                    );
                }
            }

            audit::log_insert_full(&pool, claims.org_id, claims.user_id, &claims.user_code, "flights", flight_id, &full).await;
        }
        Err(e) => {
            // Post-insert fetch failed — rare. Skip the audit row; tracing::warn
            // surfaces the anomaly so ops can reconstruct if ever needed.
            tracing::warn!("audit: failed to fetch imported flight {flight_id} for snapshot: {e:?}");
        }
    }

    Ok(Json(serde_json::json!({
        "id": flight_id,
        "message": "Flight imported successfully"
    }))
    .into_response())
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::{PgPool, Row};

    // ---- Pure helper tests (no DB) ----

    #[test]
    fn resolve_log_source_ardupilot_default_is_tlog_tier() {
        assert_eq!(
            resolve_log_source(None, "ArduPilot"),
            ("ardupilot_tlog".to_string(), Some(10))
        );
    }

    #[test]
    fn resolve_log_source_explicit_bin_overrides_to_tier_20() {
        assert_eq!(
            resolve_log_source(Some("ardupilot_bin"), "ArduPilot"),
            ("ardupilot_bin".to_string(), Some(20))
        );
    }

    #[test]
    fn resolve_log_source_empty_body_value_ignored() {
        assert_eq!(
            resolve_log_source(Some(""), "ArduPilot"),
            ("ardupilot_tlog".to_string(), Some(10))
        );
    }

    #[test]
    fn resolve_log_source_dji_default_has_null_precedence() {
        assert_eq!(
            resolve_log_source(None, "DJIFly"),
            ("dji_import".to_string(), None)
        );
    }

    #[test]
    fn resolve_log_source_explicit_dji_import_keeps_null_precedence() {
        assert_eq!(
            resolve_log_source(Some("dji_import"), "DJIFly"),
            ("dji_import".to_string(), None)
        );
    }

    #[test]
    fn aircraft_log_identifier_source_branches() {
        assert_eq!(aircraft_log_identifier_source(true), "ardupilot_uid");
        assert_eq!(aircraft_log_identifier_source(false), "fc_sn");
    }

    #[test]
    fn parse_ardupilot_guid_round_trips() {
        assert_eq!(
            parse_ardupilot_guid("ardupilot:ABC123:1783501442"),
            Some(("ABC123".to_string(), 1_783_501_442))
        );
    }

    #[test]
    fn parse_ardupilot_guid_rejects_malformed() {
        // Wrong prefix, missing arm-time, empty uid, non-numeric arm-time.
        assert_eq!(parse_ardupilot_guid("dji:ABC123:1"), None);
        assert_eq!(parse_ardupilot_guid("ardupilot:ABC123"), None);
        assert_eq!(parse_ardupilot_guid("ardupilot::1783501442"), None);
        assert_eq!(parse_ardupilot_guid("ardupilot:ABC123:notanumber"), None);
    }

    #[test]
    fn within_arm_window_bounds() {
        assert!(within_arm_window(1_783_501_442, 1_783_501_442)); // identical
        assert!(within_arm_window(1_783_501_442, 1_783_501_452)); // +10 s edge
        assert!(within_arm_window(1_783_501_452, 1_783_501_442)); // -10 s edge
        assert!(!within_arm_window(1_783_501_442, 1_783_501_453)); // +11 s out
    }

    // ---- Shared test seed helpers ----

    async fn default_org_id(pool: &PgPool) -> i32 {
        let row = sqlx::query("SELECT id FROM organisations ORDER BY id LIMIT 1")
            .fetch_one(pool)
            .await
            .expect("default org seeded by migration 002");
        row.get::<i32, _>("id")
    }

    async fn default_admin_user(pool: &PgPool) -> (i32, String) {
        let row = sqlx::query(
            "SELECT id, user_code FROM users WHERE role = 'admin' ORDER BY id LIMIT 1",
        )
        .fetch_one(pool)
        .await
        .expect("default admin seeded by migration 002");
        (row.get::<i32, _>("id"), row.get::<String, _>("user_code"))
    }

    /// Idempotent over the case-insensitive name index — returns the
    /// existing id if migration 012 (or another test in the same run)
    /// already seeded the row.
    async fn seed_model(pool: &PgPool, org_id: i32, name: &str) -> i32 {
        let existing = sqlx::query(
            "SELECT id FROM aircraft_models WHERE org_id = $1 AND LOWER(name) = LOWER($2)",
        )
        .bind(org_id)
        .bind(name)
        .fetch_optional(pool)
        .await
        .expect("model lookup")
        .map(|r| r.get::<i32, _>("id"));
        if let Some(id) = existing {
            return id;
        }
        let row = sqlx::query(
            "INSERT INTO aircraft_models (org_id, name) VALUES ($1, $2) RETURNING id",
        )
        .bind(org_id)
        .bind(name)
        .fetch_one(pool)
        .await
        .expect("model insert");
        row.get::<i32, _>("id")
    }

    async fn seed_aircraft_with_flags(
        pool: &PgPool,
        org_id: i32,
        serial: &str,
        equipment_id: &str,
        model_id: Option<i32>,
        needs_manual_sn: bool,
    ) -> i32 {
        let row = sqlx::query(
            "INSERT INTO aircraft (org_id, serial_number, equipment_id, model_id, status, needs_manual_sn) \
             VALUES ($1, $2, $3, $4, 'active', $5) RETURNING id",
        )
        .bind(org_id)
        .bind(serial)
        .bind(equipment_id)
        .bind(model_id)
        .bind(needs_manual_sn)
        .fetch_one(pool)
        .await
        .expect("aircraft insert");
        row.get::<i32, _>("id")
    }

    fn preview_with_findings(codes: &[(&str, &str, &str)]) -> serde_json::Value {
        // (code, severity, message) → findings array in preview shape.
        let arr: Vec<serde_json::Value> = codes
            .iter()
            .map(|(code, sev, msg)| {
                serde_json::json!({
                    "source": "ardupilot_bin",
                    "code": code,
                    "subsystem": "TEST",
                    "subsys_id": 6,
                    "ecode": 1,
                    "severity": sev,
                    "message": msg,
                    "occurrence_count": 2,
                    "first_offset_seconds": 12.5,
                    "last_offset_seconds": 34.0,
                    "resolved": false
                })
            })
            .collect();
        serde_json::json!({ "findings": arr })
    }

    /// commit persists preview findings into flight_log_findings.
    #[sqlx::test(migrations = "./migrations")]
    async fn test_commit_persists_findings(pool: PgPool) {
        let org_id = default_org_id(&pool).await;
        let flight_id = seed_minimal_flight(&pool, org_id).await;
        let preview = preview_with_findings(&[
            ("err.failsafe_batt.1", "high", "Battery failsafe triggered"),
            ("vibe.high", "high", "Vibration high"),
        ]);

        let n = replace_flight_findings(&pool, org_id, flight_id, &preview)
            .await
            .unwrap();
        assert_eq!(n, 2);

        let rows: Vec<(String, String, i32)> = sqlx::query_as(
            "SELECT code, severity, occurrence_count FROM flight_log_findings \
             WHERE flight_id = $1 ORDER BY code",
        )
        .bind(flight_id)
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].0, "err.failsafe_batt.1");
        assert_eq!(rows[0].1, "high");
        assert_eq!(rows[0].2, 2);
        assert_eq!(rows[1].0, "vibe.high");
    }

    /// The .bin upgrade path re-runs replace_flight_findings; it must REPLACE
    /// (delete + re-insert) rather than duplicate or leave stale rows.
    #[sqlx::test(migrations = "./migrations")]
    async fn test_upgrade_replaces_findings_not_duplicates(pool: PgPool) {
        let org_id = default_org_id(&pool).await;
        let flight_id = seed_minimal_flight(&pool, org_id).await;

        // First import: two findings.
        let first = preview_with_findings(&[
            ("err.failsafe_batt.1", "high", "batt"),
            ("ev.ekf_yaw_reset", "medium", "yaw"),
        ]);
        replace_flight_findings(&pool, org_id, flight_id, &first)
            .await
            .unwrap();

        // Upgrade: the higher-precision .bin yields a DIFFERENT set (one shared
        // code, one new). Must end with exactly the upgrade's findings.
        let upgrade = preview_with_findings(&[
            ("err.failsafe_batt.1", "high", "batt"),
            ("err.thrust_loss_check.1", "high", "thrust"),
        ]);
        replace_flight_findings(&pool, org_id, flight_id, &upgrade)
            .await
            .unwrap();

        let codes: Vec<String> = sqlx::query_scalar(
            "SELECT code FROM flight_log_findings WHERE flight_id = $1 ORDER BY code",
        )
        .bind(flight_id)
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(
            codes,
            vec![
                "err.failsafe_batt.1".to_string(),
                "err.thrust_loss_check.1".to_string()
            ],
            "upgrade must replace findings, not duplicate or leave stale rows"
        );
    }

    async fn seed_minimal_flight(pool: &PgPool, org_id: i32) -> i32 {
        let row = sqlx::query(
            "INSERT INTO flights (org_id, source_filename, log_source) \
             VALUES ($1, $2, 'dji_import') RETURNING id",
        )
        .bind(org_id)
        .bind("test_seed.csv")
        .fetch_one(pool)
        .await
        .expect("flight insert");
        row.get::<i32, _>("id")
    }

    // ---- Test 8 ----

    /// The candidate list shown in the 409 body must include only
    /// verified aircraft of the matching model — pending-review rows
    /// would propagate the "is this a ghost?" ambiguity onto the
    /// operator's UI.
    #[sqlx::test(migrations = "./migrations")]
    async fn test_candidates_query_returns_only_verified_aircraft_for_model(pool: PgPool) {
        let org_id = default_org_id(&pool).await;
        let model_id = seed_model(&pool, org_id, "TestModel_CandidatesQuery").await;

        let verified_id = seed_aircraft_with_flags(
            &pool, org_id, "T8_VERIFIED_001", "UAV-T8A", Some(model_id), false,
        )
        .await;
        let _pending_id = seed_aircraft_with_flags(
            &pool, org_id, "T8_PENDING_002", "UAV-T8B", Some(model_id), true,
        )
        .await;

        let candidates = list_fc_sn_candidates(&pool, org_id, model_id).await.unwrap();

        assert_eq!(candidates.len(), 1, "only the verified aircraft should appear");
        assert_eq!(candidates[0].id, verified_id);
        assert_eq!(candidates[0].serial_number, "T8_VERIFIED_001");
        assert_eq!(candidates[0].equipment_id.as_deref(), Some("UAV-T8A"));
    }

    // ---- Test 9 ----

    /// The link-existing path writes an fc_sn identifier for the
    /// unmatched log SN, updates flights.aircraft_id, and writes a
    /// LINK_FC_SN_IDENTIFIER audit row — all in one call.
    #[sqlx::test(migrations = "./migrations")]
    async fn test_link_existing_writes_fc_sn_identifier_and_updates_flight_aircraft_id(
        pool: PgPool,
    ) {
        let org_id = default_org_id(&pool).await;
        let (user_id, user_code) = default_admin_user(&pool).await;
        let model_id = seed_model(&pool, org_id, "TestModel_LinkExisting").await;
        let target_id = seed_aircraft_with_flags(
            &pool, org_id, "T9_TARGET_BODY_SN", "UAV-T9", Some(model_id), false,
        )
        .await;
        let flight_id = seed_minimal_flight(&pool, org_id).await;

        let log_sn = "0410000001"; // unrelated FC SN — no shared prefix with body
        perform_link_existing(
            &pool,
            org_id,
            user_id,
            &user_code,
            target_id,
            log_sn,
            "TestModel_LinkExisting",
            Some(model_id),
            flight_id,
            "fc_sn",
        )
        .await
        .unwrap();

        // (a) Identifier row written with source='fc_sn'
        let row: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM aircraft_identifiers \
             WHERE aircraft_id = $1 AND identifier = $2 AND source = 'fc_sn'",
        )
        .bind(target_id)
        .bind(log_sn)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(row.0, 1, "expected exactly one fc_sn identifier row");

        // (b) Flight linked to the chosen aircraft
        let linked: (Option<i32>,) =
            sqlx::query_as("SELECT aircraft_id FROM flights WHERE id = $1")
                .bind(flight_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(linked.0, Some(target_id));

        // (c) Audit row written
        let audit: (i64,) = sqlx::query_as(
            "SELECT COUNT(*) FROM audit_log \
             WHERE action = 'LINK_FC_SN_IDENTIFIER' AND table_name = 'aircraft' AND record_id = $1",
        )
        .bind(target_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(audit.0, 1, "expected LINK_FC_SN_IDENTIFIER audit row");
    }

    // ---- Test 10 ----

    /// Reject when the operator picks an aircraft whose model_id does
    /// not match the log's model — typical cause is the operator picking
    /// the wrong row from the candidate list.
    #[sqlx::test(migrations = "./migrations")]
    async fn test_link_existing_rejects_aircraft_with_mismatched_model_id(pool: PgPool) {
        let org_id = default_org_id(&pool).await;
        let model_a = seed_model(&pool, org_id, "TestModel_ModelA").await;
        let model_b = seed_model(&pool, org_id, "TestModel_ModelB").await;
        let target_id = seed_aircraft_with_flags(
            &pool, org_id, "T10_TARGET_SN", "UAV-T10", Some(model_a), false,
        )
        .await;

        let err = verify_link_target(&pool, org_id, target_id, Some(model_b))
            .await
            .unwrap_err();
        match err {
            AppError::BadRequest(m) => {
                assert!(
                    m.contains("model does not match"),
                    "expected model-mismatch message, got: {m}"
                );
            }
            other => panic!("expected BadRequest, got {:?}", other),
        }
    }

    // ---- Test 11 ----

    /// Reject when the operator picks a pending-review aircraft — we
    /// must not chain ghosts. The verified-only filter on
    /// list_fc_sn_candidates makes this hard to trigger via the UI, but
    /// the handler must defend against a hand-crafted request anyway.
    #[sqlx::test(migrations = "./migrations")]
    async fn test_link_existing_rejects_aircraft_with_needs_manual_sn_true(pool: PgPool) {
        let org_id = default_org_id(&pool).await;
        let model_id = seed_model(&pool, org_id, "TestModel_PendingTarget").await;
        let pending_id = seed_aircraft_with_flags(
            &pool, org_id, "T11_PENDING_SN", "UAV-T11", Some(model_id), true,
        )
        .await;

        let err = verify_link_target(&pool, org_id, pending_id, Some(model_id))
            .await
            .unwrap_err();
        match err {
            AppError::BadRequest(m) => {
                assert!(
                    m.contains("needs_manual_sn"),
                    "expected needs_manual_sn message, got: {m}"
                );
            }
            other => panic!("expected BadRequest, got {:?}", other),
        }
    }

    // ---- Test 12 ----

    /// The force-register path writes two audit rows for the same
    /// aircraft: the standard INSERT entry (via log_insert_full, as
    /// the auto-register code already does today) and the
    /// FORCE_REGISTER_FC_SN_PRONE custom-action entry that captures
    /// the operator's reason.
    #[sqlx::test(migrations = "./migrations")]
    async fn test_force_register_writes_force_audit_alongside_standard_insert_audit(
        pool: PgPool,
    ) {
        let org_id = default_org_id(&pool).await;
        let (user_id, user_code) = default_admin_user(&pool).await;
        let model_id = seed_model(&pool, org_id, "Matrice 600").await;
        let ac_id = seed_aircraft_with_flags(
            &pool,
            org_id,
            "T12_FORCED_BODY_SN",
            "UAV-T12",
            Some(model_id),
            true, // needs_manual_sn = TRUE per the auto-register path
        )
        .await;

        // Simulate the two audit calls the force-register path makes.
        let ac_row = serde_json::json!({
            "id": ac_id,
            "serial_number": "T12_FORCED_BODY_SN",
            "model_id": model_id,
        });
        audit::log_insert_full(
            &pool, org_id, user_id, &user_code, "aircraft", ac_id, &ac_row,
        )
        .await;

        let reason = "operator confirms this is a brand-new Matrice 600 unit";
        audit::log_custom_action(
            &pool,
            org_id,
            user_id,
            &user_code,
            "FORCE_REGISTER_FC_SN_PRONE",
            "aircraft",
            ac_id,
            serde_json::json!({
                "sn": "T12_FORCED_BODY_SN",
                "model_name": "Matrice 600",
                "reason": reason,
            }),
        )
        .await
        .unwrap();

        let rows: Vec<(String, serde_json::Value)> = sqlx::query_as(
            "SELECT action, diff_jsonb FROM audit_log \
             WHERE record_id = $1 AND table_name = 'aircraft' \
             ORDER BY action",
        )
        .bind(ac_id)
        .fetch_all(&pool)
        .await
        .unwrap();

        assert_eq!(rows.len(), 2, "expected INSERT + FORCE rows, got: {rows:?}");
        // Alphabetical: FORCE_... comes before INSERT
        assert_eq!(rows[0].0, "FORCE_REGISTER_FC_SN_PRONE");
        assert_eq!(
            rows[0].1.get("reason").and_then(|v| v.as_str()),
            Some(reason)
        );
        assert_eq!(
            rows[0].1.get("model_name").and_then(|v| v.as_str()),
            Some("Matrice 600")
        );
        assert_eq!(rows[1].0, "INSERT");
    }
}
