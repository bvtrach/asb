use super::StaticExternalAdapter;
pub(super) static ADAPTER: StaticExternalAdapter = StaticExternalAdapter::new(
    "swe-bench-verified",
    &[
        ("bug-fix", "One SWE-Bench Verified bug-fix trial."),
        ("regression", "Verified regression trial."),
        ("tests", "Verified test trial."),
    ],
);
