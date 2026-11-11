-- Normalise battery compatible_model values to the canonical names in
-- battery_model_specs so nominal_max_cycles / cycle_uncertainty lookups
-- succeed. Prior migrations/import paths stored shorthand names
-- ("DJI Air 2s" vs "DJI Air 2s Intelligent Battery"). Fleet-stats EoL
-- detection falls back to per-battery max_cycles when no spec matches,
-- which hides warnings against the manufacturer's nominal 200 ±40 budget.
--
-- Idempotent: each UPDATE is gated by WHERE compatible_model = <shorthand>.
-- Re-running the migration after normalisation is a no-op because no row
-- still holds the shorthand value.
--
-- Every modified battery receives one audit row via INSERT...SELECT on the
-- RETURNING set. Marker user_code 'SYS' + user_id NULL identify these as
-- system-generated migration entries in the audit viewer.

-- DJI Air 2s
WITH updated AS (
    UPDATE batteries
    SET compatible_model = 'DJI Air 2s Intelligent Battery'
    WHERE compatible_model = 'DJI Air 2s'
    RETURNING id, org_id
)
INSERT INTO audit_log (org_id, user_id, user_code, action, table_name, record_id, diff_jsonb)
SELECT org_id, NULL, 'SYS', 'UPDATE', 'batteries', id,
    jsonb_build_object(
        'before', jsonb_build_object('compatible_model', 'DJI Air 2s'),
        'after',  jsonb_build_object('compatible_model', 'DJI Air 2s Intelligent Battery')
    )
FROM updated;

-- DJI Mavic 2 Pro
WITH updated AS (
    UPDATE batteries
    SET compatible_model = 'DJI Mavic 2 Intelligent Battery'
    WHERE compatible_model = 'DJI Mavic 2 Pro'
    RETURNING id, org_id
)
INSERT INTO audit_log (org_id, user_id, user_code, action, table_name, record_id, diff_jsonb)
SELECT org_id, NULL, 'SYS', 'UPDATE', 'batteries', id,
    jsonb_build_object(
        'before', jsonb_build_object('compatible_model', 'DJI Mavic 2 Pro'),
        'after',  jsonb_build_object('compatible_model', 'DJI Mavic 2 Intelligent Battery')
    )
FROM updated;

-- DJI Mavic 2 Zoom
WITH updated AS (
    UPDATE batteries
    SET compatible_model = 'DJI Mavic 2 Intelligent Battery'
    WHERE compatible_model = 'DJI Mavic 2 Zoom'
    RETURNING id, org_id
)
INSERT INTO audit_log (org_id, user_id, user_code, action, table_name, record_id, diff_jsonb)
SELECT org_id, NULL, 'SYS', 'UPDATE', 'batteries', id,
    jsonb_build_object(
        'before', jsonb_build_object('compatible_model', 'DJI Mavic 2 Zoom'),
        'after',  jsonb_build_object('compatible_model', 'DJI Mavic 2 Intelligent Battery')
    )
FROM updated;

-- DJI Neo
WITH updated AS (
    UPDATE batteries
    SET compatible_model = 'DJI Neo Battery'
    WHERE compatible_model = 'DJI Neo'
    RETURNING id, org_id
)
INSERT INTO audit_log (org_id, user_id, user_code, action, table_name, record_id, diff_jsonb)
SELECT org_id, NULL, 'SYS', 'UPDATE', 'batteries', id,
    jsonb_build_object(
        'before', jsonb_build_object('compatible_model', 'DJI Neo'),
        'after',  jsonb_build_object('compatible_model', 'DJI Neo Battery')
    )
FROM updated;

-- DJI Matrice 600
WITH updated AS (
    UPDATE batteries
    SET compatible_model = 'DJI Matrice 600 TB47S'
    WHERE compatible_model = 'DJI Matrice 600'
    RETURNING id, org_id
)
INSERT INTO audit_log (org_id, user_id, user_code, action, table_name, record_id, diff_jsonb)
SELECT org_id, NULL, 'SYS', 'UPDATE', 'batteries', id,
    jsonb_build_object(
        'before', jsonb_build_object('compatible_model', 'DJI Matrice 600'),
        'after',  jsonb_build_object('compatible_model', 'DJI Matrice 600 TB47S')
    )
FROM updated;

-- DJI Inspire 1
WITH updated AS (
    UPDATE batteries
    SET compatible_model = 'DJI Inspire 1 TB47/TB48'
    WHERE compatible_model = 'DJI Inspire 1'
    RETURNING id, org_id
)
INSERT INTO audit_log (org_id, user_id, user_code, action, table_name, record_id, diff_jsonb)
SELECT org_id, NULL, 'SYS', 'UPDATE', 'batteries', id,
    jsonb_build_object(
        'before', jsonb_build_object('compatible_model', 'DJI Inspire 1'),
        'after',  jsonb_build_object('compatible_model', 'DJI Inspire 1 TB47/TB48')
    )
FROM updated;
