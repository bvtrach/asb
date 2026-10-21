use super::StaticExternalAdapter;
pub(super) static ADAPTER: StaticExternalAdapter = StaticExternalAdapter::new(
    "bigcodebench",
    "https://raw.githubusercontent.com/bigcode-project/bigcodebench/9059fb84d1188c02edeac4995361656a2fdecbef/README.md",
    &[
        ("smoke", "Small BigCodeBench task."),
        ("standard", "Standard BigCodeBench task."),
    ],
);
