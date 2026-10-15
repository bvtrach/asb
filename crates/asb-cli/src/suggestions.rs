//! Bounded typo suggestions for finite user-facing names.

/// Return the closest candidate when it is close enough to the input.
pub(crate) fn closest<'a>(value: &str, candidates: &'a [&str]) -> Option<&'a str> {
    let value = value.to_ascii_lowercase();
    let mut best = None;
    let mut best_distance = usize::MAX;
    for candidate in candidates {
        let distance = levenshtein(&value, &candidate.to_ascii_lowercase());
        if distance < best_distance {
            best_distance = distance;
            best = Some(*candidate);
        }
    }
    let length = value.chars().count();
    let threshold = 2.max(length.max(best.map_or(0, |item| item.chars().count())) / 3);
    (best_distance <= threshold)
        .then_some(best?)
        .filter(|item| !item.eq_ignore_ascii_case(&value))
}

/// Format a fail-closed unknown-name message with an optional question.
pub(crate) fn unknown(kind: &str, value: &str, candidates: &[&str]) -> String {
    match closest(value, candidates) {
        Some(candidate) => format!("unknown {kind} `{value}`; did you mean `{candidate}`?"),
        None => format!("unknown {kind} `{value}`"),
    }
}

fn levenshtein(left: &str, right: &str) -> usize {
    let mut previous = (0..=right.chars().count()).collect::<Vec<_>>();
    for (row, left_char) in left.chars().enumerate() {
        let mut current = vec![row + 1];
        for (column, right_char) in right.chars().enumerate() {
            current.push(if left_char == right_char {
                previous[column]
            } else {
                1 + previous[column]
                    .min(previous[column + 1])
                    .min(current[column])
            });
        }
        previous = current;
    }
    previous[right.chars().count()]
}

#[cfg(test)]
mod tests {
    use super::{closest, unknown};

    #[test]
    fn suggests_only_close_candidates() {
        let choices = ["workload", "provider", "tool"];
        assert_eq!(closest("worklod", &choices), Some("workload"));
        assert_eq!(closest("unrelated", &choices), None);
        assert!(unknown("command", "worklod", &choices).contains("did you mean `workload`?"));
    }
}
