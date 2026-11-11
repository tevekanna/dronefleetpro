# DJI / Litchi Log Parser Coverage Audit

**Date:** 2026-04-21
**Scope:** What fields the parsers extract from DJI binary `.txt`, DJI Fly
CSV, and Litchi CSV inputs, and whether those fields land in the
`flights` table.

"Internal design notes" in this document refers to a maintainer document
that is not part of the published repository; section numbers are kept for
the maintainers' reference.
**Source files reviewed:**
- [backend/src/services/dji_parser.rs](../backend/src/services/dji_parser.rs) — single parser module
  with `parse_txt` (binary) and `parse_csv` (all CSV variants)
- [backend/src/models/flight.rs](../backend/src/models/flight.rs) — `Flight` struct
- [backend/migrations/001_initial_schema.sql](../backend/migrations/001_initial_schema.sql),
  [003_add_total_distance.sql](../backend/migrations/003_add_total_distance.sql),
  [005_controller_and_drone_types.sql](../backend/migrations/005_controller_and_drone_types.sql) — flights table
- `dji-log-parser` 0.5.7 crate source (frame structs in
  `.../dji-log-parser-0.5.7/src/frame/`)

> **Note on architecture.** Both binary `.txt` and all three CSV variants
> (DJI Fly, DJI GO 4, Litchi) are handled by a single parser module that
> produces one common `FlightPreview` struct. The CSV path uses
> priority-ordered column-name pattern matching, so the set of fields it
> can extract is capped by the `find_col` calls in `parse_csv`.

---

## Table 1 — DJI binary `.txt` fields (via `dji-log-parser` 0.5.7)

The crate decodes every log frame into 10 sub-structs. The parser
iterates `frames` and touches only **5 of 10**: `osd`, `battery`,
`camera`, `custom`, `recover`. `gimbal`, `home`, `rc`, `app`, and
`details` are never read. This is the largest coverage gap in the codebase.

