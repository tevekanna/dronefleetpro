// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

use printpdf::*;
use printpdf::path::{PaintMode, WindingOrder};
use sqlx::PgPool;
use std::io::BufWriter;

use crate::errors::AppError;

// --- A4 portrait constants ---
const PAGE_W_MM: f32 = 210.0;
const PAGE_H_MM: f32 = 297.0;

// --- Colors ---
const BRAND: (f32, f32, f32) = (0.055, 0.647, 0.914);
const HEADER_BG: (f32, f32, f32) = (0.04, 0.05, 0.07);
const TEXT_DARK: (f32, f32, f32) = (0.12, 0.12, 0.12);
const TEXT_MUTED: (f32, f32, f32) = (0.45, 0.45, 0.45);
const ROW_ALT: (f32, f32, f32) = (0.95, 0.96, 0.97);
const WHITE: (f32, f32, f32) = (1.0, 1.0, 1.0);

fn rgb(c: (f32, f32, f32)) -> Color { Color::Rgb(Rgb::new(c.0, c.1, c.2, None)) }

fn draw_rect(layer: &PdfLayerReference, x: f32, y: f32, w: f32, h: f32, color: (f32, f32, f32)) {
    let points = vec![
        (Point::new(Mm(x), Mm(y)), false),
        (Point::new(Mm(x + w), Mm(y)), false),
        (Point::new(Mm(x + w), Mm(y + h)), false),
        (Point::new(Mm(x), Mm(y + h)), false),
    ];
    let polygon = Polygon {
        rings: vec![points],
        mode: PaintMode::Fill,
        winding_order: WindingOrder::NonZero,
    };
    layer.set_fill_color(rgb(color));
    layer.add_polygon(polygon);
}

fn add_header(layer: &PdfLayerReference, font_bold: &IndirectFontRef, font: &IndirectFontRef, title: &str, org_name: &str) {
    draw_rect(layer, 0.0, 277.0, 210.0, 20.0, HEADER_BG);
    layer.set_fill_color(rgb(WHITE));
    layer.use_text("DroneFleetPro", 10.0, Mm(10.0), Mm(290.0), font);
    layer.use_text(title, 14.0, Mm(10.0), Mm(284.0), font_bold);
    layer.set_fill_color(Color::Rgb(Rgb::new(0.7, 0.7, 0.7, None)));
    layer.use_text(org_name, 9.0, Mm(130.0), Mm(290.0), font);
    let now = chrono::Utc::now().format("%Y-%m-%d %H:%M UTC").to_string();
    layer.use_text(&now, 9.0, Mm(130.0), Mm(284.0), font);
}

fn fmt_duration(secs: i64) -> String {
    let h = secs / 3600;
    let m = (secs % 3600) / 60;
    if h > 0 { format!("{}h {:02}m", h, m) } else { format!("{}m", m) }
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() > max { format!("{}...", &s[..max]) } else { s.to_string() }
}

fn draw_table_header(layer: &PdfLayerReference, font: &IndirectFontRef, cols: &[(&str, f32)], y: f32) {
    let mut x = 10.0;
    let h = 7.0;
    draw_rect(layer, 10.0, y - h, 190.0, h, BRAND);
    layer.set_fill_color(rgb(WHITE));
    for (label, w) in cols {
        layer.use_text(label.to_string(), 7.0, Mm(x + 1.0), Mm(y - 5.0), font);
        x += w;
    }
}

fn draw_table_row(layer: &PdfLayerReference, font: &IndirectFontRef, values: &[&str], widths: &[f32], y: f32, alt: bool) {
    let h = 6.5;
    if alt {
        draw_rect(layer, 10.0, y - h, 190.0, h, ROW_ALT);
    }
    layer.set_fill_color(rgb(TEXT_DARK));
    let mut x = 10.0;
    for (i, val) in values.iter().enumerate() {
        layer.use_text(val.to_string(), 7.0, Mm(x + 1.0), Mm(y - 4.5), font);
        x += widths[i];
    }
}

