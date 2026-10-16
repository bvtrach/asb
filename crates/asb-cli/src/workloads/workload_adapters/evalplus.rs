use super::StaticExternalAdapter;
pub(super) static ADAPTER: StaticExternalAdapter = StaticExternalAdapter::new(
    "evalplus",
    &[
        ("smoke", "Small EvalPlus task."),
        ("standard", "Standard EvalPlus task."),
    ],
);
