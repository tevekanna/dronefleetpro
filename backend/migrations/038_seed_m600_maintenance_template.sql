-- Seed the DJI Matrice 600 annual maintenance instruction template and its
-- 36 checklist items, per DJI M600 Maintenance Manual V1.0 (2016-06).
--
-- Idempotent and safe on a fresh install:
--   * aircraft_model_id / org_id are resolved from the Matrice 600 catalog row.
--   * created_by is resolved to the lowest-id admin user.
--   * If no M600 model or no admin user exists yet, the migration RAISEs a
--     NOTICE and returns without error (a fresh install with no M600 model
--     must not fail the migration chain).
--   * The template is resolve-or-insert keyed on (org_id, aircraft_model_id,
--     name); items are inserted only when absent on (template_id,
--     section_name, item_text). No ON CONFLICT is used because migration 035
--     defined no matching unique constraint.

DO $$
DECLARE
    v_model_id    INTEGER;
    v_org_id      INTEGER;
    v_admin_id    INTEGER;
    v_template_id INTEGER;
BEGIN
    SELECT id, org_id INTO v_model_id, v_org_id
    FROM aircraft_models
    WHERE lower(name) LIKE '%m600%' OR lower(name) LIKE '%matrice 600%'
    LIMIT 1;

    SELECT id INTO v_admin_id
    FROM users
    WHERE role = 'admin'
    ORDER BY id
    LIMIT 1;

    IF v_model_id IS NULL THEN
        RAISE NOTICE 'No Matrice 600 aircraft model found; skipping M600 maintenance template seed.';
        RETURN;
    END IF;

    IF v_admin_id IS NULL THEN
        RAISE NOTICE 'No admin user found; skipping M600 maintenance template seed.';
        RETURN;
    END IF;

    -- Resolve-or-insert the template (idempotent on org_id + model + name).
    SELECT id INTO v_template_id
    FROM maintenance_instruction_templates
    WHERE org_id = v_org_id
      AND aircraft_model_id = v_model_id
      AND name = 'DJI Matrice 600 — Annual Inspection';

    IF v_template_id IS NULL THEN
        INSERT INTO maintenance_instruction_templates
            (org_id, aircraft_model_id, name, description, revision, source, is_active, created_by)
        VALUES
            (v_org_id, v_model_id,
             'DJI Matrice 600 — Annual Inspection',
             'Complete annual maintenance inspection per DJI M600 Maintenance Manual V1.0 (2016-06). Covers battery, centre frame, frame arms, landing gear, propulsion, IMU, GPS-compass, and control system.',
             '01', 'manufacturer', TRUE, v_admin_id)
        RETURNING id INTO v_template_id;
    END IF;

    -- Insert checklist items, skipping any that already exist for this template.
    INSERT INTO maintenance_checklist_items
        (template_id, section_name, section_order, item_order, item_text, is_required, reference_manual)
    SELECT v_template_id, d.section_name, d.section_order, d.item_order,
           d.item_text, d.is_required, d.reference_manual
    FROM (VALUES
        -- Section I — Battery
        ('Battery', 1, 1, 'Verify firmware and DJI GO app are up-to-date; confirm all cells fully charged and no cell differs by 0.2 V or more.', TRUE, 'DJI M600 Maintenance Manual V1.0, §I.1'),
        ('Battery', 1, 2, 'Check electrodes in battery compartment — no severe burning.', TRUE, 'DJI M600 Maintenance Manual V1.0, §I.2'),
        ('Battery', 1, 3, 'Check battery compartment plastic components and screws — secure.', TRUE, 'DJI M600 Maintenance Manual V1.0, §I.3'),
        ('Battery', 1, 4, 'Check contact pins in battery compartment — clear and not bent.', TRUE, 'DJI M600 Maintenance Manual V1.0, §I.4'),
        ('Battery', 1, 5, 'Check battery for damage and deformities — no severe damage.', TRUE, 'DJI M600 Maintenance Manual V1.0, §I.5'),
        ('Battery', 1, 6, 'Check battery gold fingers — clean with eraser if residue observed.', TRUE, 'DJI M600 Maintenance Manual V1.0, §I.6'),
        ('Battery', 1, 7, 'Check battery port metal connectors — no severe burning.', TRUE, 'DJI M600 Maintenance Manual V1.0, §I.7'),
        ('Battery', 1, 8, 'Long-term storage: verify battery stored according to Safety Guidelines and checked monthly.', FALSE, 'DJI M600 Maintenance Manual V1.0, §I.8'),
        -- Section II — Center Frame
        ('Center Frame', 2, 1, 'Check upper and lower plates for cracks and damage.', TRUE, 'DJI M600 Maintenance Manual V1.0, §II.1'),
        ('Center Frame', 2, 2, 'Check red knob for wear — frame arm must be secure when knob is locked.', TRUE, 'DJI M600 Maintenance Manual V1.0, §II.2'),
        ('Center Frame', 2, 3, 'Turn on aircraft and listen for abnormal noise from cooling fan.', TRUE, 'DJI M600 Maintenance Manual V1.0, §II.3'),
        ('Center Frame', 2, 4, 'Check damper on flight controller for wear and deformities.', TRUE, 'DJI M600 Maintenance Manual V1.0, §II.4'),
        ('Center Frame', 2, 5, 'Check all cable connections on flight controller and Lightbridge 2 Air System.', TRUE, 'DJI M600 Maintenance Manual V1.0, §II.5'),
        ('Center Frame', 2, 6, 'Check square head screws on power cables at bottom of centre frame — tighten if loose.', TRUE, 'DJI M600 Maintenance Manual V1.0, §II.6'),
        ('Center Frame', 2, 7, 'Check screws on expansion mounting kit — apply medium threadlocker if loose.', TRUE, 'DJI M600 Maintenance Manual V1.0, §II.7'),
        ('Center Frame', 2, 8, 'Check screws on retractable modules — apply medium threadlocker if loose.', TRUE, 'DJI M600 Maintenance Manual V1.0, §II.8'),
        -- Section III — Foldable Frame Arms
        ('Foldable Frame Arms', 3, 1, 'Check frame arm can move freely and is secure when knob is locked.', TRUE, 'DJI M600 Maintenance Manual V1.0, §III.1'),
        ('Foldable Frame Arms', 3, 2, 'Check motor base for cracks and damage.', TRUE, 'DJI M600 Maintenance Manual V1.0, §III.2'),
        ('Foldable Frame Arms', 3, 3, 'Check connection between motor base and arm tube.', TRUE, 'DJI M600 Maintenance Manual V1.0, §III.3'),
        ('Foldable Frame Arms', 3, 4, 'Check propeller ends on neighbouring arms are aligned — vertical difference must be less than 1 cm.', TRUE, 'DJI M600 Maintenance Manual V1.0, §III.4'),
        -- Section IV — Retractable Landing Gear
        ('Retractable Landing Gear', 4, 1, 'Check servo cables for wear — connection points must be secure.', TRUE, 'DJI M600 Maintenance Manual V1.0, §IV.1'),
        ('Retractable Landing Gear', 4, 2, 'Check power cable connector for wear.', TRUE, 'DJI M600 Maintenance Manual V1.0, §IV.2'),
        ('Retractable Landing Gear', 4, 3, 'Raise and lower landing gear several times — confirm free movement.', TRUE, 'DJI M600 Maintenance Manual V1.0, §IV.3'),
        ('Retractable Landing Gear', 4, 4, 'Check for white plastic debris around shaft screws — replace washers if found.', TRUE, 'DJI M600 Maintenance Manual V1.0, §IV.4'),
        ('Retractable Landing Gear', 4, 5, 'Check shaft screws — apply high-strength threadlocker and tighten to 0.45 ± 0.05 N·m.', TRUE, 'DJI M600 Maintenance Manual V1.0, §IV.5'),
        -- Section V — Propulsion System
        ('Propulsion System', 5, 1, 'Check motor rotor by gently shaking — must not be loose.', TRUE, 'DJI M600 Maintenance Manual V1.0, §V.1'),
        ('Propulsion System', 5, 2, 'Check motors for deformities — gap between motor and base must be even, motor must spin freely.', TRUE, 'DJI M600 Maintenance Manual V1.0, §V.2'),
        ('Propulsion System', 5, 3, 'Remove propellers, start motors — check rotor shaft is centred with no excessive vibration.', TRUE, 'DJI M600 Maintenance Manual V1.0, §V.3'),
        ('Propulsion System', 5, 4, 'Remove propellers, start motors — listen for abnormal noise indicating worn bearings.', TRUE, 'DJI M600 Maintenance Manual V1.0, §V.4'),
        ('Propulsion System', 5, 5, 'Check propellers — no bending, breakage, or cracks.', TRUE, 'DJI M600 Maintenance Manual V1.0, §V.5'),
        ('Propulsion System', 5, 6, 'Attach propellers, start aircraft, observe from 3 m — no double outline visible on spinning propeller.', TRUE, 'DJI M600 Maintenance Manual V1.0, §V.6'),
        -- Section VI — IMU
        ('IMU', 6, 1, 'Verify firmware and DJI GO app are up-to-date; perform advanced IMU calibration on a flat, stable, cool surface.', TRUE, 'DJI M600 Maintenance Manual V1.0, §VI.1'),
        -- Section VII — GPS-Compass Pro
        ('GPS-Compass Pro', 7, 1, 'Check GPS-Compass module is securely mounted with arrow pointing to front of aircraft.', TRUE, 'DJI M600 Maintenance Manual V1.0, §VII.1'),
        ('GPS-Compass Pro', 7, 2, 'Verify DJI Assistant 2 is up-to-date; confirm mounting position settings are correct.', TRUE, 'DJI M600 Maintenance Manual V1.0, §VII.2'),
        -- Section VIII — Control and Video Transmission System
        ('Control and Video Transmission System', 8, 1, 'Check two antennas on landing gear legs are secure; check antenna cables for bending or damage — must be securely connected.', TRUE, 'DJI M600 Maintenance Manual V1.0, §VIII.1'),
        ('Control and Video Transmission System', 8, 2, 'Check remote controller antennas for damage.', TRUE, 'DJI M600 Maintenance Manual V1.0, §VIII.2')
    ) AS d(section_name, section_order, item_order, item_text, is_required, reference_manual)
    WHERE NOT EXISTS (
        SELECT 1 FROM maintenance_checklist_items mci
        WHERE mci.template_id = v_template_id
          AND mci.section_name = d.section_name
          AND mci.item_text = d.item_text
    );

    RAISE NOTICE 'M600 maintenance template seeded (template_id=%).', v_template_id;
END $$;