| Field | Extracted | Stored in `flights` | Recommendation |
|---|---|---|---|
| **FrameOSD** | | | |
| `osd.fly_time` | Yes | `duration_seconds` (last-frame value) | Keep |
| `osd.latitude` / `osd.longitude` | Yes | `trajectory_json`, derives `home_lat/lon` (first point), `max_distance_m`, `total_distance_m` | Keep |
| `osd.altitude` | Yes | `max_altitude_m`, `trajectory_json.alt` | Keep |
| `osd.height` | No | — | Ignore (altitude above takeoff is more useful; `altitude` is already AMSL/home-relative depending on log) |
| `osd.height_max` | No | — | Ignore (we compute our own max from frames) |
| `osd.vps_height` | No | — | Ignore (ultrasonic/VPS, only near-ground) |
| `osd.x_speed` / `osd.y_speed` | Yes | `max_speed_ms` (horizontal magnitude) | Keep |
| `osd.x_speed_max` / `osd.y_speed_max` | No | — | Ignore (we already compute the magnitude max) |
| `osd.z_speed` | No | — | **Add to DB** — vertical speed is useful for descent-rate incident review (ascent/descent anomalies, hard landings) |
| `osd.z_speed_max` | No | — | **Add to DB** — same reasoning; low-cost aggregate |
| `osd.pitch` / `osd.roll` / `osd.yaw` | No | — | Ignore at row level (high volume), optionally aggregate max bank angle |
| `osd.flyc_state` (FlightMode enum) | No | — | **Add to DB** — flight mode transitions (GPS / ATTI / Sport / ATTI-hover) are safety-critical. Store dominant mode + count of ATTI seconds |
| `osd.flyc_command` | No | — | Ignore (debugging only) |
| `osd.flight_action` | No | — | Ignore |
| `osd.is_gpd_used` | No | — | Ignore |
| `osd.non_gps_cause` | No | — | **Add to DB** — reason for GPS loss is incident-relevant |
| `osd.gps_num` | Yes | `satellite_count_min` | Keep |
| `osd.gps_level` | No | — | **Add to DB** — fix quality (0–5) complements satellite count |
| `osd.drone_type` | Yes | `aircraft_name` via `drone_type_mappings` | Keep |
| `osd.is_swave_work`, `osd.wave_error` | No | — | Ignore (over-water radar, rarely used) |
| `osd.go_home_status` | No | — | **Add to DB** — unplanned RTH is a reportable event |
| `osd.battery_type` | No | — | Ignore (already derivable from battery record) |
| `osd.is_on_ground`, `osd.is_motor_on` | No | — | Ignore (not informative post-hoc) |
| `osd.is_motor_blocked`, `osd.motor_start_failed_cause` | No | — | **Add to DB** — motor fault is a maintenance/incident signal |
| `osd.is_imu_preheated`, `osd.imu_init_fail_reason` | No | — | **Add to DB (flag only)** — IMU faults block safe flight |
| `osd.is_acceletor_over_range`, `osd.is_barometer_dead_in_air`, `osd.is_compass_error`, `osd.is_vibrating` | No | — | **Add to DB (single `sensor_warnings` bitmap or JSONB)** — sensor health snapshot |
| `osd.is_go_home_height_modified`, `osd.can_ioc_work`, `osd.is_not_enough_force`, `osd.is_out_of_limit`, `osd.is_propeller_catapult`, `osd.is_vision_used` | No | — | Ignore (configuration / rare-edge flags) |
| `osd.voltage_warning` | No | — | **Add to DB** — battery emergency level warning (0/1/2) is safety-critical |
| **FrameBattery** | | | |
| `battery.charge_level` | Yes | `min_battery_pct` | Keep |
| `battery.voltage` | No | — | **Add to DB** — min pack voltage complements percent; good for post-flight battery health checks |
| `battery.current` | No | — | Ignore (too volatile, not useful as aggregate) |
| `battery.current_capacity`, `battery.full_capacity` | No | — | Ignore (capacity ratio can stand in for health but battery test log covers this) |
| `battery.cell_num` | No | — | Ignore (fixed per model) |
| `battery.cell_voltages` | No | — | Ignore at row level (high volume) |
| `battery.cell_voltage_deviation` / `battery.max_cell_voltage_deviation` | **No** | **`cell_variance_v` (column exists, always NULL)** | **WIRE UP** — the column is already there. Populate from `max_cell_voltage_deviation`. No migration needed |
| `battery.temperature` | Yes | `battery_temp_c` (running max) | Keep |
| `battery.min_temperature`, `battery.max_temperature` | No | — | Ignore (our per-frame max captures effectively the same) |
| **FrameCamera** | | | |
| `camera.is_photo` | Yes | `photo_count` (frame count) | Keep — but note it counts *frames* where the flag is set, not *shutter events*. Real photo count may be off by a factor |
| `camera.is_video` | Yes | `video_seconds` (frame count × 0.1) | Keep — ditto; assumes 10 Hz frame rate |
| `camera.sd_card_is_inserted`, `camera.sd_card_state` | No | — | Ignore (troubleshooting only) |
| **FrameRecover** | | | |
| `recover.app_version` | Yes | used to derive `app_source` (`DJIFly` / `DJIGO4` / `Litchi`); raw value **not persisted** | Keep. Minor: consider persisting the raw `app_version` for audit |
| `recover.app_platform` | No | — | Ignore (redundant with app_version) |
| `recover.aircraft_name` | Yes | `aircraft_name` (cleaned via `clean_aircraft_model`) | Keep |
| `recover.aircraft_sn` | Yes | `aircraft_serial`, auto-registers aircraft | Keep |
| `recover.rc_sn` | Yes | resolves/creates `controller_id` | Keep |
| `recover.battery_sn` | Yes | resolves/creates `battery_id` | Keep |
| `recover.camera_sn` | No | — | **Add to DB** — useful for camera fleet tracking (payloads move between drones) |
| **FrameCustom** | | | |
| `custom.date_time` | Yes | `flight_start`, `flight_end` | Keep |
| **FrameGimbal** — *entire struct unused* | | | |
| `gimbal.mode` | No | — | Ignore |
| `gimbal.pitch` / `gimbal.roll` / `gimbal.yaw` | No | — | Ignore at row level; optionally store min/max pitch for survey missions |
| `gimbal.is_pitch_at_limit`, `is_roll_at_limit`, `is_yaw_at_limit`, `is_stuck` | No | — | **Add as boolean** `gimbal_fault` if any frame flags `is_stuck` — indicates mechanical fault |
| **FrameHome** — *entire struct unused* | | | |
| `home.latitude` / `home.longitude` | **No** | `home_lat` / `home_lon` populated from first GPS frame instead | **Switch to this source** — authoritative vs. heuristic; no migration needed |
| `home.altitude` | No | — | **Add to DB** — home elevation lets us correct altitude readings |
| `home.height_limit`, `home.max_allowed_height`, `home.go_home_height` | No | — | Ignore (user-configured, not flight data) |
| `home.go_home_mode`, `home.is_dynamic_home_point_enabled` | No | — | Ignore |
| `home.is_near_distance_limit`, `home.is_near_height_limit` | No | — | **Add as boolean flags** — pilot-approaching-limits events are operational signals |
| `home.is_compass_calibrating`, `home.compass_calibration_state` | No | — | Ignore (pre-flight only) |
| `home.is_beginner_mode`, `home.is_ioc_enabled`, `home.ioc_mode`, `home.ioc_course_lock_angle`, `home.is_multiple_mode_enabled` | No | — | Ignore |
| `home.current_flight_record_index` | No | — | Ignore |
| **FrameRC** — *entire struct unused* | | | |
| `rc.downlink_signal` | **No** | — | **Add to DB (HIGH PRIORITY)** — min downlink signal is the single best indicator of lost-link risk |
| `rc.uplink_signal` | **No** | — | **Add to DB (HIGH PRIORITY)** — same; controller→aircraft side |
| `rc.aileron`, `rc.elevator`, `rc.throttle`, `rc.rudder` | No | — | Ignore at row level (stick positions, too volatile for aggregates) |
| **FrameApp** — *entire struct unused* | | | |
| `app.tip` | No | — | Ignore (informational DJI tooltip) |
| `app.warn` | **No** | — | **Add to DB (HIGH PRIORITY)** — distinct warning messages during a flight should be persisted as a JSONB array; replaces a lot of incident-log guesswork |
| **FrameDetails** — *entire struct unused* | | | |
| `details.total_time` / `total_distance` / `max_height` / `max_horizontal_speed` / `max_vertical_speed` | No | — | Ignore — we already compute these from per-frame data, and our numbers are arguably more honest |
| `details.photo_num`, `details.video_time` | No | — | **Consider using these** — DJI's own photo/video counters are more accurate than our frame-flag heuristic |
| `details.aircraft_name`, `details.aircraft_sn`, `details.camera_sn`, `details.rc_sn`, `details.app_version`, `details.app_platform` | No | — | Redundant with `recover.*`; ignore |

