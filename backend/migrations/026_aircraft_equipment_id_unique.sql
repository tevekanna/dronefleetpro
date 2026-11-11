-- Migration: 026_aircraft_equipment_id_unique.sql
--
-- Add UNIQUE constraint to prevent duplicate equipment_id per org.
-- Resolves the data-quality gap that led to the UAV-049 and UAV-051
-- collisions cleaned up by the 2026-05-11 data-fix scripts. Per Round 4A pre-state
-- discovery, no current duplicates exist on local DB.
--
-- Option A (strict): applies to all aircraft including retired —
-- equipment IDs become permanent identifiers in the audit trail and
-- can never be reused even after the original drone retires.
--
-- NULL equipment_id is still allowed (NULLs are distinct in UNIQUE
-- constraints per the SQL spec, so the constraint does not block
-- multiple aircraft with NULL ID — useful for auto-import flows that
-- assign IDs only after manual review).

ALTER TABLE aircraft
  ADD CONSTRAINT aircraft_org_equipment_id_unique
  UNIQUE (org_id, equipment_id);
