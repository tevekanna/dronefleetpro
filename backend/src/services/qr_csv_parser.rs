// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

//! Parser for fleet batch-import CSVs.
//!
//! Five input formats are recognised. Detection priority is the order
//! below — first match wins.
//!
//! 1. **Binary Eye export** (Android QR/DataMatrix scanner): headers
//!    contain all of `content`, `format`, `error_correction_level`. The
//!    `content` cell is a bare serial number; rows whose `format` is
//!    not `QRCode`/`DataMatrix` (case-insensitive) are reported as
//!    errors. Empty `content` rows are silently skipped.
//! 2. **DJI Mimo / DJI scanner** (legacy): header has `content` plus
//!    one of `pinned`/`_datetime`/`format`. `content` cell holds
//!    `SN:XXX SSID:YYY Password:ZZZ`; the SN is extracted and the
//!    SSID + password are routed into `device_metadata`.
//! 3. **Tello scanner**: exactly two columns, `Data` + `Timestamp`.
//!    The `Data` cell is the drone's WiFi MAC, **not** its serial
//!    number; the value is preserved on the row but flagged via
//!    `needs_manual_sn = true` and stashed in `device_metadata.raw_qr`.
//! 4. **Standard CSV**: header contains a `serial_number` column
//!    (case-insensitive, accepts `Serial Number` / `SerialNumber`).
//!    Optional `manufacturer` / `model` columns are passed through.
//! 5. **Generic / headerless** (fallback): legacy "one SN per cell of
//!    a `content` column or one per non-empty line" behaviour.
//!
//! Across all formats, extracted serial numbers are validated against
//! a permissive regex that rejects whitespace, URLs, JSON braces, and
//! non-ASCII junk. Within-file duplicates are dropped and counted.
//! Malformed rows are reported through `QrParseOutput::errors` with
//! their 1-indexed row number rather than aborting the parse, so the
//! preview UI can show the operator which rows were skipped.

use std::collections::HashMap;
use std::sync::LazyLock;

use regex::Regex;
use serde_json::{json, Value as JsonValue};

use crate::errors::AppError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CsvFormat {
    BinaryEye,
    DjiScanner,
    TelloScanner,
    StandardCsv,
    Generic,
}

#[derive(Debug, Clone)]
pub struct ParsedQrRow {
    pub serial_number: String,
    pub device_metadata: Option<JsonValue>,
    pub needs_manual_sn: bool,
    /// Pass-through from Standard CSV's optional column. None for
    /// QR-scanner formats.
    pub manufacturer: Option<String>,
    /// Pass-through from Standard CSV's optional column. None for
    /// QR-scanner formats.
    pub model: Option<String>,
}

#[derive(Debug, Clone)]
pub struct QrParseOutput {
    pub format: CsvFormat,
    pub rows: Vec<ParsedQrRow>,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
    /// Total non-header rows the parser scanned (includes ones that
    /// were silently skipped, deduped, or rejected).
    pub total_rows: usize,
    /// Within-file duplicates removed (preserving the first occurrence
    /// of each SN).
    pub duplicates_removed: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DjiQrParsed {
    pub serial_number: String,
    pub ssid: String,
    pub wifi_password: String,
}

static DJI_QR_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^SN:(?P<sn>\S+)\s+SSID:(?P<ssid>\S+)\s+Password:(?P<pwd>\S+)$").unwrap()
});

static TELLO_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^0TQ[A-Z0-9]{11}$").unwrap()
});

/// Permissive serial-number validator. Intentionally broad enough to
/// accept any plausible drone-equipment SN (DJI, Autel, Parrot, custom)
/// while rejecting obvious junk: whitespace, URLs, JSON, emoji,
/// non-ASCII bytes.
static SN_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[A-Za-z0-9][A-Za-z0-9._\-]{3,29}$").unwrap()
});

pub fn parse_dji_qr_content(s: &str) -> Option<DjiQrParsed> {
    DJI_QR_RE.captures(s.trim()).map(|caps| DjiQrParsed {
        serial_number: caps["sn"].to_string(),
        ssid: caps["ssid"].to_string(),
        wifi_password: caps["pwd"].to_string(),
    })
}

pub fn is_tello_qr_value(s: &str) -> bool {
    TELLO_RE.is_match(s.trim())
}

/// Normalisation step for QR `content` cells: if the content is a DJI
/// WiFi-pairing string (`SN:XXX SSID:YYY Password:ZZZ`), return just
/// the SN; otherwise return the trimmed input unchanged. Mavic Pro,
/// Tello, and some Mavic Mini variants encode the WiFi pairing string
/// on the same QR sticker that doubles as the SN label, so a Binary
/// Eye scan of those drones yields this format verbatim. Validation
/// stays downstream — this is purely a string-level normalisation.
fn extract_sn_from_qr_content(content: &str) -> &str {
    let trimmed = content.trim();
    if let Some(caps) = DJI_QR_RE.captures(trimmed) {
        // The named "sn" group is guaranteed by the DJI_QR_RE pattern.
        if let Some(m) = caps.name("sn") {
            return m.as_str();
        }
    }
    trimmed
}

