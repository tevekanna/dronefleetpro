-- ============================================================================
-- Cleanup of CSV header rows that were imported as data by the legacy batch
-- importer. Examples observed in dev:
--   serial_number = 'Data,Timestamp'                       (Tello scanner CSV header)
--   serial_number = 'pinned,_datetime,format,name,...'     (DJI scanner CSV header)
--
-- These rows are not real aircraft. We delete them iff they have:
--   - serial_number containing a comma (no real DJI/Tello SN does)
--   - total_flights = 0 AND total_seconds = 0
--   - no FK references from any related table
--
-- This is a follow-up to migration 017 which missed this pattern.
--
-- FK-coverage check: the seven NOT EXISTS clauses below cover every table
-- that has a foreign key on aircraft.id, as listed by:
--   SELECT tc.table_name, kcu.column_name
--   FROM information_schema.table_constraints tc
--   JOIN information_schema.key_column_usage kcu USING (constraint_name)
--   JOIN information_schema.constraint_column_usage ccu USING (constraint_name)
--   WHERE tc.constraint_type = 'FOREIGN KEY' AND ccu.table_name = 'aircraft';
-- Most of those FKs are ON DELETE SET NULL (or CASCADE for
-- maintenance_schedules), so PostgreSQL would not refuse the DELETE — but a
-- silent SET NULL on a flight or incident is exactly the kind of data
-- integrity loss this guard exists to prevent.
-- ============================================================================

DELETE FROM aircraft a
WHERE a.serial_number LIKE '%,%'
  AND a.total_flights = 0
  AND a.total_seconds = 0
  AND NOT EXISTS (SELECT 1 FROM batteries             b WHERE b.aircraft_id = a.id)
  AND NOT EXISTS (SELECT 1 FROM defect_log            d WHERE d.aircraft_id = a.id)
  AND NOT EXISTS (SELECT 1 FROM flights               f WHERE f.aircraft_id = a.id AND f.deleted_at IS NULL)
  AND NOT EXISTS (SELECT 1 FROM incidents             i WHERE i.aircraft_id = a.id)
  AND NOT EXISTS (SELECT 1 FROM maintenance           m WHERE m.aircraft_id = a.id)
  AND NOT EXISTS (SELECT 1 FROM maintenance_schedules s WHERE s.aircraft_id = a.id)
  AND NOT EXISTS (SELECT 1 FROM remote_controllers    r WHERE r.aircraft_id = a.id);
