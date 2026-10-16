use super::StaticExternalAdapter;
pub(super) static ADAPTER: StaticExternalAdapter = StaticExternalAdapter::new(
    "terminal-bench",
    &[
        (
            "smoke",
            "Short terminal task suitable for a local smoke run.",
        ),
        (
            "shell",
            "Shell and filesystem task from the local Terminal-Bench checkout.",
        ),
        (
            "debug",
            "Debugging task from the local Terminal-Bench checkout.",
        ),
    ],
);
