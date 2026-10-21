use super::StaticExternalAdapter;
pub(super) static ADAPTER: StaticExternalAdapter = StaticExternalAdapter::new(
    "agentdojo",
    "https://raw.githubusercontent.com/ethz-spylab/agentdojo/089ed468cf3ed0322acc66b0211f26d9d90dbf60/README.md",
    &[
        ("smoke", "Small AgentDojo scenario."),
        ("standard", "Standard AgentDojo scenario."),
    ],
);
