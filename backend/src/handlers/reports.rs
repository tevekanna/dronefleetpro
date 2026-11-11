// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

use axum::{
    extract::{Query, State},
    response::{IntoResponse, Response},
    http::{header, StatusCode},
};
use serde::Deserialize;

use crate::auth::middleware::Claims;
use crate::db::DbPool;
use crate::errors::AppError;

#[derive(Debug, Deserialize, Default)]
pub struct ReportParams {
    pub aircraft_id: Option<i32>,
    pub pilot_id: Option<i32>,
}

fn pdf_response(bytes: Vec<u8>, filename: &str) -> Response {
    // `inline` so browsers / iframes render the PDF without forcing a download.
    // The frontend PdfViewer component still offers an explicit Download button
    // that writes the same blob to disk via a temporary <a download> element.
    let content_disposition = format!("inline; filename=\"{}\"", filename);
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/pdf".to_string()),
            (header::CONTENT_DISPOSITION, content_disposition),
        ],
        bytes,
    ).into_response()
}

/// GET /api/v1/reports/flights — Flight log PDF report
pub async fn flight_log_report(
    claims: Claims,
    State(pool): State<DbPool>,
    Query(params): Query<ReportParams>,
) -> Result<Response, AppError> {
    let bytes = crate::services::pdf::generate_flight_log_report(&pool, claims.org_id, params.aircraft_id, params.pilot_id).await?;
    Ok(pdf_response(bytes, "flight-log.pdf"))
}

/// GET /api/v1/reports/maintenance — Maintenance record PDF report
pub async fn maintenance_report(
    claims: Claims,
    State(pool): State<DbPool>,
    Query(params): Query<ReportParams>,
) -> Result<Response, AppError> {
    let bytes = crate::services::pdf::generate_maintenance_report(&pool, claims.org_id, params.aircraft_id).await?;
    Ok(pdf_response(bytes, "maintenance-report.pdf"))
}

/// GET /api/v1/reports/pilot-logbook — Individual pilot logbook PDF
pub async fn pilot_logbook(
    claims: Claims,
    State(pool): State<DbPool>,
    Query(params): Query<ReportParams>,
) -> Result<Response, AppError> {
    let pilot_id = params.pilot_id.ok_or_else(|| AppError::BadRequest("pilot_id query parameter is required".into()))?;
    let bytes = crate::services::pdf::generate_pilot_logbook(&pool, claims.org_id, pilot_id).await?;
    Ok(pdf_response(bytes, "pilot-logbook.pdf"))
}

/// GET /api/v1/reports/fleet — Fleet status PDF report
pub async fn fleet_status_report(
    claims: Claims,
    State(pool): State<DbPool>,
) -> Result<Response, AppError> {
    let bytes = crate::services::pdf::generate_fleet_status_report(&pool, claims.org_id).await?;
    Ok(pdf_response(bytes, "fleet-status.pdf"))
}

/// GET /api/v1/reports/equipment-ids — Equipment ID labels sheet (all active
/// aircraft / controllers / batteries with QR codes of their equipment_id).
pub async fn equipment_id_sheet(
    claims: Claims,
    State(pool): State<DbPool>,
) -> Result<Response, AppError> {
    let bytes = crate::services::pdf::generate_equipment_id_sheet(&pool, claims.org_id).await?;
    Ok(pdf_response(bytes, "equipment-id-sheet.pdf"))
}
