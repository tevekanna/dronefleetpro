-- ============================================================================
-- Cleanup of aircraft imported via batch QR-CSV before the parser was hardened.
--
-- Three patterns to fix:
--
-- 1. DJI Mavic-family (Mavic Air, Mavic Pro, Mavic 2 Pro/Zoom): serial_number
--    contains the full QR content "SN:XXX SSID:YYY Password:ZZZ".
--    Action: extract the SN, move SSID and password to device_metadata.
--
-- 2. Tello, raw form: serial_number contains the WiFi MAC (e.g.
--    0TQTEST0000001). Tello logs reference a different ID that we cannot match.
--
-- 3. Tello, CSV-wrapped form: serial_number contains the entire CSV row
--    e.g.  "0TQTEST0000001","04/30/2026 12:49:49"  — observed in production
--    where the legacy fallback path read the file line-by-line.
--
-- For both Tello forms the cleanup is the same: extract the bare WiFi MAC
-- into serial_number (so the column stays usable), copy the original raw
-- value into device_metadata.raw_qr, and flag needs_manual_sn = TRUE.
-- ============================================================================

-- ----------------------------------------------------------------------------
-- Pattern 1: DJI Mavic-family — "SN:XXX SSID:YYY Password:ZZZ" in serial_number
--
-- PostgreSQL evaluates all SET expressions against the OLD row values, so it
-- is safe to derive device_metadata from serial_number while simultaneously
-- rewriting serial_number itself.
-- ----------------------------------------------------------------------------
UPDATE aircraft
SET
  serial_number = substring(serial_number FROM 'SN:(\S+)'),
  device_metadata = jsonb_build_object(
    'ssid',          substring(serial_number FROM 'SSID:(\S+)'),
    'wifi_password', substring(serial_number FROM 'Password:(\S+)'),
    'source_format', 'dji_qr_scanner_v1'
  )
WHERE serial_number ~ '^SN:\S+\s+SSID:\S+\s+Password:\S+$';

-- ----------------------------------------------------------------------------
-- Pattern 2: Tello bare WiFi MAC — "0TQ" + 11 alphanumeric chars
-- ----------------------------------------------------------------------------
UPDATE aircraft
SET
  device_metadata = jsonb_build_object(
    'raw_qr',        serial_number,
    'source_format', 'tello_qr_scanner_v1'
  ),
  needs_manual_sn = TRUE
WHERE serial_number ~ '^0TQ[A-Z0-9]{11}$'
  AND device_metadata IS NULL;

-- ----------------------------------------------------------------------------
-- Pattern 3: Tello CSV-wrapped — "0TQXXXXXXXXXXX","MM/DD/YYYY HH:MM:SS"
--
-- Extract the bare WiFi MAC into serial_number so future hand-correction or
-- match attempts have a usable value, while preserving the original raw QR
-- under device_metadata.raw_qr for traceability.
-- ----------------------------------------------------------------------------
UPDATE aircraft
SET
  device_metadata = jsonb_build_object(
    'raw_qr',        serial_number,
    'source_format', 'tello_qr_scanner_v1'
  ),
  serial_number = substring(serial_number FROM '"(0TQ[A-Z0-9]{11})"'),
  needs_manual_sn = TRUE
WHERE serial_number ~ '^"0TQ[A-Z0-9]{11}","[^"]+"$'
  AND device_metadata IS NULL;
