//! Lossless measurement paths and collision-free portable graph filenames.

use crate::Error;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

/// Escape separators in object keys, keeping ordinary historical paths readable.
pub(super) fn path_key(key: &str) -> String {
    if key.is_empty() {
        return "\\0".into();
    }
    let mut escaped = String::new();
    for character in key.chars() {
        if matches!(character, '\\' | '.' | '[' | ']') {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    escaped
}

pub(super) fn insert_unique<T>(
    output: &mut BTreeMap<String, T>,
    key: String,
    value: T,
) -> Result<(), Error> {
    match output.entry(key) {
        std::collections::btree_map::Entry::Vacant(entry) => {
            entry.insert(value);
            Ok(())
        }
        std::collections::btree_map::Entry::Occupied(entry) => Err(Error::Input(format!(
            "conflicting measurement identity `{}`; use distinct metric/series names in the source artifacts instead of overwriting observations",
            entry.key()
        ))),
    }
}

pub(super) fn flatten_numbers(
    prefix: &str,
    value: &Value,
    output: &mut BTreeMap<String, f64>,
) -> Result<(), Error> {
    match value {
        Value::Number(number) => {
            if let Some(number) = number.as_f64()
                && !prefix.is_empty()
            {
                insert_unique(output, prefix.into(), number)?;
            }
        }
        Value::Object(object) => {
            for (key, child) in object {
                if matches!(
                    key.as_str(),
                    "schema_version"
                        | "started_unix_ms"
                        | "collected_unix_ms"
                        | "clock_ticks_per_second"
                ) || (prefix.is_empty()
                    && matches!(key.as_str(), "execution_cohort" | "replica_index"))
                {
                    continue;
                }
                let key = path_key(key);
                let next = if prefix.is_empty() {
                    key
                } else {
                    format!("{prefix}.{key}")
                };
                flatten_numbers(&next, child, output)?;
            }
        }
        Value::Array(values) => {
            for (index, child) in values.iter().enumerate() {
                flatten_numbers(&format!("{prefix}[{index}]"), child, output)?;
            }
        }
        _ => {}
    }
    Ok(())
}

/// Reserve every artifact name, including case-insensitive filesystem aliases.
/// Ordinary safe names stay compatible; unsafe/long/colliding names use a full
/// digest plus a bounded readable stem. Even a digest alias cannot overwrite.
#[derive(Default)]
pub(super) struct GraphNames(BTreeSet<String>);

impl GraphNames {
    pub(super) fn allocate(&mut self, kind: &str, name: &str) -> String {
        let safe = crate::benchmark_paths::component(name);
        let legacy = format!("{kind}-{safe}.svg");
        if !safe.is_empty()
            && safe == name
            && legacy.len() <= 200
            && self.0.insert(legacy.to_ascii_lowercase())
        {
            return legacy;
        }
        let stem: String = safe.chars().take(80).collect();
        let digest = format!("{:x}", Sha256::digest(name.as_bytes()));
        let mut sequence = 0;
        loop {
            let suffix = if sequence == 0 {
                String::new()
            } else {
                format!("-{sequence}")
            };
            let candidate = format!("{kind}-{stem}--{digest}{suffix}.svg");
            if self.0.insert(candidate.to_ascii_lowercase()) {
                return candidate;
            }
            sequence += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn numeric_paths_preserve_literal_separators_arrays_empty_keys_and_backslashes() {
        let mut values = BTreeMap::new();
        flatten_numbers(
            "",
            &json!({
                "a.b":1, "a":{"b":2}, "a[0]":3, "array":[4],
                "array[0]":5, "":6, "\\0":7, "a\\.b":8,
                "not_schema_version":9, "schema_version":10,
                "nested":{"schema_version":11,"value":12}, "text":"ignored", "none":null
            }),
            &mut values,
        )
        .unwrap();
        assert_eq!(
            values,
            BTreeMap::from([
                ("a\\.b".into(), 1.0),
                ("a.b".into(), 2.0),
                ("a\\[0\\]".into(), 3.0),
                ("array[0]".into(), 4.0),
                ("array\\[0\\]".into(), 5.0),
                ("\\0".into(), 6.0),
                ("\\\\0".into(), 7.0),
                ("a\\\\\\.b".into(), 8.0),
                ("not_schema_version".into(), 9.0),
                ("nested.value".into(), 12.0)
            ])
        );
    }

    #[test]
    fn overlapping_sources_are_errors_not_silent_replacement() {
        let mut values = BTreeMap::new();
        flatten_numbers("custom", &json!({"value":1}), &mut values).unwrap();
        let error = flatten_numbers("custom", &json!({"value":2}), &mut values).unwrap_err();
        assert!(error.to_string().contains("custom.value"));
        assert_eq!(values["custom.value"], 1.0);
    }

    #[test]
    fn portable_names_are_unique_bounded_and_deterministic() {
        let inputs = [
            "a/b".into(),
            "a?b".into(),
            "a-b".into(),
            "é".into(),
            "ü".into(),
            "A-B".into(),
            "".into(),
            "x".repeat(1000),
            "safe.metric".into(),
        ];
        let allocate = || {
            let mut names = GraphNames::default();
            inputs
                .iter()
                .map(|name| names.allocate("metric", name))
                .collect::<Vec<_>>()
        };
        let names = allocate();
        assert_eq!(names, allocate());
        assert_eq!(
            names
                .iter()
                .map(|name| name.to_ascii_lowercase())
                .collect::<BTreeSet<_>>()
                .len(),
            inputs.len()
        );
        assert!(
            names
                .iter()
                .all(|name| name.len() <= 200 && name.is_ascii() && !name.contains('/'))
        );
        assert_eq!(names[2], "metric-a-b.svg");
        assert_eq!(names[8], "metric-safe.metric.svg");
        let mut registry = GraphNames::default();
        let first = registry.allocate("metric", "a/b");
        assert_ne!(registry.allocate("metric", "a/b"), first);
        assert_ne!(registry.allocate("series", "a/b"), first);
        // A legitimate safe metric can itself look exactly like a hashed name.
        let alias = first
            .strip_prefix("metric-")
            .unwrap()
            .strip_suffix(".svg")
            .unwrap();
        let mut registry = GraphNames::default();
        assert_eq!(registry.allocate("metric", alias), first);
        assert_ne!(registry.allocate("metric", "a/b"), first);
    }
}
