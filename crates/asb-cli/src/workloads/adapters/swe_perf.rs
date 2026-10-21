use super::StaticExternalAdapter;
pub(super) static ADAPTER: StaticExternalAdapter = StaticExternalAdapter::new(
    "swe-perf",
    "https://raw.githubusercontent.com/SWE-Perf/SWE-Perf/9e8fed285ed640b51baaf66b13773706dc95aa45/README.md",
    &[
        ("smoke", "Small SWE-Perf task."),
        ("standard", "Standard SWE-Perf task."),
    ],
);
