-- One-time backfill of the denormalised aircraft flight counters.
--
-- These columns drifted because the INSERT INTO flights paths in
-- handlers/flights/commit.rs and handlers/flights/crud.rs never bumped
-- aircraft.total_flights / total_seconds / last_flight. This migration
-- recomputes them authoritatively from the flights table; the handler
-- fix in the same PR keeps them in sync going forward.
--
-- Decrement on flight DELETE is not yet handled — flag for follow-up.
-- Longer-term plan is to replace these denormalised counters with on-read
-- aggregation.

-- Reset everything so aircraft with zero flights also end up with a clean
-- state (in particular, last_flight back to NULL).
UPDATE aircraft SET total_flights = 0, total_seconds = 0, last_flight = NULL;

-- Then set from authoritative aggregates over the flights table. Only
-- rows that have at least one non-deleted flight with a non-null
-- aircraft_id get updated; aircraft with no flights remain at the reset
-- defaults.
UPDATE aircraft a SET
  total_flights = s.cnt,
  total_seconds = s.total_seconds,
  last_flight   = s.last_dt
FROM (
  SELECT aircraft_id,
         COUNT(*)::int                            AS cnt,
         COALESCE(SUM(duration_seconds), 0)::int  AS total_seconds,
         MAX(flight_datetime)                     AS last_dt
  FROM flights
  WHERE deleted_at IS NULL AND aircraft_id IS NOT NULL
  GROUP BY aircraft_id
) s
WHERE a.id = s.aircraft_id;
