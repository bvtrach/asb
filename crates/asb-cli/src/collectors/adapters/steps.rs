use super::super::CollectorAdapter;

pub(super) struct StepsCollector;
pub(super) static ADAPTER: StepsCollector = StepsCollector;

impl CollectorAdapter for StepsCollector {
    fn id(&self) -> &'static str {
        "steps"
    }
    fn steps(&self, agent: &str, stdout: &str, stderr: &str) -> Option<u64> {
        crate::agents::find(agent)?.step_count(stdout, stderr)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_agent_inherits_a_guess_from_text_or_arbitrary_keys() {
        for agent in crate::agents::names() {
            for output in [
                "done",
                "command failed at step 2",
                r#"{"command":"hello","steps":27}"#,
                r#"{"type":"tool_call.delta"}"#,
            ] {
                assert_eq!(ADAPTER.steps(agent, output, ""), None, "{agent}");
            }
        }
    }

    #[test]
    fn qwen_uses_final_turn_counter_without_counting_stream_events() {
        assert_eq!(
            ADAPTER.steps(
                "qwen-code",
                r#"[{"type":"result","num_turns":6},{"type":"result"}]"#,
                ""
            ),
            None
        );
        assert_eq!(ADAPTER.steps("qwen-code", r#"[{"type":"assistant"},{"type":"result","num_turns":6},{"type":"result","num_turns":6}]"#, ""), Some(6));
        assert_eq!(
            ADAPTER.steps("qwen-code", "{\"type\":\"result\",\"num_turns\":0}", ""),
            Some(0)
        );
        assert_eq!(
            ADAPTER.steps("qwen-code", r#"{"type":"result","num_turns":-1}"#, ""),
            None
        );
    }

    #[test]
    fn cline_uses_final_iterations_not_stream_deltas_starts_and_ends() {
        let output = r#"{"type":"agent_event","event":{"type":"iteration_start"}}
{"type":"agent_event","event":{"type":"iteration_end"}}
{"type":"run_result","iterations":4}"#;
        assert_eq!(ADAPTER.steps("cline", output, ""), Some(4));
        assert_eq!(
            ADAPTER.steps(
                "cline",
                r#"{"type":"agent_event","event":{"type":"iteration_end"}}"#,
                ""
            ),
            None
        );
    }
}
