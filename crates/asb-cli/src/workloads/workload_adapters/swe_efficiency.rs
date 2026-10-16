use super::StaticExternalAdapter;
pub(super) static ADAPTER: StaticExternalAdapter = StaticExternalAdapter::new(
    "swe-efficiency",
    &[
        ("smoke", "Small SWE-Efficiency task."),
        ("standard", "Standard SWE-Efficiency task."),
    ],
);
