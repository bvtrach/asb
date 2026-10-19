use super::super::CollectorAdapter;
use super::token_usage::embedded_json_values;

pub(super) struct StepsCollector;
pub(super) static ADAPTER: StepsCollector = StepsCollector;

impl CollectorAdapter for StepsCollector {
    fn id(&self) -> &'static str {
        "steps"
    }
    fn steps(&self, agent: &str, stdout: &str, stderr: &str) -> Option<u64> {
        Some(from_sources(agent, stdout, stderr))
    }
}

/// Count bounded agent actions/turns from structured or human-readable output.
/// The count is deliberately provider-neutral: agents may expose `step`,
/// `turn`, tool, command, or iteration events under different JSON shapes.
fn from_sources(agent: &str, stdout: &str, stderr: &str) -> u64 {
    let mut count = 0_u64;
    let mut saw_structured = false;
    for line in stdout.lines().chain(stderr.lines()) {
        let lower = line.to_ascii_lowercase();
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(line.trim()) {
            saw_structured = true;
            count = count.saturating_add(structured_step_count(&value));
            continue;
        }
        for value in embedded_json_values(line) {
            saw_structured = true;
            count = count.saturating_add(structured_step_count(&value));
        }
        if !saw_structured
            && ["step", "turn", "iteration", "tool call", "command"]
                .iter()
                .any(|marker| lower.contains(marker))
        {
            count = count.saturating_add(1);
        }
    }
    // A successful agent that emitted no event still consumed one execution
    // step; this keeps per-agent comparisons meaningful without guessing token
    // usage or inventing additional actions.
    if count == 0 && !stdout.trim().is_empty() && !agent.is_empty() {
        1
    } else {
        count
    }
}

fn structured_step_count(value: &serde_json::Value) -> u64 {
    match value {
        serde_json::Value::Array(values) => values.iter().map(structured_step_count).sum(),
        serde_json::Value::Object(object) => {
            let event_count = object
                .get("type")
                .and_then(serde_json::Value::as_str)
                .filter(|event| {
                    let event = event.to_ascii_lowercase();
                    ["step", "turn", "iteration", "tool", "command", "action"]
                        .iter()
                        .any(|marker| event.contains(marker))
                })
                .map_or(0, |_| 1);
            if event_count > 0 {
                return event_count;
            }
            let mut count = 0;
            for (key, child) in object {
                let lower = key.to_ascii_lowercase();
                if [
                    "step",
                    "turn",
                    "iteration",
                    "tool_call",
                    "toolcall",
                    "command",
                    "action",
                ]
                .iter()
                .any(|marker| lower.contains(marker))
                    && !child.is_object()
                {
                    count += 1;
                }
                count += structured_step_count(child);
            }
            count
        }
        _ => 0,
    }
}
