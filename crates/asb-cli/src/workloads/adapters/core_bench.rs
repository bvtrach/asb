use super::StaticExternalAdapter;
pub(super) static ADAPTER: StaticExternalAdapter = StaticExternalAdapter::new(
    "core-bench",
    &[
        ("smoke", "Small CORE-Bench task."),
        ("standard", "Standard CORE-Bench task."),
    ],
);
