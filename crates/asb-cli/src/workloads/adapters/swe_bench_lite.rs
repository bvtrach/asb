use super::StaticExternalAdapter;
pub(super) static ADAPTER: StaticExternalAdapter = StaticExternalAdapter::new(
    "swe-bench-lite",
    &[
        ("bug-fix", "One SWE-Bench Lite bug-fix trial."),
        ("regression", "SWE-Bench Lite regression trial."),
        ("tests", "SWE-Bench Lite test trial."),
    ],
);
