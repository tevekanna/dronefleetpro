-- 001_initial_schema.sql
-- Complete database schema for DroneFleetPro v2.0
-- Multi-tenant SaaS with EASA SAIL I/II compliance

-- ============================================================
-- Organisations (tenants)
-- ============================================================
CREATE TABLE IF NOT EXISTS organisations (
    id            SERIAL PRIMARY KEY,
    name          TEXT NOT NULL,
    slug          TEXT UNIQUE NOT NULL,
    easa_op_ref   TEXT,
    country       TEXT,
    plan          TEXT NOT NULL DEFAULT 'standard',
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- ============================================================
-- Pilots (operational personnel)
-- ============================================================
CREATE TABLE IF NOT EXISTS pilots (
    id                SERIAL PRIMARY KEY,
    org_id            INTEGER NOT NULL REFERENCES organisations(id) ON DELETE CASCADE,
    name              TEXT NOT NULL,
    email             TEXT,
    phone             TEXT,
    license_number    TEXT,
    license_type      TEXT,
    license_expires   DATE,
    is_active         BOOLEAN NOT NULL DEFAULT TRUE,
    notes             TEXT,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_pilots_org_id ON pilots(org_id);

-- ============================================================
-- Users (app login accounts)
-- ============================================================
CREATE TABLE IF NOT EXISTS users (
    id            SERIAL PRIMARY KEY,
    org_id        INTEGER NOT NULL REFERENCES organisations(id) ON DELETE CASCADE,
    name          TEXT NOT NULL,
    email         TEXT NOT NULL,
    user_code     TEXT NOT NULL,
    password_hash TEXT NOT NULL,
    role          TEXT NOT NULL,
    is_active     BOOLEAN NOT NULL DEFAULT TRUE,
    pilot_id      INTEGER REFERENCES pilots(id) ON DELETE SET NULL,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(org_id, email),
    UNIQUE(org_id, user_code)
);

CREATE INDEX IF NOT EXISTS idx_users_org_id ON users(org_id);
CREATE INDEX IF NOT EXISTS idx_users_email ON users(email);

-- ============================================================
-- Aircraft
-- ============================================================
CREATE TABLE IF NOT EXISTS aircraft (
    id               SERIAL PRIMARY KEY,
    org_id           INTEGER NOT NULL REFERENCES organisations(id) ON DELETE CASCADE,
    serial_number    TEXT NOT NULL,
    equipment_id     TEXT,
    model            TEXT,
    nickname         TEXT,
    status           TEXT NOT NULL DEFAULT 'active',
    retirement_notes TEXT,
    total_flights    INTEGER NOT NULL DEFAULT 0,
    total_seconds    INTEGER NOT NULL DEFAULT 0,
    last_flight      TIMESTAMPTZ,
    created_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(org_id, serial_number)
);

CREATE INDEX IF NOT EXISTS idx_aircraft_org_id ON aircraft(org_id);

-- ============================================================
-- Batteries
-- ============================================================
CREATE TABLE IF NOT EXISTS batteries (
    id               SERIAL PRIMARY KEY,
    org_id           INTEGER NOT NULL REFERENCES organisations(id) ON DELETE CASCADE,
    aircraft_id      INTEGER REFERENCES aircraft(id) ON DELETE SET NULL,
    serial_number    TEXT NOT NULL,
    equipment_id     TEXT,
    cycle_count      INTEGER NOT NULL DEFAULT 0,
    max_cycles       INTEGER NOT NULL DEFAULT 300,
    capacity_wh      REAL,
    compatible_model TEXT,
    last_used        TIMESTAMPTZ,
    status           TEXT NOT NULL DEFAULT 'active',
    created_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(org_id, serial_number)
);

CREATE INDEX IF NOT EXISTS idx_batteries_org_id ON batteries(org_id);
CREATE INDEX IF NOT EXISTS idx_batteries_aircraft_id ON batteries(aircraft_id);

-- ============================================================
-- Remote controllers
-- ============================================================
CREATE TABLE IF NOT EXISTS remote_controllers (
    id            SERIAL PRIMARY KEY,
    org_id        INTEGER NOT NULL REFERENCES organisations(id) ON DELETE CASCADE,
    serial_number TEXT NOT NULL,
    equipment_id  TEXT,
    model         TEXT,
    nickname      TEXT,
    aircraft_id   INTEGER REFERENCES aircraft(id) ON DELETE SET NULL,
    status        TEXT NOT NULL DEFAULT 'active',
    notes         TEXT,
    created_at    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(org_id, serial_number)
);

CREATE INDEX IF NOT EXISTS idx_remote_controllers_org_id ON remote_controllers(org_id);

-- ============================================================
-- Flights
-- ============================================================
CREATE TABLE IF NOT EXISTS flights (
    id                    SERIAL PRIMARY KEY,
    org_id                INTEGER NOT NULL REFERENCES organisations(id) ON DELETE CASCADE,
    pilot_id              INTEGER REFERENCES pilots(id) ON DELETE SET NULL,
    pilot_monitoring_id   INTEGER REFERENCES pilots(id) ON DELETE SET NULL,
    aircraft_id           INTEGER REFERENCES aircraft(id) ON DELETE SET NULL,
    battery_id            INTEGER REFERENCES batteries(id) ON DELETE SET NULL,
    flight_datetime       TIMESTAMPTZ,
    flight_start          TIMESTAMPTZ,
    flight_end            TIMESTAMPTZ,
    duration_seconds      INTEGER NOT NULL DEFAULT 0,
    max_altitude_m        REAL NOT NULL DEFAULT 0,
    max_distance_m        REAL NOT NULL DEFAULT 0,
    max_speed_ms          REAL,
    home_lat              REAL,
    home_lon              REAL,
    satellite_count_min   INTEGER,
    battery_temp_c        REAL,
    cell_variance_v       REAL,
    min_battery_pct       INTEGER,
    easa_category         TEXT,
    mission_type          TEXT,
    operation_name        TEXT,
    operation_description TEXT,
    preflight_done        BOOLEAN NOT NULL DEFAULT FALSE,
    remarks               TEXT,
    status                TEXT,
    log_source            TEXT,
    source_filename       TEXT,
    flight_guid           TEXT UNIQUE,
    trajectory_json       JSONB,
    photo_count           INTEGER NOT NULL DEFAULT 0,
    video_seconds         INTEGER NOT NULL DEFAULT 0,
    aircraft_name         TEXT,
    aircraft_serial       TEXT,
    pic_name              TEXT,
    pm_name               TEXT,
    deleted_at            TIMESTAMPTZ,
    created_at            TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_flights_org_id ON flights(org_id);
CREATE INDEX IF NOT EXISTS idx_flights_org_datetime ON flights(org_id, flight_datetime DESC);
CREATE INDEX IF NOT EXISTS idx_flights_pilot_id ON flights(pilot_id);
CREATE INDEX IF NOT EXISTS idx_flights_aircraft_id ON flights(aircraft_id) WHERE deleted_at IS NULL;
CREATE INDEX IF NOT EXISTS idx_flights_battery_id ON flights(battery_id);
CREATE INDEX IF NOT EXISTS idx_flights_guid ON flights(flight_guid) WHERE flight_guid IS NOT NULL;

-- ============================================================
-- Maintenance records (EASA OSO #3 compliant)
-- ============================================================
CREATE TABLE IF NOT EXISTS maintenance (
    id             SERIAL PRIMARY KEY,
    org_id         INTEGER NOT NULL REFERENCES organisations(id) ON DELETE CASCADE,
    aircraft_id    INTEGER REFERENCES aircraft(id) ON DELETE SET NULL,
    performed_date DATE NOT NULL,
    flight_log_ref TEXT,
    created_by     TEXT NOT NULL,
    remark         TEXT,
    action_taken   TEXT,
    maint_category TEXT,
    maint_type     TEXT,
    sign_off       TEXT NOT NULL,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_maintenance_org_id ON maintenance(org_id);
CREATE INDEX IF NOT EXISTS idx_maintenance_aircraft_id ON maintenance(aircraft_id);
CREATE INDEX IF NOT EXISTS idx_maintenance_date ON maintenance(performed_date DESC);

-- ============================================================
-- Defect log
-- ============================================================
CREATE TABLE IF NOT EXISTS defect_log (
    id             SERIAL PRIMARY KEY,
    org_id         INTEGER NOT NULL REFERENCES organisations(id) ON DELETE CASCADE,
    aircraft_id    INTEGER REFERENCES aircraft(id) ON DELETE SET NULL,
    flight_log_ref TEXT,
    defect_date    DATE NOT NULL,
    reported_by    TEXT NOT NULL,
    description    TEXT NOT NULL,
    status         TEXT NOT NULL DEFAULT 'open',
    action_taken   TEXT,
    closed_by      TEXT,
    closed_date    DATE,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_defect_log_org_id ON defect_log(org_id);
CREATE INDEX IF NOT EXISTS idx_defect_log_aircraft_id ON defect_log(aircraft_id);
CREATE INDEX IF NOT EXISTS idx_defect_log_status ON defect_log(org_id, status);

-- ============================================================
-- Maintenance schedules (per-aircraft recurring tasks)
-- ============================================================
CREATE TABLE IF NOT EXISTS maintenance_schedules (
    id                          SERIAL PRIMARY KEY,
    org_id                      INTEGER NOT NULL REFERENCES organisations(id) ON DELETE CASCADE,
    aircraft_id                 INTEGER NOT NULL REFERENCES aircraft(id) ON DELETE CASCADE,
    name                        TEXT NOT NULL,
    description                 TEXT,
    interval_type               TEXT NOT NULL,
    interval_value              REAL NOT NULL,
    warning_threshold           REAL NOT NULL DEFAULT 0.8,
    is_critical                 BOOLEAN NOT NULL DEFAULT FALSE,
    last_performed_at           TIMESTAMPTZ,
    last_performed_flight_hours REAL NOT NULL DEFAULT 0,
    last_performed_cycles       INTEGER NOT NULL DEFAULT 0,
    next_due_at                 TIMESTAMPTZ,
    next_due_flight_hours       REAL,
    next_due_cycles             INTEGER,
    is_active                   BOOLEAN NOT NULL DEFAULT TRUE,
    created_at                  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_maint_schedules_org_id ON maintenance_schedules(org_id);
CREATE INDEX IF NOT EXISTS idx_maint_schedules_aircraft_id ON maintenance_schedules(aircraft_id);

-- ============================================================
-- Incidents
-- ============================================================
CREATE TABLE IF NOT EXISTS incidents (
    id                     SERIAL PRIMARY KEY,
    org_id                 INTEGER NOT NULL REFERENCES organisations(id) ON DELETE CASCADE,
    flight_id              INTEGER REFERENCES flights(id) ON DELETE SET NULL,
    pilot_id               INTEGER REFERENCES pilots(id) ON DELETE SET NULL,
    aircraft_id            INTEGER REFERENCES aircraft(id) ON DELETE SET NULL,
    occurred_at            TIMESTAMPTZ NOT NULL,
    severity               TEXT NOT NULL,
    category               TEXT NOT NULL,
    location_description   TEXT,
    location_lat           REAL,
    location_lon           REAL,
    description            TEXT NOT NULL,
    immediate_action       TEXT,
    root_cause             TEXT,
    corrective_action      TEXT,
    reported_to_authority  BOOLEAN NOT NULL DEFAULT FALSE,
    authority_ref          TEXT,
    status                 TEXT NOT NULL DEFAULT 'open',
    created_at             TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at             TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_incidents_org_id ON incidents(org_id);
CREATE INDEX IF NOT EXISTS idx_incidents_flight_id ON incidents(flight_id);
CREATE INDEX IF NOT EXISTS idx_incidents_status ON incidents(org_id, status);

-- ============================================================
-- Pilot qualifications and certifications
-- ============================================================
CREATE TABLE IF NOT EXISTS pilot_qualifications (
    id              SERIAL PRIMARY KEY,
    org_id          INTEGER NOT NULL REFERENCES organisations(id) ON DELETE CASCADE,
    pilot_id        INTEGER NOT NULL REFERENCES pilots(id) ON DELETE CASCADE,
    type_name       TEXT NOT NULL,
    issued_date     DATE,
    expiry_date     DATE,
    certificate_ref TEXT,
    notes           TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_pilot_quals_org_id ON pilot_qualifications(org_id);
CREATE INDEX IF NOT EXISTS idx_pilot_quals_pilot_id ON pilot_qualifications(pilot_id);
CREATE INDEX IF NOT EXISTS idx_pilot_quals_expiry ON pilot_qualifications(expiry_date) WHERE expiry_date IS NOT NULL;

-- ============================================================
-- Battery test log
-- ============================================================
CREATE TABLE IF NOT EXISTS battery_tests (
    id                  SERIAL PRIMARY KEY,
    org_id              INTEGER NOT NULL REFERENCES organisations(id) ON DELETE CASCADE,
    battery_id          INTEGER NOT NULL REFERENCES batteries(id) ON DELETE CASCADE,
    test_date           DATE NOT NULL,
    test_type           TEXT NOT NULL,
    result              TEXT NOT NULL,
    capacity_measured   REAL,
    internal_resistance REAL,
    notes               TEXT,
    tested_by           TEXT,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_battery_tests_org_id ON battery_tests(org_id);
CREATE INDEX IF NOT EXISTS idx_battery_tests_battery_id ON battery_tests(battery_id);
CREATE INDEX IF NOT EXISTS idx_battery_tests_date ON battery_tests(test_date DESC);

-- ============================================================
-- Checklists
-- ============================================================
CREATE TABLE IF NOT EXISTS checklists (
    id            SERIAL PRIMARY KEY,
    org_id        INTEGER NOT NULL REFERENCES organisations(id) ON DELETE CASCADE,
    model         TEXT NOT NULL,
    type          TEXT NOT NULL CHECK(type IN ('normal', 'qrh')),
    title         TEXT NOT NULL,
    revision      TEXT NOT NULL DEFAULT '01',
    revision_date TEXT,
    created_by    TEXT,
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_checklists_org_id ON checklists(org_id);

-- ============================================================
-- Checklist sections
-- ============================================================
CREATE TABLE IF NOT EXISTS checklist_sections (
    id           SERIAL PRIMARY KEY,
    checklist_id INTEGER NOT NULL REFERENCES checklists(id) ON DELETE CASCADE,
    title        TEXT NOT NULL,
    sort_order   INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_checklist_sections_checklist ON checklist_sections(checklist_id);

-- ============================================================
-- Checklist items
-- ============================================================
CREATE TABLE IF NOT EXISTS checklist_items (
    id         SERIAL PRIMARY KEY,
    section_id INTEGER NOT NULL REFERENCES checklist_sections(id) ON DELETE CASCADE,
    label      TEXT NOT NULL,
    response   TEXT NOT NULL DEFAULT '',
    sort_order INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS idx_checklist_items_section ON checklist_items(section_id);

-- ============================================================
-- App settings (per organisation)
-- ============================================================
CREATE TABLE IF NOT EXISTS settings (
    org_id  INTEGER NOT NULL REFERENCES organisations(id) ON DELETE CASCADE,
    key     TEXT NOT NULL,
    value   TEXT,
    PRIMARY KEY (org_id, key)
);

-- ============================================================
-- Audit log (immutable)
-- ============================================================
CREATE TABLE IF NOT EXISTS audit_log (
    id          BIGSERIAL PRIMARY KEY,
    org_id      INTEGER REFERENCES organisations(id),
    user_id     INTEGER REFERENCES users(id),
    user_code   TEXT,
    action      TEXT NOT NULL,
    table_name  TEXT NOT NULL,
    record_id   INTEGER,
    diff_jsonb  JSONB,
    ip_address  TEXT,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_audit_log_org_id ON audit_log(org_id);
CREATE INDEX IF NOT EXISTS idx_audit_log_table ON audit_log(table_name, record_id);
CREATE INDEX IF NOT EXISTS idx_audit_log_created ON audit_log(created_at DESC);

-- ============================================================
-- Row-Level Security (belt-and-suspenders)
-- ============================================================
ALTER TABLE pilots ENABLE ROW LEVEL SECURITY;
ALTER TABLE users ENABLE ROW LEVEL SECURITY;
ALTER TABLE aircraft ENABLE ROW LEVEL SECURITY;
ALTER TABLE batteries ENABLE ROW LEVEL SECURITY;
ALTER TABLE remote_controllers ENABLE ROW LEVEL SECURITY;
ALTER TABLE flights ENABLE ROW LEVEL SECURITY;
ALTER TABLE maintenance ENABLE ROW LEVEL SECURITY;
ALTER TABLE defect_log ENABLE ROW LEVEL SECURITY;
ALTER TABLE maintenance_schedules ENABLE ROW LEVEL SECURITY;
ALTER TABLE incidents ENABLE ROW LEVEL SECURITY;
ALTER TABLE pilot_qualifications ENABLE ROW LEVEL SECURITY;
ALTER TABLE battery_tests ENABLE ROW LEVEL SECURITY;
ALTER TABLE checklists ENABLE ROW LEVEL SECURITY;
ALTER TABLE settings ENABLE ROW LEVEL SECURITY;
ALTER TABLE audit_log ENABLE ROW LEVEL SECURITY;