/// Validate a candidate serial-number string. Returns a human-readable
/// rejection reason on failure so the preview UI can show it.
fn validate_sn(s: &str) -> Result<(), String> {
    if s.is_empty() {
        return Err("empty value".into());
    }
    if s.chars().any(char::is_whitespace) {
        return Err(format!("'{}' contains whitespace", s));
    }
    let lower = s.to_ascii_lowercase();
    if lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("www.")
        || lower.contains("://")
    {
        return Err(format!("'{}' looks like a URL", s));
    }
    if s.contains('{') || s.contains('}') {
        return Err(format!("'{}' looks like JSON", s));
    }
    if !s.is_ascii() {
        return Err(format!("'{}' contains non-ASCII characters", s));
    }
    if !SN_RE.is_match(s) {
        return Err(format!("'{}' is not a recognisable serial number", s));
    }
    Ok(())
}

/// Find a column index by exact case-insensitive header match.
fn find_header_idx(headers: &csv::StringRecord, name: &str) -> Option<usize> {
    headers
        .iter()
        .position(|col| col.trim().eq_ignore_ascii_case(name))
}

/// Find a column whose normalised name (lowercased, spaces and
/// underscores collapsed) matches `name`. Lets us accept "Serial
/// Number", "SerialNumber", and "serial_number" interchangeably.
fn find_header_idx_normalized(headers: &csv::StringRecord, name: &str) -> Option<usize> {
    let target: String = name
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '_')
        .flat_map(char::to_lowercase)
        .collect();
    headers.iter().position(|col| {
        let norm: String = col
            .chars()
            .filter(|c| !c.is_whitespace() && *c != '_')
            .flat_map(char::to_lowercase)
            .collect();
        norm == target
    })
}

pub fn detect_csv_format(header_line: &str) -> CsvFormat {
    let normalised: Vec<String> = header_line
        .split(',')
        .map(|h| h.trim().trim_matches('"').to_ascii_lowercase())
        .collect();

    let has = |name: &str| normalised.iter().any(|h| h == name);

    // Binary Eye export carries `content` + `format` +
    // `error_correction_level`. Check first because its header is a
    // strict superset of the legacy DJI scanner's signal columns.
    if has("content") && has("format") && has("error_correction_level") {
        return CsvFormat::BinaryEye;
    }

    // DJI scanner header observed: pinned,_datetime,format,name,content,...
    if has("content") && (has("pinned") || has("_datetime") || has("format")) {
        return CsvFormat::DjiScanner;
    }

    // Tello scanner header: Data,Timestamp (exactly two columns).
    if normalised.len() == 2 && has("data") && has("timestamp") {
        return CsvFormat::TelloScanner;
    }

    // Standard CSV: any of these column-name spellings counts.
    let has_normalised = |name: &str| {
        let target: String = name
            .chars()
            .filter(|c| !c.is_whitespace() && *c != '_')
            .flat_map(char::to_lowercase)
            .collect();
        normalised.iter().any(|h| {
            let norm: String = h
                .chars()
                .filter(|c| !c.is_whitespace() && *c != '_')
                .flat_map(char::to_lowercase)
                .collect();
            norm == target
        })
    };
    if has_normalised("serial_number") {
        return CsvFormat::StandardCsv;
    }

    CsvFormat::Generic
}

/// Human-friendly label for the detected format, exposed to the
/// frontend so the preview UI can name what it parsed.
pub fn format_display_name(format: CsvFormat) -> &'static str {
    match format {
        CsvFormat::BinaryEye => "Binary Eye export",
        CsvFormat::DjiScanner => "DJI Mimo QR scanner",
        CsvFormat::TelloScanner => "Tello QR scanner",
        CsvFormat::StandardCsv => "Standard CSV",
        CsvFormat::Generic => "Generic / headerless",
    }
}

/// Parse a batch-import CSV and emit one ParsedQrRow per drone.
pub fn parse_qr_csv(csv_bytes: &[u8]) -> Result<QrParseOutput, AppError> {
    let text = String::from_utf8_lossy(csv_bytes);
    let header_line = text.lines().next().unwrap_or("");
    let format = detect_csv_format(header_line);

    match format {
        CsvFormat::BinaryEye => parse_binary_eye(csv_bytes),
        CsvFormat::DjiScanner => parse_dji_scanner(csv_bytes),
        CsvFormat::TelloScanner => parse_tello_scanner(csv_bytes),
        CsvFormat::StandardCsv => parse_standard_csv(csv_bytes),
        CsvFormat::Generic => parse_generic(csv_bytes),
    }
}

