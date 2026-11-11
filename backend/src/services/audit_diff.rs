// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

use serde_json::Value;

use crate::models::audit_log::ChangedFieldDetail;

/// Extract the top-level keys that differ between diff.before and diff.after.
///
/// Shallow comparison: nested objects are compared as whole JSON values.
/// Returns empty when the diff does not have both `before` and `after` objects —
/// covers INSERT, DELETE, and ad-hoc diffs (e.g. `{"action":"complete",...}`).
pub fn summarise_changed_fields(diff: &Value) -> Vec<String> {
    let Some(after_obj) = diff.get("after").and_then(Value::as_object) else {
        return Vec::new();
    };
    let before_obj = diff.get("before").and_then(Value::as_object);
    after_obj
        .iter()
        .filter_map(|(k, v_after)| {
            let v_before = before_obj.and_then(|m| m.get(k));
            match v_before {
                Some(b) if b == v_after => None,
                _ => Some(k.clone()),
            }
        })
        .collect()
}

/// Structured diff: field name + before/after values.
/// Mirrors summarise_changed_fields' comparison rules — keyed by `after` object.
pub fn compute_changed_fields(diff: &Value) -> Vec<ChangedFieldDetail> {
    let Some(after_obj) = diff.get("after").and_then(Value::as_object) else {
        return Vec::new();
    };
    let before_obj = diff.get("before").and_then(Value::as_object);
    after_obj
        .iter()
        .filter_map(|(k, v_after)| {
            let v_before = before_obj.and_then(|m| m.get(k));
            match v_before {
                Some(b) if b == v_after => None,
                Some(b) => Some(ChangedFieldDetail {
                    field: k.clone(),
                    before: b.clone(),
                    after: v_after.clone(),
                }),
                None => Some(ChangedFieldDetail {
                    field: k.clone(),
                    before: Value::Null,
                    after: v_after.clone(),
                }),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn summarise_empty_when_no_before_after() {
        assert!(summarise_changed_fields(&json!({})).is_empty());
        assert!(summarise_changed_fields(&json!({"action": "complete"})).is_empty());
    }

    #[test]
    fn summarise_reports_changed_keys_only() {
        let diff = json!({
            "before": { "a": 1, "b": "x", "c": true },
            "after":  { "a": 2, "b": "x", "c": false },
        });
        let mut fields = summarise_changed_fields(&diff);
        fields.sort();
        assert_eq!(fields, vec!["a", "c"]);
    }

    #[test]
    fn compute_fills_null_when_before_missing() {
        let diff = json!({ "before": {}, "after": { "x": 42 } });
        let fields = compute_changed_fields(&diff);
        assert_eq!(fields.len(), 1);
        assert_eq!(fields[0].field, "x");
        assert_eq!(fields[0].before, Value::Null);
        assert_eq!(fields[0].after, json!(42));
    }

    #[test]
    fn nested_values_compared_as_whole() {
        let diff = json!({
            "before": { "k": { "nested": 1 } },
            "after":  { "k": { "nested": 2 } },
        });
        let fields = summarise_changed_fields(&diff);
        assert_eq!(fields, vec!["k"]);
    }
}
