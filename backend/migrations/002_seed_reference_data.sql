-- 002_seed_reference_data.sql
-- Seeds default organisation, admin user, and model maintenance templates.
-- This provides a working starting point for first-time deployment.

-- ============================================================
-- Default organisation for initial setup
-- ============================================================
INSERT INTO organisations (name, slug, country, plan)
VALUES ('Default Organisation', 'default', 'SE', 'standard')
ON CONFLICT (slug) DO NOTHING;

-- ============================================================
-- Password: dronefleet2025 (change immediately after first login)
-- ============================================================
INSERT INTO users (org_id, name, email, user_code, password_hash, role)
SELECT
    o.id,
    'Administrator',
    'admin@dronefleetpro.local',
    'ADM',
    '$2b$12$7T3ErfyTki4vQSnnFP6Spuw39JbFFRzMsrdsH/ZIFHgQrs0mJ13h.',
    'admin'
FROM organisations o
WHERE o.slug = 'default'
ON CONFLICT (org_id, email) DO NOTHING;

-- ============================================================
-- Model maintenance schedule templates
-- These are applied when a new aircraft of a matching model is registered.
-- Stored as JSON reference data in a settings key.
-- ============================================================

-- DJI Neo templates
INSERT INTO settings (org_id, key, value)
SELECT o.id, 'maint_template:DJI Neo', '[
  {"name": "Propeller Inspection", "interval_type": "flight_hours", "interval_value": 25, "is_critical": true},
  {"name": "General Inspection", "interval_type": "flight_hours", "interval_value": 75, "is_critical": false},
  {"name": "Annual Service", "interval_type": "calendar_days", "interval_value": 365, "is_critical": true}
]'
FROM organisations o WHERE o.slug = 'default'
ON CONFLICT (org_id, key) DO NOTHING;

-- DJI Air 2s templates
INSERT INTO settings (org_id, key, value)
SELECT o.id, 'maint_template:DJI Air 2s', '[
  {"name": "Propeller Inspection", "interval_type": "flight_hours", "interval_value": 50, "is_critical": true},
  {"name": "General Inspection", "interval_type": "flight_hours", "interval_value": 100, "is_critical": false},
  {"name": "Firmware Check", "interval_type": "calendar_days", "interval_value": 90, "is_critical": false},
  {"name": "Annual Service", "interval_type": "calendar_days", "interval_value": 365, "is_critical": true}
]'
FROM organisations o WHERE o.slug = 'default'
ON CONFLICT (org_id, key) DO NOTHING;

-- DJI Mavic 2 Pro templates
INSERT INTO settings (org_id, key, value)
SELECT o.id, 'maint_template:DJI Mavic 2 Pro', '[
  {"name": "Propeller Inspection", "interval_type": "flight_hours", "interval_value": 50, "is_critical": true},
  {"name": "General Inspection", "interval_type": "flight_hours", "interval_value": 100, "is_critical": false},
  {"name": "Firmware Check", "interval_type": "calendar_days", "interval_value": 90, "is_critical": false},
  {"name": "Annual Service", "interval_type": "calendar_days", "interval_value": 365, "is_critical": true}
]'
FROM organisations o WHERE o.slug = 'default'
ON CONFLICT (org_id, key) DO NOTHING;

-- DJI Mavic 2 Zoom templates
INSERT INTO settings (org_id, key, value)
SELECT o.id, 'maint_template:DJI Mavic 2 Zoom', '[
  {"name": "Propeller Inspection", "interval_type": "flight_hours", "interval_value": 50, "is_critical": true},
  {"name": "General Inspection", "interval_type": "flight_hours", "interval_value": 100, "is_critical": false},
  {"name": "Firmware Check", "interval_type": "calendar_days", "interval_value": 90, "is_critical": false},
  {"name": "Annual Service", "interval_type": "calendar_days", "interval_value": 365, "is_critical": true}
]'
FROM organisations o WHERE o.slug = 'default'
ON CONFLICT (org_id, key) DO NOTHING;

-- DJI Matrice 600 templates
INSERT INTO settings (org_id, key, value)
SELECT o.id, 'maint_template:DJI Matrice 600', '[
  {"name": "Propeller Inspection", "interval_type": "flight_hours", "interval_value": 25, "is_critical": true},
  {"name": "General Inspection", "interval_type": "flight_hours", "interval_value": 50, "is_critical": false},
  {"name": "Annual Service", "interval_type": "calendar_days", "interval_value": 365, "is_critical": true}
]'
FROM organisations o WHERE o.slug = 'default'
ON CONFLICT (org_id, key) DO NOTHING;