fn doc_to_bytes(doc: PdfDocumentReference) -> Result<Vec<u8>, AppError> {
    let mut buf = BufWriter::new(Vec::new());
    doc.save(&mut buf).map_err(|e| AppError::Internal(anyhow::anyhow!("PDF save error: {}", e)))?;
    Ok(buf.into_inner().map_err(|e| AppError::Internal(anyhow::anyhow!("PDF buffer error: {}", e)))?)
}

fn get_org_name_default() -> String { "DroneFleetPro".to_string() }

// ===== Flight Log Report =====
pub async fn generate_flight_log_report(pool: &PgPool, org_id: i32, aircraft_id: Option<i32>, pilot_id: Option<i32>) -> Result<Vec<u8>, AppError> {
    #[derive(sqlx::FromRow)]
    struct Row {
        flight_datetime: Option<chrono::DateTime<chrono::Utc>>,
        duration_seconds: i32,
        max_altitude_m: f32,
        easa_category: Option<String>,
        mission_type: Option<String>,
        log_source: Option<String>,
        aircraft_name: Option<String>,
        pic_name: Option<String>,
    }

    let rows = sqlx::query_as::<_, Row>(
        "SELECT f.flight_datetime, f.duration_seconds, f.max_altitude_m, f.easa_category, f.mission_type, f.log_source, COALESCE(f.aircraft_name, a.nickname, am.name) as aircraft_name, p.name as pic_name FROM flights f LEFT JOIN aircraft a ON f.aircraft_id = a.id LEFT JOIN aircraft_models am ON a.model_id = am.id LEFT JOIN pilots p ON f.pilot_id = p.id WHERE f.org_id = $1 AND f.deleted_at IS NULL AND ($2::int IS NULL OR f.aircraft_id = $2) AND ($3::int IS NULL OR f.pilot_id = $3) ORDER BY f.flight_datetime DESC"
    )
    .bind(org_id)
    .bind(aircraft_id)
    .bind(pilot_id)
    .fetch_all(pool)
    .await?;

    let total_flights = rows.len();
    let total_secs: i64 = rows.iter().map(|r| r.duration_seconds as i64).sum();

    let (doc, page, layer) = PdfDocument::new("Flight Log Report", Mm(210.0), Mm(297.0), "Layer 1");
    let font = doc.add_builtin_font(BuiltinFont::Helvetica).map_err(|e| AppError::Internal(anyhow::anyhow!("{}", e)))?;
    let font_bold = doc.add_builtin_font(BuiltinFont::HelveticaBold).map_err(|e| AppError::Internal(anyhow::anyhow!("{}", e)))?;
    let current_layer = doc.get_page(page).get_layer(layer);

    add_header(&current_layer, &font_bold, &font, "Flight Log Report", &get_org_name_default());

    // Summary
    current_layer.set_fill_color(rgb(TEXT_MUTED));
    current_layer.use_text(
        &format!("Total flights: {} | Total time: {} | Records: {}", total_flights, fmt_duration(total_secs), total_flights),
        8.0, Mm(10.0), Mm(272.0), &font,
    );

    let cols: Vec<(&str, f32)> = vec![("Date", 25.0), ("Aircraft", 30.0), ("Pilot", 28.0), ("Duration", 20.0), ("Max Alt", 18.0), ("EASA", 25.0), ("Mission", 25.0), ("Source", 19.0)];
    let widths: Vec<f32> = cols.iter().map(|(_, w)| *w).collect();
    let mut y = 265.0;

    draw_table_header(&current_layer, &font_bold, &cols, y);
    y -= 7.0;

    for (i, r) in rows.iter().enumerate() {
        if y < 20.0 { break; } // simplified: no multi-page for now
        let date = r.flight_datetime.map(|d| d.format("%Y-%m-%d").to_string()).unwrap_or_else(|| "—".into());
        let aircraft = r.aircraft_name.as_deref().unwrap_or("—");
        let pilot = r.pic_name.as_deref().unwrap_or("—");
        let dur = fmt_duration(r.duration_seconds as i64);
        let alt = format!("{:.0}m", r.max_altitude_m);
        let easa = r.easa_category.as_deref().unwrap_or("—");
        let mission = r.mission_type.as_deref().unwrap_or("—");
        let source = match r.log_source.as_deref() { Some("dji_import") => "DJI", Some("manual") => "Manual", _ => "—" };
        let vals: Vec<&str> = vec![&date, aircraft, pilot, &dur, &alt, easa, mission, source];
        draw_table_row(&current_layer, &font, &vals, &widths, y, i % 2 == 1);
        y -= 6.5;
    }

    doc_to_bytes(doc)
}

