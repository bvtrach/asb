use super::StaticExternalAdapter;
pub(super) static ADAPTER: StaticExternalAdapter = StaticExternalAdapter::new(
    "swe-bench",
    &[
        ("bug-fix", "One repository bug-fix trial."),
        ("regression", "Regression-focused repository repair trial."),
        ("tests", "Test-focused repository repair trial."),
    ],
);