fn parse_binary_eye(csv_bytes: &[u8]) -> Result<QrParseOutput, AppError> {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .flexible(true)
        .from_reader(csv_bytes);

    let headers = reader
        .headers()
        .map_err(|e| AppError::BadRequest(format!("CSV header error: {}", e)))?
        .clone();

    let content_idx = find_header_idx(&headers, "content").ok_or_else(|| {
        AppError::BadRequest("Binary Eye CSV is missing a 'content' column".into())
    })?;
    let format_idx = find_header_idx(&headers, "format");

    let mut rows: Vec<ParsedQrRow> = Vec::new();
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut errors: Vec<String> = Vec::new();
    let mut total_rows: usize = 0;
    let mut duplicates_removed: usize = 0;

    for (row_num, result) in reader.records().enumerate() {
        let record =
            result.map_err(|e| AppError::BadRequest(format!("CSV parse error: {}", e)))?;
        total_rows += 1;
        let display_row = row_num + 2;

        let raw = record.get(content_idx).map(|v| v.trim()).unwrap_or("");
        if raw.is_empty() {
            // Spec: empty content rows are silently skipped, not reported.
            continue;
        }

        // Format type filter (QRCode | DataMatrix). An empty `format`
        // cell is permitted because some DataMatrix codes leave it
        // blank in the wild; only an explicit non-matching value fails.
        if let Some(fi) = format_idx {
            if let Some(fmt_val) = record.get(fi) {
                let fv = fmt_val.trim();
                if !fv.is_empty()
                    && !fv.eq_ignore_ascii_case("qrcode")
                    && !fv.eq_ignore_ascii_case("datamatrix")
                {
                    errors.push(format!(
                        "Row {}: format '{}' is not QRCode or DataMatrix; skipped",
                        display_row, fv
                    ));
                    continue;
                }
            }
        }

        // Mavic Pro / Tello / some Mavic Mini variants encode the
        // SN as a DJI WiFi-pairing string on the QR sticker. Strip the
        // wrapper here so the validator sees just the SN.
        let candidate = extract_sn_from_qr_content(raw);

        if let Err(reason) = validate_sn(candidate) {
            errors.push(format!("Row {}: {}", display_row, reason));
            continue;
        }

        let sn = candidate.to_string();
        if seen.contains_key(&sn) {
            duplicates_removed += 1;
            continue;
        }
        seen.insert(sn.clone(), display_row);

        rows.push(ParsedQrRow {
            serial_number: sn,
            device_metadata: None,
            needs_manual_sn: false,
            manufacturer: None,
            model: None,
        });
    }

    Ok(QrParseOutput {
        format: CsvFormat::BinaryEye,
        rows,
        warnings: Vec::new(),
        errors,
        total_rows,
        duplicates_removed,
    })
}

fn parse_dji_scanner(csv_bytes: &[u8]) -> Result<QrParseOutput, AppError> {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .flexible(true)
        .from_reader(csv_bytes);

    let content_idx = reader
        .headers()
        .map_err(|e| AppError::BadRequest(format!("CSV header error: {}", e)))?
        .iter()
        .position(|col| col.trim().eq_ignore_ascii_case("content"));

    let content_idx = match content_idx {
        Some(i) => i,
        None => {
            return Err(AppError::BadRequest(
                "DJI scanner CSV is missing a 'content' column".into(),
            ))
        }
    };

    let mut rows = Vec::new();
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut errors = Vec::new();
    let mut total_rows: usize = 0;
    let mut duplicates_removed: usize = 0;

    for (row_num, result) in reader.records().enumerate() {
        let record =
            result.map_err(|e| AppError::BadRequest(format!("CSV parse error: {}", e)))?;
        total_rows += 1;
        let display_row = row_num + 2;

        let raw = match record.get(content_idx) {
            Some(v) => v.trim(),
            None => continue,
        };
        if raw.is_empty() {
            continue;
        }

        if let Some(parsed) = parse_dji_qr_content(raw) {
            if seen.contains_key(&parsed.serial_number) {
                duplicates_removed += 1;
                continue;
            }
            seen.insert(parsed.serial_number.clone(), display_row);
            rows.push(ParsedQrRow {
                serial_number: parsed.serial_number,
                device_metadata: Some(json!({
                    "ssid": parsed.ssid,
                    "wifi_password": parsed.wifi_password,
                    "source_format": "dji_qr_scanner_v1",
                })),
                needs_manual_sn: false,
                manufacturer: None,
                model: None,
            });
        } else {
            errors.push(format!(
                "Row {}: malformed DJI QR content, expected 'SN:XXX SSID:YYY Password:ZZZ'",
                display_row
            ));
        }
    }

    Ok(QrParseOutput {
        format: CsvFormat::DjiScanner,
        rows,
        warnings: Vec::new(),
        errors,
        total_rows,
        duplicates_removed,
    })
}