// ===== Maintenance Report =====
pub async fn generate_maintenance_report(pool: &PgPool, org_id: i32, _aircraft_id: Option<i32>) -> Result<Vec<u8>, AppError> {
    #[derive(sqlx::FromRow)]
    struct Row {
        performed_date: chrono::NaiveDate,
        aircraft_name: Option<String>,
        maint_category: Option<String>,
        maint_type: Option<String>,
        created_by: String,
        remark: Option<String>,
        sign_off: String,
    }

    let rows = sqlx::query_as::<_, Row>(
        "SELECT m.performed_date, COALESCE(a.nickname, am.name, a.serial_number) as aircraft_name, m.maint_category, m.maint_type, m.created_by, m.remark, m.sign_off FROM maintenance m LEFT JOIN aircraft a ON m.aircraft_id = a.id LEFT JOIN aircraft_models am ON a.model_id = am.id WHERE m.org_id = $1 ORDER BY m.performed_date DESC"
    )
    .bind(org_id)
    .fetch_all(pool)
    .await?;

    let (doc, page, layer) = PdfDocument::new("Maintenance Report", Mm(210.0), Mm(297.0), "Layer 1");
    let font = doc.add_builtin_font(BuiltinFont::Helvetica).map_err(|e| AppError::Internal(anyhow::anyhow!("{}", e)))?;
    let font_bold = doc.add_builtin_font(BuiltinFont::HelveticaBold).map_err(|e| AppError::Internal(anyhow::anyhow!("{}", e)))?;
    let current_layer = doc.get_page(page).get_layer(layer);

    add_header(&current_layer, &font_bold, &font, "Maintenance Report", &get_org_name_default());

    current_layer.set_fill_color(rgb(TEXT_MUTED));
    current_layer.use_text(&format!("Total records: {} | EASA OSO #3 Compliant", rows.len()), 8.0, Mm(10.0), Mm(272.0), &font);

    let cols: Vec<(&str, f32)> = vec![("Date", 22.0), ("Aircraft", 25.0), ("Category", 22.0), ("Type", 20.0), ("By", 18.0), ("Remark", 65.0), ("Sign", 18.0)];
    let widths: Vec<f32> = cols.iter().map(|(_, w)| *w).collect();
    let mut y = 265.0;

    draw_table_header(&current_layer, &font_bold, &cols, y);
    y -= 7.0;

    for (i, r) in rows.iter().enumerate() {
        if y < 20.0 { break; }
        let date = r.performed_date.format("%Y-%m-%d").to_string();
        let ac = r.aircraft_name.as_deref().unwrap_or("—");
        let cat = r.maint_category.as_deref().unwrap_or("—");
        let typ = r.maint_type.as_deref().unwrap_or("—");
        let rem = truncate(r.remark.as_deref().unwrap_or("—"), 50);
        let vals: Vec<&str> = vec![&date, ac, cat, typ, &r.created_by, &rem, &r.sign_off];
        draw_table_row(&current_layer, &font, &vals, &widths, y, i % 2 == 1);
        y -= 6.5;
    }

    doc_to_bytes(doc)
}

