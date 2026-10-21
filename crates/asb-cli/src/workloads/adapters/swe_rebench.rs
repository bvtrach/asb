use super::StaticExternalAdapter;
pub(super) static ADAPTER: StaticExternalAdapter = StaticExternalAdapter::new(
    "swe-rebench",
    "https://raw.githubusercontent.com/SWE-rebench/SWE-bench-fork/e4907b7a90eafaa1f0a6428fd04fe31cdd8b4284/README.md",
    &[
        ("smoke", "Small SWE-ReBench task."),
        ("standard", "Standard SWE-ReBench task."),
    ],
);
