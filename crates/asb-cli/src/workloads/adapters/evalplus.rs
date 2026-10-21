use super::StaticExternalAdapter;
pub(super) static ADAPTER: StaticExternalAdapter = StaticExternalAdapter::new(
    "evalplus",
    "https://raw.githubusercontent.com/evalplus/evalplus/e5d0ed0bab96280b60b637ec7f15b5e4841b0cb2/README.md",
    &[
        ("smoke", "Small EvalPlus task."),
        ("standard", "Standard EvalPlus task."),
    ],
);
