use super::StaticExternalAdapter;
pub(super) static ADAPTER: StaticExternalAdapter = StaticExternalAdapter::new(
    "aider-polyglot",
    &[
        ("smoke", "Small Aider polyglot task."),
        ("standard", "Standard Aider polyglot task."),
    ],
);
