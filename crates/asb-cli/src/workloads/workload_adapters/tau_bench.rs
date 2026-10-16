use super::StaticExternalAdapter;
pub(super) static ADAPTER: StaticExternalAdapter = StaticExternalAdapter::new(
    "tau-bench",
    &[
        ("smoke", "Small tau-bench scenario."),
        ("standard", "Standard tau-bench scenario."),
    ],
);