fn parse_tello_scanner(csv_bytes: &[u8]) -> Result<QrParseOutput, AppError> {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .flexible(true)
        .from_reader(csv_bytes);

    let data_idx = reader
        .headers()
        .map_err(|e| AppError::BadRequest(format!("CSV header error: {}", e)))?
        .iter()
        .position(|col| col.trim().eq_ignore_ascii_case("data"));

    let data_idx = match data_idx {
        Some(i) => i,
        None => {
            return Err(AppError::BadRequest(
                "Tello scanner CSV is missing a 'Data' column".into(),
            ))
        }
    };

    let mut rows = Vec::new();
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut warnings = Vec::new();
    let mut errors = Vec::new();
    let mut total_rows: usize = 0;
    let mut duplicates_removed: usize = 0;

    for (row_num, result) in reader.records().enumerate() {
        let record =
            result.map_err(|e| AppError::BadRequest(format!("CSV parse error: {}", e)))?;
        total_rows += 1;
        let display_row = row_num + 2;

        let raw = match record.get(data_idx) {
            Some(v) => v.trim().trim_matches('"').to_string(),
            None => continue,
        };
        if raw.is_empty() {
            continue;
        }

        if is_tello_qr_value(&raw) {
            if seen.contains_key(&raw) {
                duplicates_removed += 1;
                continue;
            }
            seen.insert(raw.clone(), display_row);
            rows.push(ParsedQrRow {
                // Tello QR doesn't yield an SN; we keep the WiFi MAC in
                // serial_number so the NOT NULL + UNIQUE constraints stay
                // satisfied, and flag the row for manual correction.
                serial_number: raw.clone(),
                device_metadata: Some(json!({
                    "raw_qr": raw,
                    "source_format": "tello_qr_scanner_v1",
                })),
                needs_manual_sn: true,
                manufacturer: None,
                model: None,
            });
            warnings.push(format!(
                "Row {}: Tello imported with placeholder SN — please correct in Fleet → Edit aircraft.",
                display_row
            ));
        } else {
            errors.push(format!(
                "Row {}: 'Data' value '{}' does not look like a Tello QR value",
                display_row, raw
            ));
        }
    }

    Ok(QrParseOutput {
        format: CsvFormat::TelloScanner,
        rows,
        warnings,
        errors,
        total_rows,
        duplicates_removed,
    })
}

fn parse_standard_csv(csv_bytes: &[u8]) -> Result<QrParseOutput, AppError> {
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(true)
        .flexible(true)
        .from_reader(csv_bytes);

    let headers = reader
        .headers()
        .map_err(|e| AppError::BadRequest(format!("CSV header error: {}", e)))?
        .clone();

    let sn_idx = find_header_idx_normalized(&headers, "serial_number").ok_or_else(|| {
        AppError::BadRequest("Standard CSV is missing 'serial_number' column".into())
    })?;
    let manufacturer_idx = find_header_idx_normalized(&headers, "manufacturer");
    let model_idx = find_header_idx_normalized(&headers, "model");

    let mut rows = Vec::new();
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut errors = Vec::new();
    let mut total_rows: usize = 0;
    let mut duplicates_removed: usize = 0;

    for (row_num, result) in reader.records().enumerate() {
        let record =
            result.map_err(|e| AppError::BadRequest(format!("CSV parse error: {}", e)))?;
        total_rows += 1;
        let display_row = row_num + 2;

        let raw = record.get(sn_idx).map(|v| v.trim()).unwrap_or("");
        if raw.is_empty() {
            continue;
        }

        if let Err(reason) = validate_sn(raw) {
            errors.push(format!("Row {}: {}", display_row, reason));
            continue;
        }

        let sn = raw.to_string();
        if seen.contains_key(&sn) {
            duplicates_removed += 1;
            continue;
        }
        seen.insert(sn.clone(), display_row);

        let manufacturer = manufacturer_idx
            .and_then(|i| record.get(i))
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty());
        let model = model_idx
            .and_then(|i| record.get(i))
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty());

        rows.push(ParsedQrRow {
            serial_number: sn,
            device_metadata: None,
            needs_manual_sn: false,
            manufacturer,
            model,
        });
    }

    Ok(QrParseOutput {
        format: CsvFormat::StandardCsv,
        rows,
        warnings: Vec::new(),
        errors,
        total_rows,
        duplicates_removed,
    })
}

