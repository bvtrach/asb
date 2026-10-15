//! Shared benchmark configuration.

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
