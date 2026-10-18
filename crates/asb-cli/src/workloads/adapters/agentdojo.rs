use super::StaticExternalAdapter;
pub(super) static ADAPTER: StaticExternalAdapter = StaticExternalAdapter::new(
    "agentdojo",
    &[
        ("smoke", "Small AgentDojo scenario."),
        ("standard", "Standard AgentDojo scenario."),
    ],
);
