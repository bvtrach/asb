use super::StaticExternalAdapter;
pub(super) static ADAPTER: StaticExternalAdapter = StaticExternalAdapter::new(
    "swe-lancer",
    "https://raw.githubusercontent.com/openai/frontier-evals/51052cede8cc608f95bb00346635e03759013e5a/README.md",
    &[
        ("smoke", "Small SWE-Lancer task."),
        ("standard", "Standard SWE-Lancer task."),
    ],
);
