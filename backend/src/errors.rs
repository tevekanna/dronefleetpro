// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

use axum::{
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde_json::json;

#[derive(Debug)]
pub enum AppError {
    NotFound(String),
    Unauthorized(String),
    Forbidden(String),
    BadRequest(String),
    Conflict(String),
    /// A 409 that carries a machine-readable `code` alongside a human
    /// `message`, so the frontend can branch on the code (e.g.
    /// `requires_new_revision`) rather than parse prose. Body shape:
    /// `{ "error": <code>, "message": <message> }`.
    ConflictCode { code: String, message: String },
    /// Per-username rate-limit hit. The handler computes `retry_after_seconds`
    /// from the rolling-window state in `auth_attempts`. The IntoResponse
    /// impl mirrors the shape produced by `tower_governor` (the per-IP
    /// layer in main.rs::rate_limit_error_response) so the frontend can
    /// treat both 429 sources identically.
    RateLimited { retry_after_seconds: u64 },
    Database(sqlx::Error),
    Internal(anyhow::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        match self {
            AppError::NotFound(msg) => simple(StatusCode::NOT_FOUND, &msg),
            AppError::Unauthorized(msg) => simple(StatusCode::UNAUTHORIZED, &msg),
            AppError::Forbidden(msg) => simple(StatusCode::FORBIDDEN, &msg),
            AppError::BadRequest(msg) => simple(StatusCode::BAD_REQUEST, &msg),
            AppError::Conflict(msg) => simple(StatusCode::CONFLICT, &msg),
            AppError::ConflictCode { code, message } => (
                StatusCode::CONFLICT,
                Json(json!({ "error": code, "message": message })),
            )
                .into_response(),
            AppError::RateLimited {
                retry_after_seconds,
            } => rate_limited_response(retry_after_seconds),
            AppError::Database(e) => {
                tracing::error!(error = ?e, "database error");
                simple(StatusCode::INTERNAL_SERVER_ERROR, "Database error")
            }
            AppError::Internal(_) => simple(
                StatusCode::INTERNAL_SERVER_ERROR,
                "Internal server error",
            ),
        }
    }
}

fn simple(status: StatusCode, message: &str) -> Response {
    (status, Json(json!({ "error": message }))).into_response()
}

fn rate_limited_response(retry_after_seconds: u64) -> Response {
    let body = Json(json!({
        "error": "rate_limited",
        "retry_after_seconds": retry_after_seconds,
    }));
    let mut resp = (StatusCode::TOO_MANY_REQUESTS, body).into_response();
    if let Ok(v) = HeaderValue::try_from(retry_after_seconds.to_string()) {
        resp.headers_mut().insert(header::RETRY_AFTER, v);
    }
    resp
}

impl From<sqlx::Error> for AppError {
    fn from(e: sqlx::Error) -> Self {
        AppError::Database(e)
    }
}

impl From<anyhow::Error> for AppError {
    fn from(e: anyhow::Error) -> Self {
        AppError::Internal(e)
    }
}
