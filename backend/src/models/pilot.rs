// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

use serde::{Deserialize, Serialize};
use sqlx::FromRow;
use chrono::{DateTime, Utc, NaiveDate};

#[derive(Debug, Serialize, Deserialize, FromRow, Clone)]
pub struct Pilot {
    pub id: i32,
    pub org_id: i32,
    pub name: String,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub license_number: Option<String>,
    pub license_type: Option<String>,
    pub license_expires: Option<NaiveDate>,
    pub is_active: bool,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, FromRow, Clone)]
pub struct PilotQualification {
    pub id: i32,
    pub org_id: i32,
    pub pilot_id: i32,
    pub type_name: String,
    pub issued_date: Option<NaiveDate>,
    pub expiry_date: Option<NaiveDate>,
    pub certificate_ref: Option<String>,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PilotWithQualifications {
    pub id: i32,
    pub org_id: i32,
    pub name: String,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub license_number: Option<String>,
    pub license_type: Option<String>,
    pub license_expires: Option<NaiveDate>,
    pub is_active: bool,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
    pub qualifications: Vec<PilotQualification>,
}

impl PilotWithQualifications {
    pub fn from_pilot(pilot: Pilot, qualifications: Vec<PilotQualification>) -> Self {
        Self {
            id: pilot.id,
            org_id: pilot.org_id,
            name: pilot.name,
            email: pilot.email,
            phone: pilot.phone,
            license_number: pilot.license_number,
            license_type: pilot.license_type,
            license_expires: pilot.license_expires,
            is_active: pilot.is_active,
            notes: pilot.notes,
            created_at: pilot.created_at,
            qualifications,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreatePilotRequest {
    pub name: String,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub license_number: Option<String>,
    pub license_type: Option<String>,
    pub license_expires: Option<String>,
    pub notes: Option<String>,
    pub qualifications: Option<Vec<String>>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct UpdatePilotRequest {
    pub name: Option<String>,
    pub email: Option<String>,
    pub phone: Option<String>,
    pub license_number: Option<String>,
    pub license_type: Option<String>,
    pub license_expires: Option<String>,
    pub is_active: Option<bool>,
    pub notes: Option<String>,
    pub qualifications: Option<Vec<String>>,
}
