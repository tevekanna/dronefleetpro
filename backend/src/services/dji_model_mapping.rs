// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

//! Map `dji-log-parser` DroneType / ProductType Debug-repr strings to
//! canonical `aircraft_models.name` values that exist in the per-org
//! catalog.
//!
//! `drone_type_code` on `FlightPreview` arrives in two shapes:
//! - From `osd.drone_type` (per-frame): bare variant name (e.g. `"MavicPro"`),
//!   because `osd.drone_type` is a `DroneType` value whose Debug repr lacks
//!   any wrapper.
//! - From `details.product_type` (top-level fallback for v9 logs): wrapped
//!   in `"Some(...)"`, because the existing fallback at
//!   `dji_parser.rs` writes `format!("Some({})", inner)` to keep the
//!   `drone_type_mappings` key shape stable across log versions.
//!
//! Both shapes are accepted. Unknown variants return `None`; ambiguous
//! variants (e.g. `Inspire1`, where the camera variant — X3 vs X5 — is not
//! discoverable from the type code) also return `None`. Callers that get
//! `None` should insert the aircraft with `model_id = NULL` so an admin can
//! resolve manually rather than auto-creating a junk catalog row.
//!
//! When extending: add a new arm ONLY after confirming the target name
//! exists in `aircraft_models` (per-org catalog). Mapping to a name that
//! doesn't yet exist would re-introduce the junk-row pathology this module
//! is built to prevent — the consuming code uses `resolve_model_only`,
//! which returns `None` on miss without inserting.

/// Returns the canonical catalog name for a DJI drone-type Debug repr, or
/// `None` if no unambiguous mapping is known.
pub fn canonical_model_name(drone_type_debug: &str) -> Option<&'static str> {
    // Accept both bare ("MavicPro") and Some-wrapped ("Some(MavicPro)") forms.
    let inner = drone_type_debug
        .strip_prefix("Some(")
        .and_then(|s| s.strip_suffix(')'))
        .unwrap_or(drone_type_debug);

    match inner {
        // DroneType (osd.drone_type) + ProductType (details.product_type)
        // share these names. Targets verified against DEV catalog.
        "MavicPro" => Some("Mavic Pro"),
        "MavicAir" => Some("Mavic Air"),
        "MavicAir2" => Some("Mavic Air 2"),
        "Matrice600" => Some("Matrice 600"),

        // ProductType-only / DroneType-only variants. "DJI Mavic 3" is the
        // only Mavic-3-shaped row in the catalog and carries the "DJI "
        // prefix that the surrounding rows omit; preserved verbatim so
        // resolve_model_only's case-insensitive lookup hits.
        "Mavic3" => Some("DJI Mavic 3"),
        "Mavic3Pro" => Some("DJI Mavic 3"),

        // Deliberately unmapped:
        // - Inspire1: camera variant (X3 vs X5) not in the type code; catalog
        //   has both "Inspire 1 X3" and "Inspire 1 X5".
        // - Mavic2: catalog has both "Mavic 2 Pro" and "Mavic 2 Zoom".
        // - MavicAir2S: catalog has duplicate rows ("DJI Air 2s" + "Air 2s"),
        //   needs cleanup before mapping is safe.
        // - All other variants: no matching catalog row today. Admin adds
        //   the catalog row + a new arm here in the same PR.
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_bare_variant() {
        assert_eq!(canonical_model_name("MavicPro"), Some("Mavic Pro"));
        assert_eq!(canonical_model_name("MavicAir"), Some("Mavic Air"));
        assert_eq!(canonical_model_name("MavicAir2"), Some("Mavic Air 2"));
        assert_eq!(canonical_model_name("Matrice600"), Some("Matrice 600"));
    }

    #[test]
    fn maps_some_wrapped_variant() {
        assert_eq!(canonical_model_name("Some(MavicPro)"), Some("Mavic Pro"));
        assert_eq!(canonical_model_name("Some(Mavic3)"), Some("DJI Mavic 3"));
        assert_eq!(canonical_model_name("Some(Mavic3Pro)"), Some("DJI Mavic 3"));
    }

    #[test]
    fn returns_none_for_ambiguous_variants() {
        // Camera/sub-variant cannot be inferred from the type code.
        assert_eq!(canonical_model_name("Inspire1"), None);
        assert_eq!(canonical_model_name("Some(Inspire1)"), None);
        assert_eq!(canonical_model_name("Mavic2"), None);
        assert_eq!(canonical_model_name("Some(Mavic2)"), None);
    }

    #[test]
    fn returns_none_for_unknown_variants() {
        assert_eq!(canonical_model_name("Phantom4Pro"), None);
        assert_eq!(canonical_model_name("Unknown(42)"), None);
        assert_eq!(canonical_model_name("None"), None);
        assert_eq!(canonical_model_name(""), None);
    }
}
