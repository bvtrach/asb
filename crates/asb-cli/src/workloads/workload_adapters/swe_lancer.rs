use super::StaticExternalAdapter;
pub(super) static ADAPTER: StaticExternalAdapter = StaticExternalAdapter::new(
    "swe-lancer",
    &[
        ("smoke", "Small SWE-Lancer task."),
        ("standard", "Standard SWE-Lancer task."),
    ],
);
