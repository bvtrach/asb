use super::StaticExternalAdapter;
pub(super) static ADAPTER: StaticExternalAdapter = StaticExternalAdapter::new(
    "bigcodebench",
    &[
        ("smoke", "Small BigCodeBench task."),
        ("standard", "Standard BigCodeBench task."),
    ],
);
