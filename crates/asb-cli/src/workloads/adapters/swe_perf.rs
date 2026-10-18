use super::StaticExternalAdapter;
pub(super) static ADAPTER: StaticExternalAdapter = StaticExternalAdapter::new(
    "swe-perf",
    &[
        ("smoke", "Small SWE-Perf task."),
        ("standard", "Standard SWE-Perf task."),
    ],
);