// ===== Pilot Logbook =====
pub async fn generate_pilot_logbook(pool: &PgPool, org_id: i32, pilot_id: i32) -> Result<Vec<u8>, AppError> {
    #[derive(sqlx::FromRow)]
    struct PilotRow { name: String, license_type: Option<String>, license_number: Option<String>, license_expires: Option<chrono::NaiveDate> }

    #[derive(sqlx::FromRow)]
    struct FlightRow {
        flight_datetime: Option<chrono::DateTime<chrono::Utc>>,
        duration_seconds: i32,
        max_altitude_m: f32,
        easa_category: Option<String>,
        mission_type: Option<String>,
        aircraft_name: Option<String>,
    }

    let pilot = sqlx::query_as::<_, PilotRow>("SELECT name, license_type, license_number, license_expires FROM pilots WHERE id = $1 AND org_id = $2")
        .bind(pilot_id).bind(org_id).fetch_optional(pool).await?
        .ok_or_else(|| AppError::NotFound("Pilot not found".into()))?;

    let flights = sqlx::query_as::<_, FlightRow>(
        "SELECT f.flight_datetime, f.duration_seconds, f.max_altitude_m, f.easa_category, f.mission_type, COALESCE(f.aircraft_name, a.nickname, am.name) as aircraft_name FROM flights f LEFT JOIN aircraft a ON f.aircraft_id = a.id LEFT JOIN aircraft_models am ON a.model_id = am.id WHERE f.pilot_id = $1 AND f.org_id = $2 AND f.deleted_at IS NULL ORDER BY f.flight_datetime DESC"
    )
    .bind(pilot_id).bind(org_id).fetch_all(pool).await?;

    let total_secs: i64 = flights.iter().map(|f| f.duration_seconds as i64).sum();

    let (doc, page, layer) = PdfDocument::new("Pilot Logbook", Mm(210.0), Mm(297.0), "Layer 1");
    let font = doc.add_builtin_font(BuiltinFont::Helvetica).map_err(|e| AppError::Internal(anyhow::anyhow!("{}", e)))?;
    let font_bold = doc.add_builtin_font(BuiltinFont::HelveticaBold).map_err(|e| AppError::Internal(anyhow::anyhow!("{}", e)))?;
    let current_layer = doc.get_page(page).get_layer(layer);

    let title = format!("Pilot Logbook — {}", pilot.name);
    add_header(&current_layer, &font_bold, &font, &title, &get_org_name_default());

    // Pilot info
    current_layer.set_fill_color(rgb(TEXT_DARK));
    let mut iy = 270.0;
    current_layer.use_text(&format!("Name: {}", pilot.name), 9.0, Mm(10.0), Mm(iy), &font); iy -= 5.0;
    current_layer.use_text(&format!("License: {} | {}", pilot.license_type.as_deref().unwrap_or("—"), pilot.license_number.as_deref().unwrap_or("—")), 9.0, Mm(10.0), Mm(iy), &font); iy -= 5.0;
    current_layer.use_text(&format!("Expires: {}", pilot.license_expires.map(|d| d.format("%Y-%m-%d").to_string()).unwrap_or_else(|| "—".into())), 9.0, Mm(10.0), Mm(iy), &font); iy -= 5.0;
    current_layer.use_text(&format!("Total Flights: {} | Total Time: {}", flights.len(), fmt_duration(total_secs)), 9.0, Mm(10.0), Mm(iy), &font_bold); iy -= 8.0;

    let cols: Vec<(&str, f32)> = vec![("Date", 28.0), ("Aircraft", 35.0), ("Duration", 22.0), ("Max Alt", 20.0), ("EASA", 30.0), ("Mission", 30.0)];
    let widths: Vec<f32> = cols.iter().map(|(_, w)| *w).collect();

    draw_table_header(&current_layer, &font_bold, &cols, iy);
    iy -= 7.0;

    for (i, f) in flights.iter().enumerate() {
        if iy < 20.0 { break; }
        let date = f.flight_datetime.map(|d| d.format("%Y-%m-%d").to_string()).unwrap_or_else(|| "—".into());
        let ac = f.aircraft_name.as_deref().unwrap_or("—");
        let dur = fmt_duration(f.duration_seconds as i64);
        let alt = format!("{:.0}m", f.max_altitude_m);
        let easa = f.easa_category.as_deref().unwrap_or("—");
        let mission = f.mission_type.as_deref().unwrap_or("—");
        let vals: Vec<&str> = vec![&date, ac, &dur, &alt, easa, mission];
        draw_table_row(&current_layer, &font, &vals, &widths, iy, i % 2 == 1);
        iy -= 6.5;
    }

    doc_to_bytes(doc)
}

