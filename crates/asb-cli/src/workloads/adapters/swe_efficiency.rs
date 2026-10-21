use super::StaticExternalAdapter;
pub(super) static ADAPTER: StaticExternalAdapter = StaticExternalAdapter::new(
    "swe-efficiency",
    "https://raw.githubusercontent.com/swefficiency/swefficiency/12d32a2d6800824a7d84bdb6797b5708e7b7957f/README.md",
    &[
        ("smoke", "Small SWE-Efficiency task."),
        ("standard", "Standard SWE-Efficiency task."),
    ],
);
