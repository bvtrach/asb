//! Versioned evaluator output, independent of execution and cleanup.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "kebab-case", deny_unknown_fields)]
pub(crate) enum EvaluationOutcome {
    Completed {
        resolution: Option<bool>,
        rewards: BTreeMap<String, f64>,
        #[serde(default)]
        reason: Option<String>,
    },
    Error {
        reason: String,
    },
    Pending {
        reason: String,
    },
}

impl EvaluationOutcome {
    pub fn binary(resolved: bool) -> Self {
        Self::Completed {
            resolution: Some(resolved),
            rewards: BTreeMap::from([("resolution".into(), if resolved { 1.0 } else { 0.0 })]),
            reason: None,
        }
    }
    pub fn resolution(&self) -> Option<bool> {
        match self {
            Self::Completed { resolution, .. } => *resolution,
            _ => None,
        }
    }
    pub fn completed(&self) -> bool {
        matches!(self, Self::Completed { .. })
    }
    pub fn validate(&self) -> Result<(), String> {
        if let Self::Completed { rewards, .. } = self
            && (rewards.len() > 128
                || rewards
                    .iter()
                    .any(|(name, value)| name.is_empty() || name.len() > 256 || !value.is_finite()))
        {
            return Err("invalid named evaluator rewards".into());
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MetricPolicy {
    pub id: String,
    pub version: u32,
    pub reward: String,
    pub aggregation: Aggregation,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Aggregation {
    AttemptMean,
    TaskMean,
}

impl Default for MetricPolicy {
    fn default() -> Self {
        Self {
            id: "binary-resolution".into(),
            version: 1,
            reward: "resolution".into(),
            aggregation: Aggregation::AttemptMean,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BenchmarkOutcome {
    pub schema_version: u32,
    /// None denotes unavailable historical execution, never successful execution.
    pub execution_success: Option<bool>,
    pub evaluation: EvaluationOutcome,
    pub cleanup_confirmed: Option<bool>,
    pub evidence: Vec<std::path::PathBuf>,
    pub policy: MetricPolicy,
    pub lineage: EvaluationLineage,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct EvaluationLineage {
    pub execution_attempt: String,
    pub evaluation_id: String,
    pub supersedes: Option<String>,
}

impl BenchmarkOutcome {
    pub fn read(record: &serde_json::Value) -> Result<Option<Self>, String> {
        if let Some(value) = record
            .get("benchmark_outcome")
            .filter(|value| !value.is_null())
        {
            let outcome: Self = serde_json::from_value(value.clone())
                .map_err(|e| format!("invalid benchmark outcome: {e}"))?;
            outcome.validate()?;
            return Ok(Some(outcome));
        }
        Ok(None)
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != 1
            || self.policy.version == 0
            || self.policy.id.is_empty()
            || self.policy.id.len() > 256
            || self.policy.reward.is_empty()
            || self.policy.reward.len() > 256
            || self.lineage.execution_attempt.is_empty()
            || self.lineage.execution_attempt.len() > 256
            || self.lineage.evaluation_id.is_empty()
            || self.lineage.evaluation_id.len() > 256
            || self
                .lineage
                .supersedes
                .as_ref()
                .is_some_and(|id| id.is_empty() || id.len() > 256)
            || self.evidence.len() > 64
            || self.lineage.supersedes.as_ref() == Some(&self.lineage.evaluation_id)
        {
            return Err("unsupported outcome schema, metric policy or evaluation lineage".into());
        }
        self.evaluation.validate()
    }
}

#[derive(Debug, Serialize)]
pub(crate) struct MetricSummary {
    pub policy: MetricPolicy,
    pub selected_attempts: usize,
    pub evaluated_attempts: usize,
    pub rewarded_attempts: usize,
    pub unique_tasks: usize,
    pub repetitions_by_task: BTreeMap<String, usize>,
    pub estimate: Option<f64>,
    /// Standard error over task means, treating tasks as independent clusters.
    pub task_cluster_standard_error: Option<f64>,
    pub complete: bool,
}

pub(crate) fn aggregate(
    policy: MetricPolicy,
    rows: &[(String, EvaluationOutcome)],
) -> MetricSummary {
    let mut repetitions = BTreeMap::new();
    let mut values: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    let mut evaluated = 0;
    for (task, outcome) in rows {
        *repetitions.entry(task.clone()).or_insert(0) += 1;
        if let EvaluationOutcome::Completed { rewards, .. } = outcome {
            evaluated += 1;
            if let Some(value) = rewards.get(&policy.reward).filter(|v| v.is_finite()) {
                values.entry(task.clone()).or_default().push(*value);
            }
        }
    }
    let count: usize = values.values().map(Vec::len).sum();
    let means: Vec<f64> = values
        .values()
        .map(|v| v.iter().map(|x| x / v.len() as f64).sum::<f64>())
        .collect();
    let estimate = (count > 0)
        .then(|| match policy.aggregation {
            Aggregation::AttemptMean => values
                .values()
                .flatten()
                .map(|x| x / count as f64)
                .sum::<f64>(),
            Aggregation::TaskMean => means.iter().map(|x| x / means.len() as f64).sum::<f64>(),
        })
        .filter(|v| v.is_finite());
    // Unequal cluster sizes require weighted cluster inference for attempt means.
    let balanced = values
        .values()
        .map(Vec::len)
        .collect::<std::collections::BTreeSet<_>>()
        .len()
        <= 1;
    let standard_error = (means.len() > 1
        && (balanced || policy.aggregation == Aggregation::TaskMean))
        .then(|| {
            let mean = means.iter().sum::<f64>() / means.len() as f64;
            (means.iter().map(|v| (v - mean).powi(2)).sum::<f64>()
                / ((means.len() - 1) * means.len()) as f64)
                .sqrt()
        })
        .filter(|v| v.is_finite());
    MetricSummary {
        policy,
        selected_attempts: rows.len(),
        evaluated_attempts: evaluated,
        rewarded_attempts: count,
        unique_tasks: repetitions.len(),
        repetitions_by_task: repetitions,
        estimate,
        task_cluster_standard_error: standard_error,
        complete: count == rows.len() && !rows.is_empty() && estimate.is_some(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn partial(value: f64) -> EvaluationOutcome {
        EvaluationOutcome::Completed {
            resolution: None,
            rewards: BTreeMap::from([("accuracy".into(), value)]),
            reason: None,
        }
    }
    #[test]
    fn native_rewards_repetitions_coverage_and_cluster_uncertainty() {
        let policy = MetricPolicy {
            id: "accuracy".into(),
            version: 2,
            reward: "accuracy".into(),
            aggregation: Aggregation::TaskMean,
        };
        let rows = vec![
            ("a".into(), partial(0.2)),
            ("a".into(), partial(0.6)),
            ("b".into(), partial(0.8)),
            (
                "c".into(),
                EvaluationOutcome::Pending {
                    reason: "queued".into(),
                },
            ),
        ];
        let summary = aggregate(policy, &rows);
        assert!((summary.estimate.unwrap() - 0.6).abs() < 1e-9);
        assert!((summary.task_cluster_standard_error.unwrap() - 0.2).abs() < 1e-9);
        assert_eq!(summary.repetitions_by_task["a"], 2);
        assert_eq!(summary.evaluated_attempts, 3);
        assert!(!summary.complete);
        assert_eq!(partial(0.8).resolution(), None);
    }
    #[test]
    fn nonfinite_rewards_and_explicit_malformed_records_fail_closed() {
        assert!(partial(f64::NAN).validate().is_err());
        assert!(
            BenchmarkOutcome::read(
                &serde_json::json!({"benchmark_outcome": {"schema_version":42}})
            )
            .is_err()
        );
        assert!(
            BenchmarkOutcome::read(&serde_json::json!({"workload_success":false}))
                .unwrap()
                .is_none()
        );
        let summary = aggregate(
            MetricPolicy::default(),
            &[("a".into(), EvaluationOutcome::binary(false))],
        );
        assert_eq!(summary.estimate, Some(0.0));
        assert!(summary.task_cluster_standard_error.is_none());
    }
}
