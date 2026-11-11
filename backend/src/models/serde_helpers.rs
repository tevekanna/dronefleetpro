// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 Lars Wallström

//! Shared serde helpers for request DTOs.
use serde::{Deserialize, Deserializer};

/// Deserialize a field into `Option<Option<T>>` so a handler can distinguish an
/// OMITTED field (outer `None` -> keep existing) from an EXPLICIT JSON null
/// (outer `Some(None)` -> clear to NULL) from a value (`Some(Some(v))` -> set).
/// Pair with `#[serde(default, deserialize_with = "...")]`.
pub fn double_option<'de, T, D>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: Deserializer<'de>,
{
    Ok(Some(Option::deserialize(deserializer)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Deserialize)]
    struct Probe {
        #[serde(default, deserialize_with = "double_option")]
        aircraft_id: Option<Option<i32>>,
    }

    #[test]
    fn omitted_field_is_outer_none() {
        let p: Probe = serde_json::from_str("{}").unwrap();
        assert_eq!(p.aircraft_id, None);
    }

    #[test]
    fn explicit_null_is_some_none() {
        let p: Probe = serde_json::from_str("{\"aircraft_id\": null}").unwrap();
        assert_eq!(p.aircraft_id, Some(None));
    }

    #[test]
    fn value_is_some_some() {
        let p: Probe = serde_json::from_str("{\"aircraft_id\": 7}").unwrap();
        assert_eq!(p.aircraft_id, Some(Some(7)));
    }
}
