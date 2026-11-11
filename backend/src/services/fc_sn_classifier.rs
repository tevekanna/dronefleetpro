// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

//! Classification helpers for the FC-SN-prone model flow in commit.rs.
//!
//! Some DJI airframes (Matrice 600, Inspire 1 X3/X5) record the flight-
//! controller SN in their logs instead of the chassis body SN that the
//! operator scanned from the QR sticker. Auto-registering on no-match
//! produces duplicate ghost aircraft that need manual merge (seen in a
//! production database: two ghosts of one airframe, merged by hand on
//! 2026-05-16).
//!
//! This module lets the commit handler refuse auto-register for these
//! models by default, returning a 409 with candidate aircraft, while
//! still allowing the operator to override via either an explicit link
//! to an existing aircraft or a written confirmation that the SN belongs
//! to a brand-new physical unit.

use crate::errors::AppError;

/// Canonical `aircraft_models.name` values for airframes where the DJI log
/// SN is known to be the flight-controller SN rather than the chassis body
/// SN.
///
/// Matching is exact (case-sensitive) — add models here only with empirical
/// evidence, because every entry forces operators through a confirmation
/// flow when an unmatched SN of that model shows up at commit time.
pub const FC_SN_PRONE_MODELS: &[&str] = &[
    "Inspire 1 X3",
    "Inspire 1 X5",
    "Matrice 600",
];

/// True when `name` exactly matches one of `FC_SN_PRONE_MODELS`. Used by
/// `commit_imported_flight` to decide whether the pre-flight refusal path
/// applies and whether the FORCE-register audit row should fire.
pub fn is_fc_sn_prone_model(name: &str) -> bool {
    FC_SN_PRONE_MODELS.iter().any(|m| *m == name)
}

/// The four outcomes of the commit-time FC-SN classification policy.
#[derive(Debug, PartialEq, Eq)]
pub enum FcSnCommitDecision {
    /// Non-prone model, or any model when the operator hasn't asked for a
    /// special path. The handler proceeds through the existing commit
    /// pipeline — `find_match` runs later in the aircraft branch and the
    /// usual auto-link / auto-register fallbacks apply.
    NormalFlow,
    /// Prone model + no override flags. The handler still confirms via a
    /// read-only `find_match` that the SN really has no match, and if so
    /// returns 409 with candidate aircraft so the operator can pick Link
    /// or Force on the next attempt.
    RefusePreflight,
    /// Operator chose an existing aircraft to link the unmatched SN to.
    /// Overrides every other classification — operator authority is the
    /// ground truth here.
    LinkExisting(i32),
    /// Operator confirmed it's a brand-new physical unit. The carried
    /// String is the verbatim trimmed reason text, already validated to
    /// be ≥10 non-whitespace characters.
    ForceRegister(String),
}