// ===== Equipment ID Sheet =====

/// Renders a QR code for `data` as an uncompressed 8-bit greyscale bitmap
/// (black = 0, white = 255) and returns `(pixel_buffer, side_length_px)`.
///
/// `scale` is the number of pixels per QR module. `quiet_modules` adds that
/// many white modules of padding around the code on every side — the QR
/// specification requires ≥ 4 modules for reliable decoding. The `qrcode`
/// crate's `to_colors()` returns the raw module grid only, with no quiet
/// zone, so we add it here.
fn qr_to_greyscale(data: &str, scale: usize, quiet_modules: usize) -> Result<(Vec<u8>, u32), AppError> {
    let qr = qrcode::QrCode::new(data.as_bytes())
        .map_err(|e| AppError::Internal(anyhow::anyhow!("QR encode failed for '{}': {}", data, e)))?;
    let modules_per_side = qr.width();            // data modules per side (no quiet zone)
    let colors = qr.to_colors();                  // row-major, Dark=black, Light=white
    let total_modules = modules_per_side + 2 * quiet_modules;
    let side_px = total_modules * scale;
    let offset_px = quiet_modules * scale;
    let mut buf = vec![255u8; side_px * side_px]; // start all white — quiet zone is free
    for my in 0..modules_per_side {
        for mx in 0..modules_per_side {
            if colors[my * modules_per_side + mx] == qrcode::Color::Dark {
                for py in 0..scale {
                    for px in 0..scale {
                        let row = offset_px + my * scale + py;
                        let col = offset_px + mx * scale + px;
                        buf[row * side_px + col] = 0;
                    }
                }
            }
        }
    }
    Ok((buf, side_px as u32))
}

/// Draws a QR for `data` onto `layer` as a `size_mm` × `size_mm` square with
/// its bottom-left corner at `(x_mm, y_mm)`. The rendered square includes
/// the QR's own quiet zone, so `size_mm` is the total footprint.
fn draw_qr(
    layer: &PdfLayerReference,
    data: &str,
    x_mm: f32,
    y_mm: f32,
    size_mm: f32,
) -> Result<(), AppError> {
    // 4 px/module gives a crisp render for typical short strings.
    let (pixels, side_px) = qr_to_greyscale(data, 4, QR_QUIET_ZONE_MODULES)?;
    let img_x = ImageXObject {
        width: Px(side_px as usize),
        height: Px(side_px as usize),
        color_space: ColorSpace::Greyscale,
        bits_per_component: ColorBits::Bit8,
        interpolate: false,          // keep hard edges so scanners can read it
        image_data: pixels,
        image_filter: None,
        clipping_bbox: None,
        smask: None,
    };
    // Physical size at dpi=300: width_mm = side_px / 300 * 25.4.
    // scale compensates so final size is exactly `size_mm`.
    let native_mm = (side_px as f32) / 300.0 * 25.4;
    let scale = size_mm / native_mm;
    Image::from(img_x).add_to_layer(
        layer.clone(),
        ImageTransform {
            translate_x: Some(Mm(x_mm)),
            translate_y: Some(Mm(y_mm)),
            rotate: None,
            scale_x: Some(scale),
            scale_y: Some(scale),
            dpi: Some(300.0),
        },
    );
    Ok(())
}

#[derive(sqlx::FromRow)]
struct EqRow {
    equipment_id: Option<String>,
    model: Option<String>,
    serial_number: String,
}