---

## Table 2 — DJI Fly CSV fields

The CSV parser matches columns by substring (case-insensitive). A DJI Fly
export contains ~50–150 columns depending on firmware; the parser
matches **~16 of them** via `find_col`.

| Column (DJI Fly) | Extracted | Stored in `flights` | Recommendation |
|---|---|---|---|
| `osd.latitude` / `osd.longitude` | Yes | `trajectory_json` + home derivation | Keep |
| `osd.altitude` / `osd.height` | Yes | `max_altitude_m`, `trajectory_json.alt` | Keep |
| `osd.xSpeed` (and similar) | Yes | `max_speed_ms` (abs) | Keep |
| `osd.gpsNum` | Yes | `satellite_count_min` | Keep |
| `osd.flyTime(s)` | Yes | `duration_seconds` (from last row) | Keep |
| `osd.pitch` / `osd.roll` / `osd.yaw` | No | — | Ignore at row level (see Table 1) |
| `osd.flycState` | No | — | **Match as new column** — flight mode; same rationale as binary |
| `osd.goHomeStatus` | No | — | **Match** — unplanned RTH |
| `osd.droneType` | Yes | resolves `aircraft_name` via `drone_type_mappings` | Keep |
| `osd.voltageWarning` | No | — | **Match** — battery emergency warning |
| `osd.isCompassError` / `isVibrating` / etc. | No | — | **Match as batched `sensor_warnings`** (JSONB); parity with Table 1 |
| `osd.nonGpsCause` | No | — | **Match** |
| `battery.chargeLevel` | Yes | `min_battery_pct` | Keep |
| `battery.voltage` | No | — | **Match** |
| `battery.current` | No | — | Ignore |
| `battery.temperature` / `battery.maxTemperature` | Yes | `battery_temp_c` (running max) | Keep |
| `battery.cellVoltageDeviation` / `maxCellVoltageDeviation` | **No** | `cell_variance_v` (column exists, unused) | **WIRE UP** — parity with binary path |
| `camera.isPhoto` | Yes | `photo_count` | Keep (frame-count caveat applies) |
| `camera.isVideo` | Yes | `video_seconds` | Keep |
| `gimbal.pitch` / `roll` / `yaw` / `isStuck` | No | — | Ignore / optionally match `isStuck` |
| `home.latitude` / `home.longitude` / `home.altitude` | No | — | **Match** — authoritative home; parity with binary |
| `home.isNearDistanceLimit` / `isNearHeightLimit` | No | — | **Match** |
| `rc.downlinkSignal` / `rc.uplinkSignal` | **No** | — | **Match (HIGH PRIORITY)** — critical safety signal |
| `rc.aileron` / `elevator` / `throttle` / `rudder` | No | — | Ignore |
| `app.tip` | No | — | Ignore |
| `app.warn` | **No** | — | **Match (HIGH PRIORITY)** — aggregate distinct warnings into JSONB array |
| `recover.aircraftName` / `aircraftSn` / `rcSn` / `batterySn` / `appVersion` | Yes | `aircraft_name`, `aircraft_serial`, auto-registers controller & battery | Keep |
| `recover.cameraSn` | No | — | **Match** |
| `custom.dateTime` | Yes | `flight_start` / `flight_end` | Keep |

