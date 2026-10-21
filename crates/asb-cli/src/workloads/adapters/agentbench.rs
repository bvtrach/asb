use super::StaticExternalAdapter;
pub(super) static ADAPTER: StaticExternalAdapter = StaticExternalAdapter::new(
    "agentbench",
    "https://raw.githubusercontent.com/THUDM/AgentBench/d1e4a10db08c87075c78972e48ecc182be03e2d5/README.md",
    &[
        ("smoke", "Small AgentBench scenario."),
        ("standard", "Standard AgentBench scenario."),
    ],
);
