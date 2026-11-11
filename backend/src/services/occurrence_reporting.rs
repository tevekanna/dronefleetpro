// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

//! Occurrence reporting state machine for incidents.
//!
//! Computes whether the 72-hour mandatory reporting clock of Article 4(7) of
//! Regulation (EU) No 376/2014 applies to an incident, and where it stands.
//!
//! CRITICAL RULE: "due" and "overdue" must NEVER be produced when
//! applicability has not been established. Annex VI Remark 2 and recitals 6
//! and 7 of Implementing Regulation (EU) 2026/1821 limit mandatory reporting
//! to UAS for which a certificate or declaration for design is required under
//! Article 56(1) and (5) of Regulation (EU) 2018/1139. Presenting a legal
//! deadline to an operator who has no such obligation would manufacture a
//! duty that does not exist — so while the applicability of the obligation is
//! undetermined (model not assessed, incident has no aircraft, aircraft has
//! no model) or established as absent ('not_required'), no deadline is ever
//! emitted. `deadline_at` is only computed once `applicable == Some(true)`
//! AND an occurrence code has been assigned.

use chrono::{DateTime, Duration, Utc};
use serde::Serialize;

/// The reporting-obligation status of one incident, embedded in the incident
/// list rows and the incident detail response.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ReportingStatus {
    /// One of: "not_applicable" | "undetermined" | "unclassified" |
    /// "awaiting_awareness" | "reported" | "due" | "overdue".
    pub state: String,
    /// Some(true) when the aircraft model requires a design certificate or
    /// declaration; Some(false) when it does not; None when not determined.
    pub applicable: Option<bool>,
    /// became_aware_at + 72 hours. Emitted ONLY when `applicable` is
    /// Some(true) and an occurrence code is set; None in every other state.
    pub deadline_at: Option<DateTime<Utc>>,
    /// Whole hours until the deadline (negative when overdue). Only present
    /// in the "due" and "overdue" states.
    pub hours_remaining: Option<i64>,
    /// True only in the "reported" state, when the record cannot evidence
    /// WHICH Annex VI item was reported (`occurrence_code` missing) or that
    /// the report was TIMELY (`became_aware_at` missing). An evidence
    /// problem, not a status problem: the obligation itself is discharged,
    /// so the state stays "reported".
    pub record_incomplete: bool,
}

/// Article 4(7) of Regulation (EU) No 376/2014: report within 72 hours of
/// becoming aware of the occurrence.
const REPORTING_WINDOW_HOURS: i64 = 72;

