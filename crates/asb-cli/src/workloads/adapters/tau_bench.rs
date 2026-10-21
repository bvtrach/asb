use super::StaticExternalAdapter;
pub(super) static ADAPTER: StaticExternalAdapter = StaticExternalAdapter::new(
    "tau-bench",
    "https://raw.githubusercontent.com/sierra-research/tau-bench/59a200c6d575d595120f1cb70fea53cef0632f6b/README.md",
    &[
        ("smoke", "Small tau-bench scenario."),
        ("standard", "Standard tau-bench scenario."),
    ],
);
