use super::StaticExternalAdapter;
pub(super) static ADAPTER: StaticExternalAdapter = StaticExternalAdapter::new(
    "agentbench",
    &[
        ("smoke", "Small AgentBench scenario."),
        ("standard", "Standard AgentBench scenario."),
    ],
);