fn eq_section_header(layer: &PdfLayerReference, font_bold: &IndirectFontRef, title: &str, y: f32) {
    // Brand band + white title
    draw_rect(layer, 10.0, y - 10.0, 190.0, 10.0, BRAND);
    layer.set_fill_color(rgb(WHITE));
    layer.use_text(title, 12.0, Mm(13.0), Mm(y - 7.0), font_bold);
}

/// Column layout for an equipment row, right of the left margin at x=10mm:
/// Model (50 mm) | Serial (50 mm) | Equipment ID (30 mm) | gap | QR 16×16 mm.
///
/// Row height is 26 mm and the QR is 16 mm, so each QR is vertically centred
/// with 5 mm of clear space above and below — enough separation from
/// adjacent rows' QR finder patterns that field scanners don't misread one
/// row for another. The QR itself includes a 4-module quiet zone
/// (see `QR_QUIET_ZONE_MODULES`).
const ROW_HEIGHT_MM: f32 = 26.0;
const QR_SIZE_MM: f32 = 16.0;
const QR_QUIET_ZONE_MODULES: usize = 4;
const QR_RIGHT_PADDING_MM: f32 = 3.0;

const EQ_MODEL_W: f32 = 50.0;
const EQ_SERIAL_W: f32 = 50.0;
const EQ_EID_W: f32 = 30.0;

fn draw_eq_table_header(layer: &PdfLayerReference, font_bold: &IndirectFontRef, y: f32) {
    draw_rect(layer, 10.0, y - 7.0, 190.0, 7.0, HEADER_BG);
    layer.set_fill_color(rgb(WHITE));
    layer.use_text("Model",        8.0, Mm(12.0),                                             Mm(y - 5.0), font_bold);
    layer.use_text("Serial Number", 8.0, Mm(12.0 + EQ_MODEL_W),                                Mm(y - 5.0), font_bold);
    layer.use_text("Equipment ID", 8.0, Mm(12.0 + EQ_MODEL_W + EQ_SERIAL_W),                  Mm(y - 5.0), font_bold);
    layer.use_text("QR",           8.0, Mm(12.0 + EQ_MODEL_W + EQ_SERIAL_W + EQ_EID_W + 5.0), Mm(y - 5.0), font_bold);
}

/// Draws one row starting with its top edge at `y`. Returns the y value of
/// the NEXT row's top edge.
fn draw_eq_row(
    layer: &PdfLayerReference,
    font: &IndirectFontRef,
    font_bold: &IndirectFontRef,
    r: &EqRow,
    y: f32,
    alt: bool,
) -> Result<f32, AppError> {
    if alt {
        draw_rect(layer, 10.0, y - ROW_HEIGHT_MM, 190.0, ROW_HEIGHT_MM, ROW_ALT);
    }
    layer.set_fill_color(rgb(TEXT_DARK));
    let text_y = y - ROW_HEIGHT_MM / 2.0 - 1.0;
    let model = r.model.as_deref().unwrap_or("—");
    let eid = r.equipment_id.as_deref().unwrap_or("—");
    layer.use_text(truncate(model, 36),        9.0, Mm(12.0),                                             Mm(text_y), font);
    layer.use_text(truncate(&r.serial_number, 36), 9.0, Mm(12.0 + EQ_MODEL_W),                            Mm(text_y), font);
    layer.use_text(eid,                        10.0, Mm(12.0 + EQ_MODEL_W + EQ_SERIAL_W),                 Mm(text_y), font_bold);
    // QR: right edge is QR_RIGHT_PADDING_MM inside the 200 mm content anchor.
    // Vertical centre within the row ⇒ (ROW_HEIGHT_MM − QR_SIZE_MM)/2 clearance
    // above and below the QR (≈ 5 mm each side for 26 mm row / 16 mm QR).
    let qr_x = 200.0 - QR_SIZE_MM - QR_RIGHT_PADDING_MM;
    let qr_y = y - ROW_HEIGHT_MM + (ROW_HEIGHT_MM - QR_SIZE_MM) / 2.0;
    draw_qr(layer, eid, qr_x, qr_y, QR_SIZE_MM)?;
    Ok(y - ROW_HEIGHT_MM)
}

