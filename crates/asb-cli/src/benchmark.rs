//! Shared benchmark selections and workload prompts.

use serde::{Deserialize, Serialize};

/// Small, extensible benchmark configuration written by `asb setup`.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BenchmarkConfig {
    pub agent: String,
    pub workload: String,
    pub provider: String,
    pub model: String,
    #[serde(default = "default_workspace")]
    pub workspace: String,
}

fn default_workspace() -> String {
    "./asb-workspace".into()
}

/// Prompt for the built-in starter workloads.
pub fn prompt_for(workload: &str) -> &'static str {
    match workload {
        "repository-navigation" | "original.repository-navigation" => {
            "Inspect the repository and describe its main modules and how they fit together."
        }
        "bug-fix" | "original.bug-fix" => {
            "Find a small reproducible bug in the repository, explain it, and propose a focused fix."
        }
        "feature-addition" | "original.feature-addition" => {
            "Identify a small useful feature for the repository and implement it with a concise explanation."
        }
        "test-generation" | "original.test-generation" => {
            "Inspect the repository and add a focused test for an uncovered behavior."
        }
        "build-repair" | "original.build-repair" => {
            "Inspect the repository build and repair the first reproducible build problem you find."
        }
        "dependency-migration" | "original.dependency-migration" => {
            "Inspect the repository dependencies and describe a safe, focused migration opportunity."
        }
        "refactoring" | "original.refactoring" => {
            "Find one small refactoring that improves clarity without changing behavior, then implement it."
        }
        _ => "Inspect the repository and complete the selected benchmark workload.",
    }
}