fn parse_generic(csv_bytes: &[u8]) -> Result<QrParseOutput, AppError> {
    // Backward-compat path: a bare `content` column with no other
    // signal columns. Pre-feature behaviour treated this as "header +
    // one SN per content cell"; preserve that exactly.
    {
        let mut reader = csv::ReaderBuilder::new()
            .has_headers(true)
            .flexible(true)
            .from_reader(csv_bytes);
        if let Ok(headers) = reader.headers() {
            let headers = headers.clone();
            if let Some(idx) = find_header_idx(&headers, "content") {
                return collect_from_csv_column(reader, idx);
            }
        }
    }

    // No CSV header signal. Apply FORMAT C / FORMAT D heuristics to the
    // raw text: if there's only one column of data and the first line
    // validates as an SN, treat the file as headerless. Otherwise treat
    // the first line as a header and skip it.
    let text = String::from_utf8_lossy(csv_bytes).to_string();
    let lines: Vec<&str> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .collect();

    if lines.is_empty() {
        return Ok(empty_generic_output());
    }

    let first = lines[0].trim();
    let single_column = !first.contains(',');

    if single_column {
        let start = if validate_sn(first).is_ok() { 0 } else { 1 };
        return collect_lines_as_sns(&lines[start..]);
    }

    // Multi-column with no recognised header. Punt: treat each
    // non-empty line's first cell as a candidate SN.
    collect_lines_as_sns(&lines)
}

fn collect_from_csv_column(
    mut reader: csv::Reader<&[u8]>,
    col_idx: usize,
) -> Result<QrParseOutput, AppError> {
    let mut rows = Vec::new();
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut errors = Vec::new();
    let mut total_rows: usize = 0;
    let mut duplicates_removed: usize = 0;

    for (row_num, result) in reader.records().enumerate() {
        let record =
            result.map_err(|e| AppError::BadRequest(format!("CSV parse error: {}", e)))?;
        total_rows += 1;
        let display_row = row_num + 2;

        let raw = record.get(col_idx).map(|v| v.trim()).unwrap_or("");
        if raw.is_empty() {
            continue;
        }

        if let Err(reason) = validate_sn(raw) {
            errors.push(format!("Row {}: {}", display_row, reason));
            continue;
        }

        let sn = raw.to_string();
        if seen.contains_key(&sn) {
            duplicates_removed += 1;
            continue;
        }
        seen.insert(sn.clone(), display_row);

        rows.push(ParsedQrRow {
            serial_number: sn,
            device_metadata: None,
            needs_manual_sn: false,
            manufacturer: None,
            model: None,
        });
    }

    Ok(QrParseOutput {
        format: CsvFormat::Generic,
        rows,
        warnings: Vec::new(),
        errors,
        total_rows,
        duplicates_removed,
    })
}

fn collect_lines_as_sns(lines: &[&str]) -> Result<QrParseOutput, AppError> {
    let mut rows = Vec::new();
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut errors = Vec::new();
    let mut total_rows: usize = 0;
    let mut duplicates_removed: usize = 0;

    for (i, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        total_rows += 1;
        let display_row = i + 1;

        if let Err(reason) = validate_sn(trimmed) {
            errors.push(format!("Row {}: {}", display_row, reason));
            continue;
        }

        let sn = trimmed.to_string();
        if seen.contains_key(&sn) {
            duplicates_removed += 1;
            continue;
        }
        seen.insert(sn.clone(), display_row);

        rows.push(ParsedQrRow {
            serial_number: sn,
            device_metadata: None,
            needs_manual_sn: false,
            manufacturer: None,
            model: None,
        });
    }

    Ok(QrParseOutput {
        format: CsvFormat::Generic,
        rows,
        warnings: Vec::new(),
        errors,
        total_rows,
        duplicates_removed,
    })
}