/// Decision policy for the commit-time FC-SN flow. Pure: no DB access, no
/// awaiting. The handler runs this BEFORE the flight INSERT so a refusal
/// is a clean no-op on the caller side.
///
/// Precedence (top wins):
///   1. `link_existing_aircraft_id = Some(id)` → `LinkExisting(id)` —
///      operator authority overrides everything.
///   2. `force_register_new = true` → `ForceRegister(reason)` if reason
///      trims to ≥10 chars, otherwise `Err(BadRequest)`.
///   3. `is_fc_sn_prone_model(model_name)` and no flags → `RefusePreflight`.
///   4. Else → `NormalFlow`.
pub fn classify_commit_fc_sn(
    model_name: &str,
    link_existing_aircraft_id: Option<i32>,
    force_register_new: bool,
    force_register_reason: Option<&str>,
) -> Result<FcSnCommitDecision, AppError> {
    if let Some(id) = link_existing_aircraft_id {
        return Ok(FcSnCommitDecision::LinkExisting(id));
    }
    if force_register_new {
        let trimmed = force_register_reason.map(|s| s.trim()).unwrap_or("");
        if trimmed.len() < 10 {
            return Err(AppError::BadRequest(
                "force_register_reason must be at least 10 characters".into(),
            ));
        }
        return Ok(FcSnCommitDecision::ForceRegister(trimmed.to_string()));
    }
    if is_fc_sn_prone_model(model_name) {
        return Ok(FcSnCommitDecision::RefusePreflight);
    }
    Ok(FcSnCommitDecision::NormalFlow)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- is_fc_sn_prone_model: 3 positive, 3 negative ----

    #[test]
    fn prone_models_match_exact() {
        assert!(is_fc_sn_prone_model("Inspire 1 X3"));
        assert!(is_fc_sn_prone_model("Inspire 1 X5"));
        assert!(is_fc_sn_prone_model("Matrice 600"));
    }

    #[test]
    fn non_prone_models_dont_match() {
        assert!(!is_fc_sn_prone_model("DJI Mavic 3"));
        assert!(!is_fc_sn_prone_model(""));
        // Near-miss: "Inspire 1" without the camera variant suffix must
        // NOT match — prone-ness is variant-specific and we refuse to
        // guess.
        assert!(!is_fc_sn_prone_model("Inspire 1"));
    }

    // ---- classify_commit_fc_sn ----

    #[test]
    fn classify_returns_normal_flow_for_non_prone_with_no_flags() {
        let d = classify_commit_fc_sn("DJI Mavic 3", None, false, None).unwrap();
        assert_eq!(d, FcSnCommitDecision::NormalFlow);
    }

    #[test]
    fn classify_returns_refuse_preflight_for_prone_with_no_flags() {
        let d = classify_commit_fc_sn("Matrice 600", None, false, None).unwrap();
        assert_eq!(d, FcSnCommitDecision::RefusePreflight);
    }

    #[test]
    fn classify_returns_link_existing_when_link_id_set_regardless_of_model() {
        // Link wins on a prone model
        let d = classify_commit_fc_sn("Matrice 600", Some(42), false, None).unwrap();
        assert_eq!(d, FcSnCommitDecision::LinkExisting(42));
        // Link wins on a non-prone model too
        let d2 = classify_commit_fc_sn("DJI Mavic 3", Some(7), false, None).unwrap();
        assert_eq!(d2, FcSnCommitDecision::LinkExisting(7));
        // Link wins even when force-register is also asserted (operator
        // sent both flags — link is the ground truth).
        let d3 = classify_commit_fc_sn(
            "Matrice 600",
            Some(5),
            true,
            Some("operator changed mind, link instead"),
        )
        .unwrap();
        assert_eq!(d3, FcSnCommitDecision::LinkExisting(5));
    }

    #[test]
    fn classify_returns_force_register_with_valid_reason() {
        let d = classify_commit_fc_sn(
            "Matrice 600",
            None,
            true,
            Some("this is genuinely a new Matrice 600 unit"),
        )
        .unwrap();
        assert_eq!(
            d,
            FcSnCommitDecision::ForceRegister(
                "this is genuinely a new Matrice 600 unit".to_string()
            )
        );
    }

    #[test]
    fn classify_rejects_force_register_with_short_reason() {
        let err =
            classify_commit_fc_sn("Matrice 600", None, true, Some("too short")).unwrap_err();
        match err {
            AppError::BadRequest(m) => assert!(m.contains("at least 10 characters"), "got: {m}"),
            other => panic!("expected BadRequest, got {:?}", other),
        }
    }

    #[test]
    fn classify_rejects_force_register_with_missing_reason() {
        let err = classify_commit_fc_sn("Matrice 600", None, true, None).unwrap_err();
        match err {
            AppError::BadRequest(m) => assert!(m.contains("at least 10 characters"), "got: {m}"),
            other => panic!("expected BadRequest, got {:?}", other),
        }
        // Whitespace-only reason is also rejected (trim().len() < 10).
        let err2 =
            classify_commit_fc_sn("Matrice 600", None, true, Some("          ")).unwrap_err();
        match err2 {
            AppError::BadRequest(_) => (),
            other => panic!("expected BadRequest, got {:?}", other),
        }
    }
}
