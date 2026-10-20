//! Shared benchmark configuration.

use serde::{Deserialize, Serialize};

/// Small, extensible benchmark configuration written by `asb setup`.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct BenchmarkConfig {
    pub agent: String,
    pub workload: String,
    pub provider: String,
    pub model: String,
    /// Number of identical agent executions to run concurrently.
    #[serde(
        default = "default_concurrency",
        deserialize_with = "deserialize_concurrency"
    )]
    pub concurrency: usize,
    /// Optional trial-set TOML selected for an external workload.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trial_set: Option<String>,
    /// Optional trial identifier within the selected trial set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trial: Option<String>,
    #[serde(default = "default_workspace")]
    pub workspace: String,
    #[serde(default = "default_log_file")]
    pub log_file: String,
    #[serde(default)]
    pub measurements: Option<MeasurementConfig>,
}

/// Measurement modules selected by generated setup files.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct MeasurementConfig {
    pub enabled: bool,
    pub directory: String,
    pub collectors: Vec<String>,
}

fn default_workspace() -> String {
    "./asb-workspace".into()
}

fn default_log_file() -> String {
    "benchmark.log".into()
}

fn default_concurrency() -> usize {
    1
}

fn deserialize_concurrency<'de, D>(deserializer: D) -> Result<usize, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = usize::deserialize(deserializer)?;
    (value > 0)
        .then_some(value)
        .ok_or_else(|| serde::de::Error::custom("concurrency must be at least 1"))
}

#[cfg(test)]
mod tests {
    use super::BenchmarkConfig;

    #[test]
    fn concurrency_defaults_to_one_and_accepts_positive_values() {
        let config: BenchmarkConfig = toml::from_str(
            "agent='pi'\nworkload='bug-fix'\nprovider='openrouter'\nmodel='tool-model'\n",
        )
        .expect("default configuration parses");
        assert_eq!(config.concurrency, 1);

        let config: BenchmarkConfig = toml::from_str(
            "agent='pi'\nworkload='bug-fix'\nprovider='openrouter'\nmodel='tool-model'\nconcurrency=3\n",
        )
        .expect("positive concurrency parses");
        assert_eq!(config.concurrency, 3);
    }

    #[test]
    fn zero_concurrency_is_rejected() {
        let error = toml::from_str::<BenchmarkConfig>(
            "agent='pi'\nworkload='bug-fix'\nprovider='openrouter'\nmodel='tool-model'\nconcurrency=0\n",
        )
        .expect_err("zero concurrency must be invalid");
        assert!(error.to_string().contains("concurrency must be at least 1"));
    }
}
