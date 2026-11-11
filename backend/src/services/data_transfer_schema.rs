// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

//! Shared schema knowledge for data export/import.
//!
//! Single source of truth for which tables belong to which entity group,
//! and the dependency order in which they must be processed. Used by
//! both `data_export` (to emit) and `data_import` (to consume).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntityGroup {
    Equipment,
    Pilots,
    Flights,
    Maintenance,
    Incidents,
    Settings,
    Organisation,
    AuditLog,
}

impl EntityGroup {
    pub fn from_str(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "equipment" => Some(Self::Equipment),
            "pilots" => Some(Self::Pilots),
            "flights" => Some(Self::Flights),
            "maintenance" => Some(Self::Maintenance),
            "incidents" => Some(Self::Incidents),
            "settings" => Some(Self::Settings),
            "organisation" | "organization" => Some(Self::Organisation),
            "audit_log" | "audit-log" | "auditlog" => Some(Self::AuditLog),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Equipment => "equipment",
            Self::Pilots => "pilots",
            Self::Flights => "flights",
            Self::Maintenance => "maintenance",
            Self::Incidents => "incidents",
            Self::Settings => "settings",
            Self::Organisation => "organisation",
            Self::AuditLog => "audit_log",
        }
    }

    pub fn all() -> &'static [EntityGroup] {
        &[
            Self::Equipment,
            Self::Pilots,
            Self::Flights,
            Self::Maintenance,
            Self::Incidents,
            Self::Settings,
            Self::Organisation,
            Self::AuditLog,
        ]
    }
}

/// Canonical table list with dependency order. Index in this slice IS
/// the ordering used during export and import. Tables earlier in the
/// list must be processed before tables later in the list, so FKs in
/// later rows can be remapped from already-imported parents.
///
/// Deliberately EXCLUDED, and why:
/// - `model_documents` / `incident_attachments` — their payload lives on
///   the filesystem under `UPLOAD_DIR`; exporting the row metadata without
///   the files would produce dangling references on import.
/// - `users` / `user_roles` — auth/credential surface; forbidden on import
///   (see `is_forbidden_on_import`).
/// - `auth_attempts` / `data_transfer_jobs` — operational/audit-of-audit
///   tables that are meaningless across instances.
pub const CANONICAL_TABLES: &[(&str, EntityGroup)] = &[
    ("aircraft_models", EntityGroup::Equipment),
    ("aircraft", EntityGroup::Equipment),
    ("aircraft_identifiers", EntityGroup::Equipment),
    ("batteries", EntityGroup::Equipment),
    ("battery_identifiers", EntityGroup::Equipment),
    ("remote_controllers", EntityGroup::Equipment),
    ("controller_identifiers", EntityGroup::Equipment),
    // Weighted inventory (migration 050) with a nullable aircraft_id mount
    // link (migration 051). Must follow `aircraft` in this list so the import
    // FK remap resolves from an already-imported parent; it already does.
    ("accessories", EntityGroup::Equipment),
    ("pilots", EntityGroup::Pilots),
    ("pilot_qualifications", EntityGroup::Pilots),
    ("flights", EntityGroup::Flights),
    // Child of flights; must follow it so flight_id remaps from an
    // already-imported parent.
    ("flight_log_findings", EntityGroup::Flights),
    // Instruction templates and their checklist items must precede
    // maintenance_schedules and maintenance (both carry an
    // instruction_template_id FK) and precede checklist completions
    // (which reference both a maintenance record and a checklist item).
    ("maintenance_schedule_templates", EntityGroup::Maintenance),
    ("maintenance_instruction_templates", EntityGroup::Maintenance),
    ("maintenance_checklist_items", EntityGroup::Maintenance),
    ("maintenance_schedules", EntityGroup::Maintenance),
    ("maintenance", EntityGroup::Maintenance),
    ("maintenance_checklist_completions", EntityGroup::Maintenance),
    ("maintenance_drafts", EntityGroup::Maintenance),
    ("defect_log", EntityGroup::Maintenance),
    ("battery_tests", EntityGroup::Maintenance),
    ("incidents", EntityGroup::Incidents),
    ("incident_notes", EntityGroup::Incidents),
    ("settings", EntityGroup::Settings),
    // Own group (Organisation) so it can be deselected at export time and
    // skipped on import; ordering position is unchanged (grouping is
    // orthogonal to FK dependency order).
    ("organisation_branding", EntityGroup::Organisation),
    ("drone_type_mappings", EntityGroup::Settings),
    ("checklists", EntityGroup::Settings),
    ("audit_log", EntityGroup::AuditLog),
];

/// Tables that this server understands for import/export.
pub fn is_known_table(name: &str) -> bool {
    CANONICAL_TABLES.iter().any(|(t, _)| *t == name)
}