/// Compute the reporting status of an incident.
///
/// `design_approval_required` is the value from the incident's aircraft's
/// model: pass `None` when the incident has no aircraft, the aircraft has no
/// model, or the model's `design_approval_required` column is NULL — all
/// three collapse to the "undetermined" state, because NULL means NOT
/// DETERMINED and must never be treated as 'not_required'.
///
/// `now` is injected for testability; handlers pass `Utc::now()`.
pub fn compute_status(
    design_approval_required: Option<&str>,
    occurrence_code: Option<&str>,
    became_aware_at: Option<DateTime<Utc>>,
    reported_at: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> ReportingStatus {
    let applicable = match design_approval_required {
        Some("required") => Some(true),
        Some("not_required") => Some(false),
        // Any other stored value is constrained away by the CHECK in
        // migration 054; treat defensively as undetermined.
        _ => None,
    };

    // No deadline may exist before applicability is established as present
    // and the occurrence is classified — see module doc comment.
    match applicable {
        Some(false) => {
            return ReportingStatus {
                state: "not_applicable".into(),
                applicable,
                deadline_at: None,
                hours_remaining: None,
                record_incomplete: false,
            };
        }
        None => {
            return ReportingStatus {
                state: "undetermined".into(),
                applicable,
                deadline_at: None,
                hours_remaining: None,
                record_incomplete: false,
            };
        }
        Some(true) => {}
    }

    // A recorded report wins over every clock-derived state: an occurrence
    // that WAS reported must never surface as still awaiting action. Missing
    // classification or awareness evidence is flagged via record_incomplete
    // instead. The deadline guarantee is unchanged — deadline_at is emitted
    // only when an occurrence code is set (and awareness makes it computable).
    if reported_at.is_some() {
        let record_incomplete = occurrence_code.is_none() || became_aware_at.is_none();
        let deadline_at = match (occurrence_code, became_aware_at) {
            (Some(_), Some(aware)) => Some(aware + Duration::hours(REPORTING_WINDOW_HOURS)),
            _ => None,
        };
        return ReportingStatus {
            state: "reported".into(),
            applicable,
            deadline_at,
            hours_remaining: None,
            record_incomplete,
        };
    }

    if occurrence_code.is_none() {
        return ReportingStatus {
            state: "unclassified".into(),
            applicable,
            deadline_at: None,
            hours_remaining: None,
            record_incomplete: false,
        };
    }

    let Some(aware) = became_aware_at else {
        return ReportingStatus {
            state: "awaiting_awareness".into(),
            applicable,
            deadline_at: None,
            hours_remaining: None,
            record_incomplete: false,
        };
    };

    // Applicable + classified + awareness recorded: the deadline exists.
    let deadline = aware + Duration::hours(REPORTING_WINDOW_HOURS);

    let state = if now < deadline { "due" } else { "overdue" };
    ReportingStatus {
        state: state.into(),
        applicable,
        deadline_at: Some(deadline),
        hours_remaining: Some((deadline - now).num_hours()),
        record_incomplete: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn t0() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 7, 1, 12, 0, 0).unwrap()
    }

    #[test]
    fn not_required_yields_not_applicable_with_no_deadline() {
        let s = compute_status(
            Some("not_required"),
            Some("VI.1.4.1"),
            Some(t0()),
            None,
            t0() + Duration::hours(200),
        );
        assert_eq!(s.state, "not_applicable");
        assert_eq!(s.applicable, Some(false));
        assert_eq!(s.deadline_at, None);
        assert_eq!(s.hours_remaining, None);
        assert!(!s.record_incomplete);
    }

    #[test]
    fn null_design_approval_yields_undetermined_with_no_deadline() {
        // Even fully classified, aware and long past 72 hours: while
        // applicability is not established, no deadline may be shown.
        let s = compute_status(
            None,
            Some("VI.1.4.1"),
            Some(t0()),
            None,
            t0() + Duration::hours(200),
        );
        assert_eq!(s.state, "undetermined");
        assert_eq!(s.applicable, None);
        assert_eq!(s.deadline_at, None);
        assert_eq!(s.hours_remaining, None);
        assert!(!s.record_incomplete);
    }

    #[test]
    fn reported_but_not_applicable_never_sets_record_incomplete() {
        // record_incomplete is defined only for applicable == Some(true):
        // with no mandatory-reporting obligation there is no evidence duty
        // for the record to fall short of.
        let s = compute_status(
            Some("not_required"),
            None,
            None,
            Some(t0()),
            t0() + Duration::hours(200),
        );
        assert_eq!(s.state, "not_applicable");
        assert!(!s.record_incomplete);
    }

    #[test]
    fn applicable_but_unclassified_yields_no_deadline() {
        let s = compute_status(Some("required"), None, Some(t0()), None, t0());
        assert_eq!(s.state, "unclassified");
        assert_eq!(s.applicable, Some(true));
        assert_eq!(s.deadline_at, None, "unclassified must never carry a deadline");
        assert_eq!(s.hours_remaining, None);
        assert!(!s.record_incomplete);
    }

    #[test]
    fn classified_without_awareness_yields_awaiting_awareness() {
        let s = compute_status(Some("required"), Some("VI.2.1.4"), None, None, t0());
        assert_eq!(s.state, "awaiting_awareness");
        assert_eq!(s.deadline_at, None);
        assert_eq!(s.hours_remaining, None);
        assert!(!s.record_incomplete);
    }

    #[test]
    fn reported_wins_over_the_clock() {
        let s = compute_status(
            Some("required"),
            Some("VI.4.1"),
            Some(t0()),
            Some(t0() + Duration::hours(10)),
            t0() + Duration::hours(500),
        );
        assert_eq!(s.state, "reported");
        assert_eq!(s.deadline_at, Some(t0() + Duration::hours(72)));
        assert_eq!(s.hours_remaining, None);
        assert!(
            !s.record_incomplete,
            "fully evidenced report (code + awareness) is not incomplete"
        );
    }

    #[test]
    fn reported_without_awareness_is_reported_and_incomplete_with_no_deadline() {
        // Reported to the authority, but became_aware_at was never recorded:
        // the obligation is discharged (state "reported", NOT
        // "awaiting_awareness"), while the missing timeliness evidence is
        // surfaced separately via record_incomplete. No deadline can exist
        // without an awareness time.
        let s = compute_status(
            Some("required"),
            Some("VI.4.1"),
            None,
            Some(t0()),
            t0() + Duration::hours(500),
        );
        assert_eq!(s.state, "reported");
        assert!(s.record_incomplete);
        assert_eq!(s.deadline_at, None);
        assert_eq!(s.hours_remaining, None);
    }

    #[test]
    fn reported_without_code_is_reported_and_incomplete_with_no_deadline() {
        // Reported but never classified: state stays "reported" (not
        // "unclassified"), record_incomplete flags the missing Annex VI
        // item, and the deadline guarantee holds — no occurrence code, no
        // deadline_at, even though awareness is recorded.
        let s = compute_status(
            Some("required"),
            None,
            Some(t0()),
            Some(t0() + Duration::hours(10)),
            t0() + Duration::hours(500),
        );
        assert_eq!(s.state, "reported");
        assert!(s.record_incomplete);
        assert_eq!(s.deadline_at, None, "no deadline may be emitted without an occurrence code");
        assert_eq!(s.hours_remaining, None);
    }

    #[test]
    fn within_window_is_due() {
        let s = compute_status(
            Some("required"),
            Some("VI.4.1"),
            Some(t0()),
            None,
            t0() + Duration::hours(1),
        );
        assert_eq!(s.state, "due");
        assert_eq!(s.deadline_at, Some(t0() + Duration::hours(72)));
        assert_eq!(s.hours_remaining, Some(71));
        assert!(!s.record_incomplete);
    }

    #[test]
    fn boundary_exactly_72_hours_is_overdue() {
        // now == deadline: the window is "within 72 hours", so at exactly
        // +72h the report is no longer timely (now >= deadline).
        let s = compute_status(
            Some("required"),
            Some("VI.4.1"),
            Some(t0()),
            None,
            t0() + Duration::hours(72),
        );
        assert_eq!(s.state, "overdue");
        assert_eq!(s.hours_remaining, Some(0));
    }

    #[test]
    fn boundary_one_second_past_72_hours_is_overdue() {
        let s = compute_status(
            Some("required"),
            Some("VI.4.1"),
            Some(t0()),
            None,
            t0() + Duration::hours(72) + Duration::seconds(1),
        );
        assert_eq!(s.state, "overdue");
        assert_eq!(s.deadline_at, Some(t0() + Duration::hours(72)));
    }

    #[test]
    fn one_second_before_72_hours_is_still_due() {
        let s = compute_status(
            Some("required"),
            Some("VI.4.1"),
            Some(t0()),
            None,
            t0() + Duration::hours(72) - Duration::seconds(1),
        );
        assert_eq!(s.state, "due");
        assert_eq!(s.hours_remaining, Some(0));
    }

    #[test]
    fn unexpected_stored_value_is_treated_as_undetermined() {
        let s = compute_status(Some("maybe"), Some("VI.4.1"), Some(t0()), None, t0());
        assert_eq!(s.state, "undetermined");
        assert_eq!(s.applicable, None);
        assert_eq!(s.deadline_at, None);
    }
}