**Note:** DJI Fly CSVs also often contain computed columns (`distance(m)`,
`heightAboveTakeoff`, `wind_speed_*` — though wind is **not** actually
present in standard DJI exports; some third-party tools add it). The
parser does not match these. Trusting our own computed `max_distance_m`
is fine; matching `wind_speed_*` would require deciding which third-party
format to support.

---

## Table 3 — Litchi CSV fields

Litchi uses idiosyncratic column names (see the internal design notes §16). The parser
matches these via the same `find_col` aliases that cover DJI Fly.

| Column (Litchi) | Extracted | Stored in `flights` | Recommendation |
|---|---|---|---|
| `latitude` | Yes | `trajectory_json.lat` | Keep |
| `longitude` | Yes | `trajectory_json.lon` | Keep |
| `altitude(m)` / `altitude(ft)` | Partial — only `(m)` matched | `max_altitude_m` | Keep; the parser does not unit-convert feet. If Litchi ever emits `(ft)` only, values would be wrong. **Add explicit ft→m conversion** when the matched header contains `(ft)` |
| `speed(m/s)` / `speed(mph)` | Partial — only `(m/s)` matched | `max_speed_ms` | Same unit caveat as altitude |
| `satellites` | Yes | `satellite_count_min` | Keep |
| `time(millisecond)` | Yes | `duration_seconds` (ms→s conversion) | Keep |
| `datetime(utc)` | Yes (via generic `datetime` matcher) | `flight_start` / `flight_end` | Keep |
| `voltage(v)` | No | — | **Match** — Litchi exposes pack voltage; currently lost |
| `current(a)` | No | — | Ignore |
| `battery_percent` / `battery(%)` | Yes | `min_battery_pct` | Keep |
| `battery_temperature` | Yes (via generic `temp` matcher) | `battery_temp_c` | Keep |
| `distance(m)` / `distance(ft)` | No | — | Ignore (we compute our own) |
| `rc_signal` | **No** | — | **Match (HIGH PRIORITY)** — Litchi exposes signal strength; parity with binary |
| `rc_battery` | No | — | Ignore (remote's own battery, low value) |
| `gps_level` | No | — | **Match** — fix quality |
| `flight_mode` / `flycstate` | No | — | **Match** — parity with binary |
| `message` | No | — | **Match** — Litchi occasionally emits warnings here |
| `gimbal_pitch` / `gimbal_roll` / `gimbal_yaw` | No | — | Ignore at row level |
| `Planename` | Yes (via `planename` alias in `aircraft_name` matcher) | `aircraft_name` | Keep — this is the **operator's own name** for the aircraft, not the model. `drone_type_mappings` is how the model gets resolved |
| `FlyControllerSerialNumber` | Yes (via `flycontrollerserialnumber` alias) | `aircraft_serial` | Keep |
| `BatterySerialNumber` | Yes | `battery_serial` → resolves/creates `battery_id` | Keep |
| `RemoteSerialNumber` | Yes | `rc_serial` → resolves/creates `controller_id` | Keep |
| `appversion` / similar | Partial — Litchi often omits this | `app_source = "Litchi"` derived from string match; raw not persisted | Keep |

---

## Table 4 — Summary & recommendations

**Fields audited:** ~95 across the three formats (63 in the binary Frame
structs + 16 matched CSV columns + ~16 additional CSV columns worth
matching).

**Biggest gaps (logs → DB):**

1. **Nothing from `FrameRC`, `FrameApp`, `FrameHome`, or `FrameGimbal`
   reaches the DB for binary logs.** These contain the highest-signal
   safety data the crate exposes: downlink/uplink signal strength,
   DJI's own warning messages (`app.warn`), authoritative home-point
   coordinates, and gimbal fault flags.
2. **`cell_variance_v` column exists in the schema but is never
   populated.** Both the binary path (`battery.max_cell_voltage_deviation`)
   and the CSV path (`cellVoltageDeviation` columns) have the data.
   This is a pure wiring gap — no migration needed.
3. **Flight mode (`flyc_state`) and voltage warning (`voltage_warning`)
   are ignored on both paths.** Knowing whether a pilot entered ATTI mode
   or hit a low-voltage warning materially changes how you read an
   incident report.
4. **CSV parser only looks at 16 columns.** Even DJI Fly exports with
   150+ columns lose the same safety fields as the binary path.

**Stored but rarely populated from logs:**

- `cell_variance_v` — **never** populated from any log source.
- `pic_name`, `pm_name`, `easa_category`, `mission_type`,
  `operation_name`, `operation_description`, `preflight_done`, `remarks`
  — populated from the EASA debrief modal, not the log. Expected.
- `flight_guid` — always `None` in the parser output; deduplication
  currently relies on `source_filename` per §9 of the internal design notes.

**Inconsistencies between formats:**

- Unit handling in Litchi CSV: the parser matches `altitude(m)` and
  `speed(m/s)` but not `(ft)` / `(mph)`. If a user ever exports in
  imperial units the values silently fall through to defaults.
- `camera.isPhoto` / `isVideo` counting is *frame-count based*, not
  *shutter event based*. `FrameDetails.photo_num` and `video_time`
  would give truer numbers but aren't used.
- Home point derivation: binary parser uses the first valid GPS row
  (heuristic), while `FrameHome.latitude/longitude` are the
  authoritative recorded home. CSV parsers can't do better because
  DJI Fly CSV doesn't always expose home columns — but when they do
  exist, matching them would be more accurate.
- Wind speed: **no format exposes it directly**. The `dji-log-parser`
  crate does not decode wind; DJI Fly CSV does not include it in
  standard exports. Deriving it from airspeed vs. groundspeed is
  possible but out of scope.

**Proposed priority order:**

1. **Wire `cell_variance_v`** from `battery.max_cell_voltage_deviation`
   (binary) and the matching CSV column. Zero migrations, ~10 lines.
2. **Extract `app.warn` distinct messages into a new JSONB column
   `log_warnings`** on the flights table. Covers both routine low-battery
   events and unusual faults. Requires one migration + one UI list view.
3. **Extract `rc.downlink_signal` min and `rc.uplink_signal` min** into
   two new columns. Signal strength is the strongest lost-link predictor.
   Requires migration + minor UI (two new rows in the flight detail
   panel).
4. **Extract `osd.flyc_state` dominant mode + `voltage_warning` max** —
   captures ATTI excursions and battery emergencies. Migration + UI.
5. **Switch home-point source to `FrameHome`** for binary logs; keep
   the first-point heuristic only as CSV fallback. No migration.
6. Broaden CSV column matcher to cover the same fields as the binary
   path (flight mode, warn, signal). Parser-only change.
7. Use `FrameDetails.photo_num` / `video_time` for more accurate
   camera counts. Parser-only change.
8. Fix Litchi unit handling for `(ft)` and `(mph)` columns.
   Parser-only change.

**Impact summary:**

- **#1, #5, #6, #7, #8** are parser-only. No schema migration, no UI
  change, no breaking API change. These are safe, small wins.
- **#2, #3, #4** each require one migration (additive columns only,
  no breaking change), a parser update, and a minor UI addition in the
  flight detail modal. The API stays backward compatible because all
  new columns are nullable. No `flights` column would be renamed or
  dropped, so audit-log diffs and existing reports continue to work.
- No migration would require down-time or data backfill. Historic
  rows stay NULL in the new columns; future imports populate them.
