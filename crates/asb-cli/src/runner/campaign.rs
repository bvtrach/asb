//! Side-effect-free persisted execution layout shared by run, sweep and reports.
//! Deserialization is not admission: consumers must validate before activation.
use crate::{benchmark::BenchmarkConfig, planning};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Slot {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replay_track: Option<u64>,
    pub id: u64,
    pub batch: u64,
    pub replica: usize,
    pub workspace: PathBuf,
    pub log: PathBuf,
    pub measurements: Option<PathBuf>,
    pub replay_input: Option<usize>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReplayInput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub complete: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub track: Option<u64>,
    pub source: PathBuf,
    pub private: PathBuf,
    pub bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Plan {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replay_selection: Option<crate::cassette::SelectionPlan>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recording: Option<PathBuf>,
    pub admission: planning::BatchAdmissionPolicy,
    pub schema_version: u32,
    pub scope: String,
    pub campaign: String,
    pub source: PathBuf,
    pub configuration: BenchmarkConfig,
    pub configuration_sha256: String,
    pub repeat: u64,
    pub concurrency: usize,
    pub mode: planning::ExecutionMode,
    pub respect_record_timings: bool,
    pub measurement_root: Option<PathBuf>,
    pub replay_inputs: Vec<ReplayInput>,
    pub slots: Vec<Slot>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn owned_campaign_retains_existing_wire_shapes_without_invented_replay_facts() {
        let configuration: BenchmarkConfig = toml::from_str(
            "agent='pi'\nprovider='openrouter'\nmodel='fixture'\nworkload='bug-fix'",
        )
        .unwrap();
        let wire = json!({
            "admission": planning::BatchAdmissionPolicy::current(),
            "schema_version": 1,
            "scope": "intended-executions-not-attempt-evidence",
            "campaign": "fixture", "source": "/project/benchmark.toml",
            "configuration": configuration,
            "configuration_sha256": "a".repeat(64),
            "repeat": 1, "concurrency": 1, "mode": "replay",
            "respect_record_timings": false, "measurement_root": null,
            "replay_inputs": [{"source":"/project/legacy.jsonl", "private":"inputs/1.jsonl", "bytes":42, "sha256":"b".repeat(64)}],
            "slots": [{"id":1,"batch":1,"replica":1,"workspace":"batch-1/workspace", "log":"batch-1/run.jsonl", "measurements":null,"replay_input":0}]
        });
        let plan: Plan = serde_json::from_value(wire.clone()).unwrap();
        assert_eq!(serde_json::to_value(plan).unwrap(), wire);
        let mut bundle = wire;
        bundle["replay_inputs"][0]["complete"] = json!(false);
        bundle["replay_inputs"][0]["track"] = json!(7);
        bundle["slots"][0]["replay_track"] = json!(7);
        let plan: Plan = serde_json::from_value(bundle.clone()).unwrap();
        assert_eq!(serde_json::to_value(plan).unwrap(), bundle);
        for location in ["plan", "slot", "input"] {
            let mut changed = bundle.clone();
            match location {
                "slot" => changed["slots"][0]["unexpected"] = json!(true),
                "input" => changed["replay_inputs"][0]["unexpected"] = json!(true),
                _ => changed["unexpected"] = json!(true),
            }
            assert!(
                serde_json::from_value::<Plan>(changed).is_err(),
                "{location}"
            );
        }
    }
}