fn empty_generic_output() -> QrParseOutput {
    QrParseOutput {
        format: CsvFormat::Generic,
        rows: Vec::new(),
        warnings: Vec::new(),
        errors: Vec::new(),
        total_rows: 0,
        duplicates_removed: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> Vec<u8> {
        let path = format!("tests/fixtures/qr_csv/{}", name);
        std::fs::read(&path).unwrap_or_else(|e| panic!("fixture {} unreadable: {}", path, e))
    }

    #[test]
    fn parses_dji_qr_content() {
        let row = "SN:0K1UF5S00E001D SSID:MAVIC_AIR-000001 Password:testpass2";
        let parsed = parse_dji_qr_content(row).unwrap();
        assert_eq!(parsed.serial_number, "0K1UF5S00E001D");
        assert_eq!(parsed.ssid, "MAVIC_AIR-000001");
        assert_eq!(parsed.wifi_password, "testpass2");
    }

    #[test]
    fn rejects_malformed_dji_content() {
        assert!(parse_dji_qr_content("just a random string").is_none());
        assert!(parse_dji_qr_content("SN:abc").is_none());
        assert!(parse_dji_qr_content("SN:abc SSID:xyz").is_none());
        assert!(parse_dji_qr_content("0K1UF5S00E001D").is_none());
    }

    #[test]
    fn detects_csv_format_from_header() {
        let bineye_header =
            "pinned,_datetime,format,name,content,error_correction_level,version,sequence_size,sequence_index,sequence_id";
        let dji_header_no_ecl = "pinned,_datetime,format,name,content";
        let tello_header = "Data,Timestamp";
        let standard_header = "Serial Number,Manufacturer,Model";
        let standard_header_alt = "serial_number,manufacturer";
        let standard_header_camel = "SerialNumber";
        // Binary Eye must win over DJI even though both share "content" + "format".
        assert_eq!(detect_csv_format(bineye_header), CsvFormat::BinaryEye);
        // DJI Mimo without error_correction_level still routes to DjiScanner.
        assert_eq!(detect_csv_format(dji_header_no_ecl), CsvFormat::DjiScanner);
        assert_eq!(detect_csv_format(tello_header), CsvFormat::TelloScanner);
        assert_eq!(detect_csv_format(standard_header), CsvFormat::StandardCsv);
        assert_eq!(detect_csv_format(standard_header_alt), CsvFormat::StandardCsv);
        assert_eq!(detect_csv_format(standard_header_camel), CsvFormat::StandardCsv);
        assert_eq!(detect_csv_format("foo,bar,baz"), CsvFormat::Generic);
        // A bare "content" column without DJI scanner sentinels is Generic
        // so the legacy plain-list flow keeps working.
        assert_eq!(detect_csv_format("content"), CsvFormat::Generic);
    }

    #[test]
    fn tello_pattern_matches_real_data() {
        for sample in &["0TQTEST0000031", "0TQTEST0000032", "0TQTEST0000033"] {
            assert!(is_tello_qr_value(sample), "should match: {}", sample);
        }
        assert!(!is_tello_qr_value("0K1UF5S00E001D"));
        assert!(!is_tello_qr_value("0TX1234567890A"));
        assert!(!is_tello_qr_value("0TQ123"));
    }

    // --- Binary Eye fixtures (modelled on real Binary Eye exports; all
    //     serial numbers, timestamps and Wi-Fi details are synthetic) ---

    #[test]
    fn binary_eye_inspire1_extracts_one_sn() {
        let out = parse_qr_csv(&fixture("uav-inspire1_x5.csv")).unwrap();
        assert_eq!(out.format, CsvFormat::BinaryEye);
        assert_eq!(out.rows.len(), 1);
        assert_eq!(out.errors.len(), 0);
        assert_eq!(out.duplicates_removed, 0);
        assert_eq!(out.total_rows, 1);
        assert_eq!(out.rows[0].serial_number, "W13TEST0000001");
        assert!(!out.rows[0].needs_manual_sn);
        assert!(out.rows[0].device_metadata.is_none());
    }

    #[test]
    fn binary_eye_ctr_mavicair_extracts_eight_sns() {
        let out = parse_qr_csv(&fixture("ctr-mavicair.csv")).unwrap();
        assert_eq!(out.format, CsvFormat::BinaryEye);
        assert_eq!(out.errors.len(), 0);
        assert_eq!(out.rows.len(), 8);
        let sns: Vec<&str> = out.rows.iter().map(|r| r.serial_number.as_str()).collect();
        assert_eq!(
            sns,
            vec![
                "0K2TEST0000011",
                "0K2TEST0000013",
                "0K2TEST0000012",
                "0K2TEST0000014",
                "0K2TEST0000015",
                "0K2TEST0000016",
                "0K2TEST0000017",
                "0K2TEST0000018",
            ]
        );
    }

    #[test]
    fn binary_eye_bat_mavicair_extracts_nine_sns() {
        let out = parse_qr_csv(&fixture("bat-mavicair.csv")).unwrap();
        assert_eq!(out.format, CsvFormat::BinaryEye);
        assert_eq!(out.rows.len(), 9);
        assert_eq!(out.errors.len(), 0);
        // Spot-check the row that has a different error_correction_level
        // ('Q' vs 'L') — must extract regardless of QR error tier.
        let sns: Vec<&str> = out.rows.iter().map(|r| r.serial_number.as_str()).collect();
        assert!(sns.contains(&"0K4TEST0000022"));
    }

    /// Regression guard: Binary Eye DataMatrix rows MUST be accepted.
    /// The fixture has format="DataMatrix" and an empty
    /// error_correction_level (16x16 codes have no QR-style ECL tier).
    #[test]
    fn binary_eye_datamatrix_row_is_accepted() {
        let out = parse_qr_csv(&fixture("99ctr-mavicpro.csv")).unwrap();
        assert_eq!(out.format, CsvFormat::BinaryEye);
        assert_eq!(out.rows.len(), 1, "DataMatrix row must NOT be filtered out");
        assert_eq!(out.errors.len(), 0);
        assert_eq!(out.rows[0].serial_number, "08WTEST0000005");
    }

    /// Within-file duplicates are dropped; empty `content` rows are
    /// silently skipped (NOT reported as errors).
    #[test]
    fn binary_eye_dedupes_within_file_and_silent_skips_empty() {
        let out = parse_qr_csv(&fixture("bineye_with_dupes.csv")).unwrap();
        assert_eq!(out.format, CsvFormat::BinaryEye);
        assert_eq!(out.rows.len(), 2, "two unique SNs survive");
        assert_eq!(out.duplicates_removed, 2, "two duplicate scans dropped");
        assert_eq!(out.errors.len(), 0, "empty content row is silent skip");
        assert_eq!(out.total_rows, 5, "all 5 body rows scanned");
        let sns: Vec<&str> = out.rows.iter().map(|r| r.serial_number.as_str()).collect();
        assert_eq!(sns, vec!["0K2TEST0000011", "0K2TEST0000012"]);
    }

    /// Mavic Pro / Tello / some Mavic Mini variants encode the SN as
    /// a DJI WiFi-pairing string on the QR sticker (the same QR doubles
    /// as both an SN label and a WiFi pairing code). Binary Eye writes
    /// the whole string into `content` verbatim; the parser must
    /// unwrap it before validation. Pre-fix, this fixture returned 0
    /// SNs with a "contains whitespace" error.
    #[test]
    fn binary_eye_mavic_pro_wifi_format_extracts_sn() {
        let out = parse_qr_csv(&fixture("99uav-mavicpro.csv")).unwrap();
        assert_eq!(out.format, CsvFormat::BinaryEye);
        assert_eq!(out.rows.len(), 1);
        assert_eq!(out.rows[0].serial_number, "08QTEST0000004");
        assert_eq!(out.errors.len(), 0);
    }

    /// Pure unit test of the helper: WiFi-format strings are unwrapped,
    /// bare SNs pass through, and trimming runs first.
    #[test]
    fn extract_sn_from_qr_content_handles_both_formats() {
        assert_eq!(
            extract_sn_from_qr_content("SN:08QTEST0000004 SSID:foo Password:bar"),
            "08QTEST0000004"
        );
        assert_eq!(
            extract_sn_from_qr_content("0K4TEST0000021"),
            "0K4TEST0000021"
        );
        assert_eq!(
            extract_sn_from_qr_content("  SN:ABC123 SSID:x Password:y  "),
            "ABC123"
        );
        assert_eq!(extract_sn_from_qr_content(""), "");
    }

    /// A single Binary Eye export can mix WiFi-format and bare-SN
    /// content cells (e.g. a Mavic Pro scanned alongside an Inspire 1).
    /// All three rows must land cleanly.
    #[test]
    fn binary_eye_mixed_content_formats_in_one_file() {
        let csv = b"pinned,_datetime,format,name,content,error_correction_level,version,sequence_size,sequence_index,sequence_id\n\
                    \"0\",\"2026-04-30 10:00:00:000\",\"QRCode\",,\"SN:08QTEST0000004 SSID:Mavic Password:abc\",\"M\",\"1\",\"-1\",\"-1\",\"\"\n\
                    \"0\",\"2026-04-30 10:00:01:000\",\"QRCode\",,\"0K4TEST0000021\",\"L\",\"1\",\"-1\",\"-1\",\"\"\n\
                    \"0\",\"2026-04-30 10:00:02:000\",\"QRCode\",,\"W13TEST0000001\",\"L\",\"1\",\"-1\",\"-1\",\"\"\n";
        let out = parse_qr_csv(csv).unwrap();
        assert_eq!(out.format, CsvFormat::BinaryEye);
        assert_eq!(out.errors.len(), 0);
        assert_eq!(out.duplicates_removed, 0);
        let sns: Vec<&str> = out.rows.iter().map(|r| r.serial_number.as_str()).collect();
        assert_eq!(sns, vec!["08QTEST0000004", "0K4TEST0000021", "W13TEST0000001"]);
    }

    /// Junk content (URL, www, JSON braces, embedded space, emoji) is
    /// rejected with a per-row reason; a valid SN in the same file
    /// still gets through.
    #[test]
    fn binary_eye_rejects_junk_with_reason() {
        let out = parse_qr_csv(&fixture("junk_rows.csv")).unwrap();
        assert_eq!(out.format, CsvFormat::BinaryEye);
        assert_eq!(out.rows.len(), 1, "only the one valid SN survives");
        assert_eq!(out.rows[0].serial_number, "0K2TEST0000011");
        assert_eq!(out.errors.len(), 5, "five junk rows reported");
        // Sanity check the rejection reasons reference the row number
        // and explain what was wrong.
        assert!(out.errors.iter().any(|e| e.contains("URL")));
        assert!(out.errors.iter().any(|e| e.contains("JSON")));
        assert!(out.errors.iter().any(|e| e.contains("whitespace")));
        assert!(out.errors.iter().any(|e| e.contains("non-ASCII")));
    }

    // --- Standard CSV (FORMAT B) ---

    #[test]
    fn standard_csv_extracts_sn_manufacturer_model() {
        let out = parse_qr_csv(&fixture("standard_csv.csv")).unwrap();
        assert_eq!(out.format, CsvFormat::StandardCsv);
        assert_eq!(out.rows.len(), 3);
        assert_eq!(out.errors.len(), 0);
        assert_eq!(out.rows[0].serial_number, "0K1UF5S00E001D");
        assert_eq!(out.rows[0].manufacturer.as_deref(), Some("DJI"));
        assert_eq!(out.rows[0].model.as_deref(), Some("Mavic Air"));
        assert_eq!(out.rows[2].manufacturer.as_deref(), Some("Autel"));
        assert_eq!(out.rows[2].model.as_deref(), Some("EVO Lite"));
    }

    // --- Headerless / FORMAT D ---

    #[test]
    fn headerless_plain_text_returns_three_sns() {
        let out = parse_qr_csv(&fixture("headerless_plain.csv")).unwrap();
        assert_eq!(out.format, CsvFormat::Generic);
        assert_eq!(out.rows.len(), 3);
        assert_eq!(out.errors.len(), 0);
        let sns: Vec<&str> = out.rows.iter().map(|r| r.serial_number.as_str()).collect();
        assert_eq!(
            sns,
            vec!["0K1UF5S00E001D", "0K2VF6S11F002E", "0K3WG7T22G003F"]
        );
    }

    // --- Legacy regression: existing pre-feature behaviour preserved ---

    #[test]
    fn parses_full_dji_scanner_csv() {
        let csv = b"pinned,_datetime,format,name,content\n\
                    ,2026-04-30T10:00:00Z,QR,Mavic1,SN:0K1UF5S00E001D SSID:MAVIC_AIR-000001 Password:testpass2\n\
                    ,2026-04-30T10:01:00Z,QR,Mavic2,SN:0K2VF6S11F002E SSID:MAVIC_AIR-aabbcc Password:deadbeef\n";
        let out = parse_qr_csv(csv).unwrap();
        assert_eq!(out.format, CsvFormat::DjiScanner);
        assert_eq!(out.rows.len(), 2);
        assert_eq!(out.errors.len(), 0);
        assert_eq!(out.rows[0].serial_number, "0K1UF5S00E001D");
        assert!(!out.rows[0].needs_manual_sn);
        let meta = out.rows[0].device_metadata.as_ref().unwrap();
        assert_eq!(meta["ssid"], "MAVIC_AIR-000001");
        assert_eq!(meta["wifi_password"], "testpass2");
        assert_eq!(meta["source_format"], "dji_qr_scanner_v1");
    }

    #[test]
    fn dji_scanner_csv_reports_malformed_rows() {
        let csv = b"pinned,_datetime,format,name,content\n\
                    ,2026-04-30T10:00:00Z,QR,Good,SN:0K1UF5S00E001D SSID:foo Password:bar\n\
                    ,2026-04-30T10:01:00Z,QR,Bad,RANDOM_GARBAGE\n";
        let out = parse_qr_csv(csv).unwrap();
        assert_eq!(out.rows.len(), 1);
        assert_eq!(out.errors.len(), 1);
        assert!(out.errors[0].contains("Row 3"));
        assert!(out.errors[0].contains("malformed DJI QR"));
    }

    #[test]
    fn parses_tello_scanner_csv() {
        let csv = b"Data,Timestamp\n\
                    \"0TQTEST0000031\",\"04/30/2026 12:49:49\"\n\
                    \"0TQTEST0000032\",\"04/30/2026 12:50:01\"\n";
        let out = parse_qr_csv(csv).unwrap();
        assert_eq!(out.format, CsvFormat::TelloScanner);
        assert_eq!(out.rows.len(), 2);
        assert!(out.rows.iter().all(|r| r.needs_manual_sn));
        assert_eq!(out.warnings.len(), 2);
        let meta = out.rows[0].device_metadata.as_ref().unwrap();
        assert_eq!(meta["raw_qr"], "0TQTEST0000031");
        assert_eq!(meta["source_format"], "tello_qr_scanner_v1");
        assert_eq!(out.rows[0].serial_number, "0TQTEST0000031");
    }

    #[test]
    fn generic_csv_with_content_header_keeps_legacy_behaviour() {
        let csv = b"content\n0K1UF5S00E001D\n0K2VF6S11F002E\n";
        let out = parse_qr_csv(csv).unwrap();
        assert_eq!(out.format, CsvFormat::Generic);
        assert_eq!(out.rows.len(), 2);
        assert!(out.rows.iter().all(|r| r.device_metadata.is_none()));
        assert!(out.rows.iter().all(|r| !r.needs_manual_sn));
    }

    #[test]
    fn generic_plain_list_still_works() {
        // Pre-feature behaviour: when there's no recognised header,
        // every non-empty line is treated as a serial number.
        let csv = b"0K1UF5S00E001D\n0K2VF6S11F002E\n0K3WG7T22G003F\n";
        let out = parse_qr_csv(csv).unwrap();
        assert_eq!(out.format, CsvFormat::Generic);
        assert_eq!(out.rows.len(), 3);
    }
}
