-- ============================================================================
-- Seed maintenance_schedule_templates with the defaults that were previously
-- hardcoded in services/maintenance_defaults.rs::defaults_for_model.
--
-- Family mappings (substring match on aircraft_models.name, lower-case):
--   "neo"                                -> 25h prop / 75h general / 365d annual
--   "matrice" | "m600" | "inspire"       -> 25h prop / 50h general / 365d annual
--   everything else                      -> 50h prop / 100h general / 90d firmware / 365d annual
--
-- Mavic 3 falls into "everything else" intentionally: same behaviour as the
-- prior Rust code, admin can override per-template via the new CRUD UI.
--
-- Idempotent via ON CONFLICT (org_id, model_id, name) DO NOTHING — safe to
-- re-run if a model is added later (the new model picks up the defaults
-- next time the migration is replayed; in practice it won't be replayed,
-- so model auto-seeding for newly-added rows belongs to the templates CRUD
-- workflow rather than this one-shot backfill).
--
-- Filter on is_active = TRUE so retired/merged catalog rows don't accrete
-- templates.
-- ============================================================================

-- ----------------------------------------------------------------------------
-- Family: "neo" — 25h prop / 75h general
-- ----------------------------------------------------------------------------
INSERT INTO maintenance_schedule_templates
    (org_id, model_id, name, description, interval_type, interval_value,
     warning_threshold, is_critical, created_by)
SELECT am.org_id, am.id,
       'Propeller inspection',
       'Inspect propellers for cracks, chips, and balance.',
       'flight_hours', 25, 0.8, TRUE, 'system'
FROM aircraft_models am
WHERE am.is_active = TRUE
  AND lower(am.name) LIKE '%neo%'
ON CONFLICT (org_id, model_id, name) DO NOTHING;

INSERT INTO maintenance_schedule_templates
    (org_id, model_id, name, description, interval_type, interval_value,
     warning_threshold, is_critical, created_by)
SELECT am.org_id, am.id,
       'General inspection',
       'Visual inspection of airframe, motors, and gimbal.',
       'flight_hours', 75, 0.8, FALSE, 'system'
FROM aircraft_models am
WHERE am.is_active = TRUE
  AND lower(am.name) LIKE '%neo%'
ON CONFLICT (org_id, model_id, name) DO NOTHING;

-- ----------------------------------------------------------------------------
-- Family: "matrice|m600|inspire" — 25h prop / 50h general
-- ----------------------------------------------------------------------------
INSERT INTO maintenance_schedule_templates
    (org_id, model_id, name, description, interval_type, interval_value,
     warning_threshold, is_critical, created_by)
SELECT am.org_id, am.id,
       'Propeller inspection',
       'Inspect propellers for cracks, chips, and balance.',
       'flight_hours', 25, 0.8, TRUE, 'system'
FROM aircraft_models am
WHERE am.is_active = TRUE
  AND (lower(am.name) LIKE '%matrice%'
       OR lower(am.name) LIKE '%m600%'
       OR lower(am.name) LIKE '%inspire%')
  AND lower(am.name) NOT LIKE '%neo%'  -- defensive; no Neo-Inspire collision today
ON CONFLICT (org_id, model_id, name) DO NOTHING;

INSERT INTO maintenance_schedule_templates
    (org_id, model_id, name, description, interval_type, interval_value,
     warning_threshold, is_critical, created_by)
SELECT am.org_id, am.id,
       'General inspection',
       'Visual inspection of airframe, motors, and gimbal.',
       'flight_hours', 50, 0.8, FALSE, 'system'
FROM aircraft_models am
WHERE am.is_active = TRUE
  AND (lower(am.name) LIKE '%matrice%'
       OR lower(am.name) LIKE '%m600%'
       OR lower(am.name) LIKE '%inspire%')
  AND lower(am.name) NOT LIKE '%neo%'
ON CONFLICT (org_id, model_id, name) DO NOTHING;

-- ----------------------------------------------------------------------------
-- Family: "default" — everything else; 50h prop / 100h general / 90d firmware
-- ----------------------------------------------------------------------------
INSERT INTO maintenance_schedule_templates
    (org_id, model_id, name, description, interval_type, interval_value,
     warning_threshold, is_critical, created_by)
SELECT am.org_id, am.id,
       'Propeller inspection',
       'Inspect propellers for cracks, chips, and balance.',
       'flight_hours', 50, 0.8, TRUE, 'system'
FROM aircraft_models am
WHERE am.is_active = TRUE
  AND lower(am.name) NOT LIKE '%neo%'
  AND lower(am.name) NOT LIKE '%matrice%'
  AND lower(am.name) NOT LIKE '%m600%'
  AND lower(am.name) NOT LIKE '%inspire%'
ON CONFLICT (org_id, model_id, name) DO NOTHING;

INSERT INTO maintenance_schedule_templates
    (org_id, model_id, name, description, interval_type, interval_value,
     warning_threshold, is_critical, created_by)
SELECT am.org_id, am.id,
       'General inspection',
       'Visual inspection of airframe, motors, and gimbal.',
       'flight_hours', 100, 0.8, FALSE, 'system'
FROM aircraft_models am
WHERE am.is_active = TRUE
  AND lower(am.name) NOT LIKE '%neo%'
  AND lower(am.name) NOT LIKE '%matrice%'
  AND lower(am.name) NOT LIKE '%m600%'
  AND lower(am.name) NOT LIKE '%inspire%'
ON CONFLICT (org_id, model_id, name) DO NOTHING;

INSERT INTO maintenance_schedule_templates
    (org_id, model_id, name, description, interval_type, interval_value,
     warning_threshold, is_critical, created_by)
SELECT am.org_id, am.id,
       'Firmware check',
       'Verify firmware is up-to-date.',
       'calendar_days', 90, 0.8, FALSE, 'system'
FROM aircraft_models am
WHERE am.is_active = TRUE
  AND lower(am.name) NOT LIKE '%neo%'
  AND lower(am.name) NOT LIKE '%matrice%'
  AND lower(am.name) NOT LIKE '%m600%'
  AND lower(am.name) NOT LIKE '%inspire%'
ON CONFLICT (org_id, model_id, name) DO NOTHING;

-- ----------------------------------------------------------------------------
-- Annual service for ALL active models, every family.
-- ----------------------------------------------------------------------------
INSERT INTO maintenance_schedule_templates
    (org_id, model_id, name, description, interval_type, interval_value,
     warning_threshold, is_critical, created_by)
SELECT am.org_id, am.id,
       'Annual service',
       'Complete annual inspection and service per manufacturer schedule.',
       'calendar_days', 365, 0.8, TRUE, 'system'
FROM aircraft_models am
WHERE am.is_active = TRUE
ON CONFLICT (org_id, model_id, name) DO NOTHING;
