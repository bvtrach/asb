use super::StaticExternalAdapter;
pub(super) static ADAPTER: StaticExternalAdapter = StaticExternalAdapter::new(
    "swe-rebench",
    &[
        ("smoke", "Small SWE-ReBench task."),
        ("standard", "Standard SWE-ReBench task."),
    ],
);
