use super::StaticExternalAdapter;
pub(super) static ADAPTER: StaticExternalAdapter = StaticExternalAdapter::new(
    "aider-polyglot",
    "https://raw.githubusercontent.com/Aider-AI/aider/5dc9490bb35f9729ef2c95d00a19ccd30c26339c/README.md",
    &[
        ("smoke", "Small Aider polyglot task."),
        ("standard", "Standard Aider polyglot task."),
    ],
);
