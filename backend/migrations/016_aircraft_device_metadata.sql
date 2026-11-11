-- Add device_metadata JSONB for storing extra QR scan data (SSID, WiFi password, raw QR content, etc.)
ALTER TABLE aircraft
  ADD COLUMN device_metadata JSONB;

-- Add needs_manual_sn flag for aircraft whose serial_number cannot be extracted from QR data
-- (e.g. Tello, where the QR encodes the WiFi MAC instead of the SN)
ALTER TABLE aircraft
  ADD COLUMN needs_manual_sn BOOLEAN NOT NULL DEFAULT FALSE;

-- Index for efficient lookup of aircraft needing manual SN entry
CREATE INDEX idx_aircraft_needs_manual_sn
  ON aircraft(org_id)
  WHERE needs_manual_sn = TRUE;

COMMENT ON COLUMN aircraft.device_metadata IS
  'Extra data captured by QR scanner: {ssid, wifi_password, raw_qr, source_format}. NULL if scanned manually or no extras.';
COMMENT ON COLUMN aircraft.needs_manual_sn IS
  'TRUE for aircraft (e.g. Tello) where QR scan did not yield a usable serial_number. UI shows a warning until SN is manually corrected.';