/// Tables that an import quietly skips even if present in the document.
/// `data_transfer_jobs` is the operational tracking table itself; it
/// makes no sense to import it across instances.
pub fn is_silently_ignored_on_import(name: &str) -> bool {
    matches!(name, "data_transfer_jobs")
}

/// Tables that are explicitly forbidden to appear in an import document.
/// `users` and `user_roles` are excluded from export by design (auth /
/// credential / RBAC surface); any document containing them is suspect
/// (tampered or hand-edited), and importing user ids across instances is
/// meaningless.
pub fn is_forbidden_on_import(name: &str) -> bool {
    matches!(name, "users" | "user_roles")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_group_strings() {
        for g in EntityGroup::all() {
            assert_eq!(EntityGroup::from_str(g.as_str()), Some(*g));
        }
    }

    #[test]
    fn organisation_group_accepts_both_spellings() {
        assert_eq!(EntityGroup::from_str("organisation"), Some(EntityGroup::Organisation));
        assert_eq!(EntityGroup::from_str("organization"), Some(EntityGroup::Organisation));
    }

    #[test]
    fn organisation_branding_is_in_its_own_group() {
        let entry = CANONICAL_TABLES
            .iter()
            .find(|(t, _)| *t == "organisation_branding")
            .expect("organisation_branding present");
        assert_eq!(entry.1, EntityGroup::Organisation);
        // Nothing else shares the Organisation group.
        assert_eq!(
            CANONICAL_TABLES
                .iter()
                .filter(|(_, g)| *g == EntityGroup::Organisation)
                .count(),
            1
        );
    }

    #[test]
    fn dependency_order_invariants() {
        let names: Vec<&str> = CANONICAL_TABLES.iter().map(|(t, _)| *t).collect();
        let pos = |n: &str| {
            names
                .iter()
                .position(|x| *x == n)
                .unwrap_or_else(|| panic!("table {n} missing from CANONICAL_TABLES"))
        };
        // Equipment.
        assert!(pos("aircraft_models") < pos("aircraft"));
        assert!(pos("aircraft") < pos("aircraft_identifiers"));
        assert!(pos("aircraft") < pos("batteries"));
        assert!(pos("batteries") < pos("battery_identifiers"));
        assert!(pos("aircraft") < pos("remote_controllers"));
        assert!(pos("remote_controllers") < pos("controller_identifiers"));
        // Pilots / flights.
        assert!(pos("pilots") < pos("pilot_qualifications"));
        assert!(pos("aircraft") < pos("flights"));
        assert!(pos("pilots") < pos("flights"));
        // Maintenance: templates and items precede schedules, maintenance,
        // and completions so instruction_template_id / checklist_item_id
        // FKs resolve from already-imported parents.
        assert!(pos("aircraft_models") < pos("maintenance_schedule_templates"));
        assert!(pos("aircraft_models") < pos("maintenance_instruction_templates"));
        assert!(pos("maintenance_instruction_templates") < pos("maintenance_checklist_items"));
        assert!(pos("maintenance_instruction_templates") < pos("maintenance_schedules"));
        assert!(pos("maintenance_instruction_templates") < pos("maintenance"));
        assert!(pos("maintenance_schedules") < pos("maintenance"));
        assert!(pos("maintenance_checklist_items") < pos("maintenance_checklist_completions"));
        assert!(pos("maintenance") < pos("maintenance_checklist_completions"));
        assert!(pos("maintenance_schedules") < pos("maintenance_drafts"));
        // Incidents.
        assert!(pos("flights") < pos("incidents"));
        assert!(pos("incidents") < pos("incident_notes"));
    }

    #[test]
    fn forbidden_and_ignored_tables() {
        assert!(is_forbidden_on_import("users"));
        assert!(is_forbidden_on_import("user_roles"));
        assert!(!is_forbidden_on_import("aircraft"));
        assert!(is_silently_ignored_on_import("data_transfer_jobs"));
        assert!(!is_silently_ignored_on_import("aircraft"));
    }

    #[test]
    fn known_tables_match_canonical() {
        assert!(is_known_table("aircraft"));
        assert!(is_known_table("audit_log"));
        assert!(is_known_table("aircraft_identifiers"));
        assert!(is_known_table("maintenance_instruction_templates"));
        assert!(is_known_table("maintenance_checklist_completions"));
        assert!(is_known_table("organisation_branding"));
        assert!(is_known_table("incident_notes"));
        assert!(!is_known_table("users"));
        assert!(!is_known_table("user_roles"));
        assert!(!is_known_table("model_documents"));
        assert!(!is_known_table("incident_attachments"));
        assert!(!is_known_table("nonexistent"));
    }
}
