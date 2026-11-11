// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct Organisation {
    pub id: i32,
    pub name: String,
    pub slug: String,
    pub easa_op_ref: Option<String>,
    pub country: Option<String>,
    pub plan: String,
    pub created_at: DateTime<Utc>,
}