fn new_page(
    doc: &PdfDocumentReference,
    font_bold: &IndirectFontRef,
    font: &IndirectFontRef,
    title: &str,
) -> (PdfLayerReference, f32) {
    let (page, layer_idx) = doc.add_page(Mm(PAGE_W_MM), Mm(PAGE_H_MM), "Layer");
    let layer = doc.get_page(page).get_layer(layer_idx);
    add_header(&layer, font_bold, font, title, &get_org_name_default());
    // Content starts 10 mm below the header band (header fills top 20 mm).
    (layer, 270.0)
}

fn render_eq_section(
    doc: &PdfDocumentReference,
    first_layer: &PdfLayerReference,
    font: &IndirectFontRef,
    font_bold: &IndirectFontRef,
    title: &str,
    section_name: &str,
    rows: &[EqRow],
    starting_y: f32,
) -> Result<(), AppError> {
    // Section heading band + in-section table header on the CURRENT page.
    let mut layer = first_layer.clone();
    let mut y = starting_y;
    eq_section_header(&layer, font_bold, title, y);
    y -= 14.0;

    if rows.is_empty() {
        layer.set_fill_color(rgb(TEXT_MUTED));
        layer.use_text(
            format!("No active {} registered.", section_name),
            9.0,
            Mm(12.0),
            Mm(y - 5.0),
            font,
        );
        return Ok(());
    }

    draw_eq_table_header(&layer, font_bold, y);
    y -= 7.0;

    for (i, row) in rows.iter().enumerate() {
        // Need room for a full row above the 15 mm bottom margin.
        if y - ROW_HEIGHT_MM < 15.0 {
            let (new_layer, new_y) = new_page(doc, font_bold, font, "Equipment ID Sheet");
            layer = new_layer;
            y = new_y;
            // Repeat the section heading + table header on the new page.
            eq_section_header(&layer, font_bold, title, y);
            y -= 14.0;
            draw_eq_table_header(&layer, font_bold, y);
            y -= 7.0;
        }
        y = draw_eq_row(&layer, font, font_bold, row, y, i % 2 == 1)?;
    }
    Ok(())
}

pub async fn generate_equipment_id_sheet(pool: &PgPool, org_id: i32) -> Result<Vec<u8>, AppError> {
    // Three queries, one per resource. Sort by equipment_id so the sheet
    // matches the physical labelling order. Rows with NULL equipment_id sort
    // last (NULLS LAST) so the labelled items come first.
    let aircraft: Vec<EqRow> = sqlx::query_as::<_, EqRow>(
        "SELECT a.equipment_id, am.name AS model, a.serial_number FROM aircraft a \
         LEFT JOIN aircraft_models am ON a.model_id = am.id \
         WHERE a.org_id = $1 AND a.status = 'active' \
         ORDER BY a.equipment_id NULLS LAST, a.serial_number"
    ).bind(org_id).fetch_all(pool).await?;

    let controllers: Vec<EqRow> = sqlx::query_as::<_, EqRow>(
        "SELECT equipment_id, model, serial_number FROM remote_controllers \
         WHERE org_id = $1 AND status = 'active' \
         ORDER BY equipment_id NULLS LAST, serial_number"
    ).bind(org_id).fetch_all(pool).await?;

    // Batteries don't have a `model` column; their model is the `compatible_model`
    // free-text field. Aliased so the common EqRow struct works for all three.
    let batteries: Vec<EqRow> = sqlx::query_as::<_, EqRow>(
        "SELECT equipment_id, compatible_model AS model, serial_number FROM batteries \
         WHERE org_id = $1 AND status = 'active' \
         ORDER BY equipment_id NULLS LAST, serial_number"
    ).bind(org_id).fetch_all(pool).await?;

    let (doc, page1, layer1) = PdfDocument::new(
        "Equipment ID Sheet", Mm(PAGE_W_MM), Mm(PAGE_H_MM), "Layer"
    );
    let font = doc.add_builtin_font(BuiltinFont::Helvetica)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("{}", e)))?;
    let font_bold = doc.add_builtin_font(BuiltinFont::HelveticaBold)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("{}", e)))?;
    let layer = doc.get_page(page1).get_layer(layer1);
    add_header(&layer, &font_bold, &font, "Equipment ID Sheet", &get_org_name_default());

    // Page 1 = Aircraft section.
    render_eq_section(&doc, &layer, &font, &font_bold, "Aircraft", "aircraft", &aircraft, 270.0)?;

    // Each subsequent section starts on its own page.
    let (l2, y2) = new_page(&doc, &font_bold, &font, "Equipment ID Sheet");
    render_eq_section(&doc, &l2, &font, &font_bold, "Controllers", "controllers", &controllers, y2)?;

    let (l3, y3) = new_page(&doc, &font_bold, &font, "Equipment ID Sheet");
    render_eq_section(&doc, &l3, &font, &font_bold, "Batteries", "batteries", &batteries, y3)?;

    doc_to_bytes(doc)
}

