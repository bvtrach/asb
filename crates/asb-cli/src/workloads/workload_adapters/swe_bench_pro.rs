use super::StaticExternalAdapter;
pub(super) static ADAPTER: StaticExternalAdapter = StaticExternalAdapter::new(
    "swe-bench-pro",
    &[
        ("bug-fix", "One SWE-Bench Pro bug-fix trial."),
        ("regression", "SWE-Bench Pro regression trial."),
        ("tests", "SWE-Bench Pro test trial."),
    ],
);
