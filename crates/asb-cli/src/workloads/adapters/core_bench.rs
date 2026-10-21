use super::StaticExternalAdapter;
pub(super) static ADAPTER: StaticExternalAdapter = StaticExternalAdapter::new(
    "core-bench",
    "https://raw.githubusercontent.com/siegelz/core-bench/e32a2980e72fe6eb04ee04eb749458f570625663/README.md",
    &[
        ("smoke", "Small CORE-Bench task."),
        ("standard", "Standard CORE-Bench task."),
    ],
);