// ===== Fleet Status Report =====
pub async fn generate_fleet_status_report(pool: &PgPool, org_id: i32) -> Result<Vec<u8>, AppError> {
    #[derive(sqlx::FromRow)]
    struct Row {
        equipment_id: Option<String>,
        nickname: Option<String>,
        model: Option<String>,
        serial_number: String,
        status: String,
        flight_count: Option<i64>,
        total_seconds: Option<i64>,
    }

    let rows = sqlx::query_as::<_, Row>(
        "SELECT a.equipment_id, a.nickname, am.name AS model, a.serial_number, a.status, COUNT(f.id) as flight_count, COALESCE(SUM(f.duration_seconds), 0) as total_seconds FROM aircraft a LEFT JOIN aircraft_models am ON a.model_id = am.id LEFT JOIN flights f ON a.id = f.aircraft_id AND f.deleted_at IS NULL WHERE a.org_id = $1 GROUP BY a.id, am.name ORDER BY a.nickname"
    )
    .bind(org_id)
    .fetch_all(pool)
    .await?;

    let (doc, page, layer) = PdfDocument::new("Fleet Status Report", Mm(210.0), Mm(297.0), "Layer 1");
    let font = doc.add_builtin_font(BuiltinFont::Helvetica).map_err(|e| AppError::Internal(anyhow::anyhow!("{}", e)))?;
    let font_bold = doc.add_builtin_font(BuiltinFont::HelveticaBold).map_err(|e| AppError::Internal(anyhow::anyhow!("{}", e)))?;
    let current_layer = doc.get_page(page).get_layer(layer);

    add_header(&current_layer, &font_bold, &font, "Fleet Status Report", &get_org_name_default());

    current_layer.set_fill_color(rgb(TEXT_MUTED));
    current_layer.use_text(&format!("Total aircraft: {}", rows.len()), 8.0, Mm(10.0), Mm(272.0), &font);

    let cols: Vec<(&str, f32)> = vec![("Equip ID", 25.0), ("Model", 45.0), ("Serial", 35.0), ("Status", 20.0), ("Flights", 15.0), ("Total Time", 25.0)];
    let widths: Vec<f32> = cols.iter().map(|(_, w)| *w).collect();
    let mut y = 265.0;

    draw_table_header(&current_layer, &font_bold, &cols, y);
    y -= 7.0;

    for (i, r) in rows.iter().enumerate() {
        if y < 20.0 { break; }
        let eid = r.equipment_id.as_deref().unwrap_or("—");
        let name = r.model.as_deref().unwrap_or("—");
        let fc = r.flight_count.unwrap_or(0).to_string();
        let tt = fmt_duration(r.total_seconds.unwrap_or(0));
        let vals: Vec<&str> = vec![eid, name, &r.serial_number, &r.status, &fc, &tt];
        draw_table_row(&current_layer, &font, &vals, &widths, y, i % 2 == 1);
        y -= 6.5;
    }

    doc_to_bytes(doc)
}
