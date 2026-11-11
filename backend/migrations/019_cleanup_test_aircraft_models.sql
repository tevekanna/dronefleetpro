-- ============================================================================
-- Cleanup test artifacts in aircraft_models catalog.
--
-- Identified test rows from discovery (2026-04-30):
--   id 3:  DJI Test
--   id 4:  UAV101
--   id 7:  TestImportModel
--   id 8:  MfrTestModel-1777391534
--   id 9:  MfrTestModel2-...
--
-- FK references to aircraft_models (verified before writing this migration):
--   aircraft.model_id            ON DELETE SET NULL
--   drone_type_mappings.model_id ON DELETE CASCADE  -- silent data loss risk
--
-- Conservative deletion: only delete a row when it has NO aircraft AND NO
-- drone_type_mappings referencing it. Anything else stays — admin resolves
-- manually. Discovery showed UAV101 (id 4) has 1 drone_type_mapping; that
-- row will be left in place by this migration.
-- ============================================================================

DELETE FROM aircraft_models am
WHERE am.id IN (3, 4, 7, 8, 9)
  AND NOT EXISTS (SELECT 1 FROM aircraft a WHERE a.model_id = am.id)
  AND NOT EXISTS (SELECT 1 FROM drone_type_mappings dtm WHERE dtm.model_id = am.id);
