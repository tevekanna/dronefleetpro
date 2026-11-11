-- 043_ardupilot_log_support.sql
-- Foundation for ArduPilot flight-log import (PR A of 3).
-- (1) Allow 'ardupilot_uid' as an aircraft_identifiers source: the autopilot
--     board CPU UID is the closest analogue to a manufacturer serial number
--     for an ArduPilot airframe (no body SN is broadcast in MAVLink).
-- (2) Add flights.log_precedence so a later, richer .bin DataFlash import can
--     UPGRADE a flight previously created from a .tlog without a downgrade
--     ever happening in the reverse direction.
--     Convention for the precedence value (set by the commit handler in PR B/C,
--     left NULL here for all existing/DJI rows):
--        NULL = legacy / DJI / manual (no ArduPilot precedence semantics)
--        10   = ardupilot_tlog  (telemetry capture, lower trust)
--        20   = ardupilot_bin   (onboard DataFlash, higher trust, may overwrite 10)

ALTER TABLE aircraft_identifiers DROP CONSTRAINT IF EXISTS aircraft_identifiers_source_check;
ALTER TABLE aircraft_identifiers ADD CONSTRAINT aircraft_identifiers_source_check CHECK (source IN (
    'body_sn',
    'fc_sn',
    'alias_djigo',
    'alias_djifly',
    'alias_manual',
    'qr_label',
    'ardupilot_uid',
    'other'
));

ALTER TABLE flights ADD COLUMN IF NOT EXISTS log_precedence SMALLINT;
COMMENT ON COLUMN flights.log_precedence IS 'ArduPilot log-source trust level: NULL=legacy/DJI/manual, 10=ardupilot_tlog, 20=ardupilot_bin. A higher value may overwrite a lower one during merge; a lower value never downgrades a higher one.';
