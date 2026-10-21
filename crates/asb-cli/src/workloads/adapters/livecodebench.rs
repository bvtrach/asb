use super::StaticExternalAdapter;
pub(super) static ADAPTER: StaticExternalAdapter = StaticExternalAdapter::new(
    "livecodebench",
    "https://raw.githubusercontent.com/LiveCodeBench/LiveCodeBench/28fef95ea8c9f7a547c8329f2cd3d32b92c1fa24/README.md",
    &[
        ("smoke", "Small LiveCodeBench task."),
        ("standard", "Standard LiveCodeBench task."),
    ],
);
