// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

//! Read-only endpoint serving the Annex VI occurrence taxonomy.

use axum::Json;
use serde::Serialize;

use crate::auth::middleware::Claims;
use crate::errors::AppError;
use crate::services::occurrence_taxonomy::{OccurrenceCategory, OCCURRENCE_TAXONOMY, SOURCE};

#[derive(Debug, Serialize)]
pub struct OccurrenceTaxonomyResponse {
    pub source: &'static str,
    pub categories: &'static [OccurrenceCategory],
}

/// GET /api/v1/occurrence-taxonomy — the full Annex VI taxonomy plus its
/// source citation. Authenticated users only, no role gate: this is public
/// regulatory text, useful to every role that can view incidents.
pub async fn get_taxonomy(_claims: Claims) -> Result<Json<OccurrenceTaxonomyResponse>, AppError> {
    Ok(Json(OccurrenceTaxonomyResponse {
        source: SOURCE,
        categories: &OCCURRENCE_TAXONOMY,
    }))
}
