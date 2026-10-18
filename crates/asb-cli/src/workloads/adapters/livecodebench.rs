use super::StaticExternalAdapter;
pub(super) static ADAPTER: StaticExternalAdapter = StaticExternalAdapter::new(
    "livecodebench",
    &[
        ("smoke", "Small LiveCodeBench task."),
        ("standard", "Standard LiveCodeBench task."),
    ],
);
