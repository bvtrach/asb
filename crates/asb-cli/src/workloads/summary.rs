//! Workload-resolution summary publication.
//!
//! A benchmark run may contain collector output, but the meaning of success is
//! owned by the workload evaluator. Keeping this here prevents measurement
//! collection from deciding workload success policy.

use std::path::Path;

const MAX_RUNS: usize = 10_000;

pub(crate) fn update_summary(dir: &Path, run: serde_json::Value) -> Result<(), String> {
    let path = dir.join("summary.json");
    crate::evidence::update_json(&path, |old| {
        let mut document = if let Some(bytes) = old {
            serde_json::from_slice::<serde_json::Value>(bytes)
                .map_err(|_| "historical summary JSON is malformed; preserve it before repair")?
                .as_object()
                .cloned()
                .ok_or("historical summary must be an object; preserve it before repair")?
        } else {
            serde_json::Map::new()
        };
        let mut runs = match document.remove("runs") {
            Some(serde_json::Value::Array(runs))
                if runs.iter().all(serde_json::Value::is_object) =>
            {
                runs
            }
            None if old.is_none() => Vec::new(),
            _ => return Err(
                "historical summary runs must be an array of objects; preserve it before repair"
                    .into(),
            ),
        };
        runs.push(run);
        if runs.len() > MAX_RUNS {
            runs.drain(..runs.len() - MAX_RUNS);
        }
        let mut resolution = super::ResolutionSummary::default();
        for run in &runs {
            let kind = run
                .get("workload_evaluation")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("unknown");
            let kind = super::scored_kind(kind, &super::BaselineEvidence::from_record(run));
            resolution.observe(
                kind,
                run.get("workload_success")
                    .and_then(serde_json::Value::as_bool)
                    == Some(true),
            );
        }
        let fields = serde_json::json!({
            "schema_version": 2,
            "runs": runs,
            "success_count": resolution.resolved_count,
            "failure_count": resolution.unresolved_count,
            "task_count": resolution.task_count,
            "unscored_count": resolution.unscored_count,
            "success_rate": resolution.success_rate,
            "success_rate_definition": "resolved task attempts / all selected task attempts; smoke exercises excluded; not pass@k"
        });
        document.extend(fields.as_object().expect("summary object").clone());
        Ok(document)
    })
}
