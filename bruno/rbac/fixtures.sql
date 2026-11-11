-- ============================================================================
-- RBAC Bruno suite fixtures
-- ----------------------------------------------------------------------------
-- Run once before executing the bruno/rbac/ folder. Idempotent — safe to
-- re-run. Password for all fixture users is 'dronefleet2025' (same bcrypt
-- hash as the seeded admin).
-- ============================================================================

-- Technician
INSERT INTO users (org_id, name, email, user_code, password_hash, role, is_active)
VALUES (
    1,
    'RBAC Test Technician',
    'rbac-tech@dronefleetpro.local',
    'RTE',
    '$2b$12$7T3ErfyTki4vQSnnFP6Spuw39JbFFRzMsrdsH/ZIFHgQrs0mJ13h.',
    'technician',
    TRUE
)
ON CONFLICT (org_id, email) DO UPDATE
    SET role = 'technician', is_active = TRUE, password_hash = EXCLUDED.password_hash;

-- Safety officer
INSERT INTO users (org_id, name, email, user_code, password_hash, role, is_active)
VALUES (
    1,
    'RBAC Test Safety Officer',
    'rbac-safety@dronefleetpro.local',
    'RSO',
    '$2b$12$7T3ErfyTki4vQSnnFP6Spuw39JbFFRzMsrdsH/ZIFHgQrs0mJ13h.',
    'safety_officer',
    TRUE
)
ON CONFLICT (org_id, email) DO UPDATE
    SET role = 'safety_officer', is_active = TRUE, password_hash = EXCLUDED.password_hash;

-- Viewer (reuses existing BRT fixture if present, otherwise creates RVI)
INSERT INTO users (org_id, name, email, user_code, password_hash, role, is_active)
VALUES (
    1,
    'RBAC Test Viewer',
    'rbac-viewer@dronefleetpro.local',
    'RVI',
    '$2b$12$7T3ErfyTki4vQSnnFP6Spuw39JbFFRzMsrdsH/ZIFHgQrs0mJ13h.',
    'viewer',
    TRUE
)
ON CONFLICT (org_id, email) DO UPDATE
    SET role = 'viewer', is_active = TRUE, password_hash = EXCLUDED.password_hash;

-- Pilot fixture: ensure ATP exists and is linked to a pilot record so
-- pilot-self filtering can be exercised. Use Carl Captain (pilots.id = 4)
-- as the linked pilot — he has flight history in the dev fixtures.
INSERT INTO users (org_id, name, email, user_code, password_hash, role, is_active)
VALUES (
    1,
    'Audit Test Pilot',
    'audit-test-pilot@dronefleetpro.local',
    'ATP',
    '$2b$12$7T3ErfyTki4vQSnnFP6Spuw39JbFFRzMsrdsH/ZIFHgQrs0mJ13h.',
    'pilot',
    TRUE
)
ON CONFLICT (org_id, email) DO UPDATE
    SET role = 'pilot', is_active = TRUE, password_hash = EXCLUDED.password_hash;

UPDATE users
SET pilot_id = (SELECT id FROM pilots WHERE org_id = 1 AND name = 'Carl Captain' LIMIT 1)
WHERE email = 'audit-test-pilot@dronefleetpro.local'
  AND pilot_id IS NULL;
