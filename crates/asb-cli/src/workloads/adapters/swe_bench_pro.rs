use super::StaticExternalAdapter;
pub(super) static ADAPTER: StaticExternalAdapter = StaticExternalAdapter::new(
    "swe-bench-pro",
    "https://raw.githubusercontent.com/scaleapi/SWE-bench_Pro-os/ca10a60a5fcae51e6948ffe1485d4153d421e6c5/README.md",
    &[
        ("bug-fix", "One SWE-Bench Pro bug-fix trial."),
        ("regression", "SWE-Bench Pro regression trial."),
        ("tests", "SWE-Bench Pro test trial."),
    ],
);
