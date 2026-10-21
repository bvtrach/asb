//! Coverage is separate from measured success and from process exit status.
use super::BenchmarkData;
use crate::Error;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

fn metric_scope(benchmark: &BenchmarkData) -> Result<String, Error> {
    serde_json::to_string(&(
        &benchmark.agent,
        &benchmark.provider,
        &benchmark.model,
        &benchmark.workload,
        benchmark
            .selection
            .as_ref()
            .and_then(|s| s.dataset.as_ref())
            .or_else(|| benchmark.task.as_ref().and_then(|t| t.dataset.as_ref())),
        &benchmark.module,
        group_kind(benchmark),
    ))
    .map_err(|e| Error::Run(e.to_string()))
}

pub(super) fn metric_groups(
    benchmarks: &[BenchmarkData],
) -> Result<BTreeMap<String, crate::workloads::outcome::MetricSummary>, Error> {
    use crate::workloads::outcome::{EvaluationOutcome, MetricPolicy};
    let mut groups = BTreeMap::<String, (MetricPolicy, Vec<(String, EvaluationOutcome)>)>::new();
    let mut observed = BTreeMap::<String, Vec<MetricPolicy>>::new();
    for benchmark in benchmarks {
        if let Some(outcome) = &benchmark.benchmark_outcome {
            let policies = observed.entry(metric_scope(benchmark)?).or_default();
            if !policies.contains(&outcome.policy) {
                policies.push(outcome.policy.clone());
            }
        }
    }
    for benchmark in benchmarks {
        if !crate::workloads::resolution_eligible(group_kind(benchmark)) {
            continue;
        }
        let policy = benchmark
            .benchmark_outcome
            .as_ref()
            .map(|o| o.policy.clone())
            .unwrap_or_default();
        let scope = metric_scope(benchmark)?;
        let task = benchmark
            .task
            .as_ref()
            .map(|t| t.task_id.clone())
            .or_else(|| benchmark.selection.as_ref().map(|s| s.task_id.clone()))
            .unwrap_or_else(|| benchmark.label.clone());
        let evaluation = benchmark
            .benchmark_outcome
            .as_ref()
            .map(|o| o.evaluation.clone())
            .unwrap_or_else(|| {
                if benchmark.evaluation_completed {
                    EvaluationOutcome::binary(benchmark.metrics.get("success_rate") == Some(&1.0))
                } else {
                    EvaluationOutcome::Pending {
                        reason: "no completed evaluation recorded".into(),
                    }
                }
            });
        let mut policies = vec![policy];
        // An absent attempt still belongs in every observed policy's coverage.
        // Never make a custom score look complete by dropping missing rows.
        if benchmark.benchmark_outcome.is_none()
            && !benchmark.evaluation_completed
            && let Some(known) = observed.get(&scope)
        {
            policies = known.clone();
        }
        for policy in policies {
            let key =
                serde_json::to_string(&(&scope, &policy)).map_err(|e| Error::Run(e.to_string()))?;
            groups
                .entry(key)
                .or_insert_with(|| (policy, Vec::new()))
                .1
                .push((task.clone(), evaluation.clone()));
        }
    }
    Ok(groups
        .into_iter()
        .map(|(key, (policy, rows))| (key, crate::workloads::outcome::aggregate(policy, &rows)))
        .collect())
}

pub(super) fn metric_html(benchmarks: &[BenchmarkData]) -> Result<String, Error> {
    let mut html = String::from(
        "<section><h2>Benchmark metric policies</h2><p>Estimates use completed named rewards only; incomplete coverage is not an official population score. Uncertainty is the standard error over independent task means, unavailable with fewer than two tasks or unequal repetition counts for attempt means. Repetitions are not pass@k.</p><table><tr><th>Group</th><th>Policy / reward</th><th>Reward coverage</th><th>Tasks</th><th>Estimate</th><th>Task standard error</th></tr>",
    );
    for (key, group) in metric_groups(benchmarks)? {
        let unit = group
            .policy
            .rewards
            .get(&group.policy.reward)
            .map_or("unknown", |spec| spec.unit.as_str());
        html.push_str(&format!("<tr><td>{}</td><td>{} v{} / {} ({})</td><td>{}/{}</td><td>{}</td><td>{}</td><td>{}</td></tr>", super::escape(&key), super::escape(&group.policy.id), group.policy.version, super::escape(&group.policy.reward), super::escape(unit), group.rewarded_attempts, group.selected_attempts, group.unique_tasks, group.estimate.map_or_else(|| "Unavailable".into(), |v| format!("{v:.6}")), group.task_cluster_standard_error.map_or_else(|| "Unavailable".into(), |v| format!("{v:.6}"))));
    }
    html.push_str("</table></section>");
    Ok(html)
}

pub(super) fn group_kind(benchmark: &BenchmarkData) -> &str {
    benchmark
        .selection
        .as_ref()
        .filter(|selection| selection.dataset.is_some())
        .map_or(&benchmark.workload_evaluation, |selection| {
            &selection.evaluation_kind
        })
}

#[derive(Default, Serialize)]
pub(super) struct EvaluationCoverage {
    pub selected_attempts: usize,
    pub eligible_selected_attempts: usize,
    pub evaluated_attempts: usize,
    pub unevaluated_attempts: usize,
    pub unscored_attempts: usize,
}

pub(super) fn evaluation_summary(benchmarks: &[BenchmarkData]) -> EvaluationCoverage {
    let mut summary = EvaluationCoverage {
        selected_attempts: benchmarks.len(),
        ..Default::default()
    };
    for benchmark in benchmarks {
        if crate::workloads::resolution_eligible(group_kind(benchmark)) {
            summary.eligible_selected_attempts += 1;
            summary.evaluated_attempts += usize::from(benchmark.evaluation_completed);
        } else {
            summary.unscored_attempts += 1;
        }
    }
    summary.unevaluated_attempts = summary.eligible_selected_attempts - summary.evaluated_attempts;
    summary
}

#[derive(Default, Serialize)]
pub(super) struct PopulationCoverage {
    label: String,
    selected_attempts: usize,
    evaluated_attempts: usize,
    resolved_attempts: usize,
    selected_tasks: usize,
    evaluated_tasks: usize,
    resolved_tasks: usize,
    population_size: usize,
    selection_coverage: f64,
    evaluation_coverage: f64,
    selected_resolution_lower_bound: f64,
    complete_population_evaluation: bool,
    full_population_resolution_rate: Option<f64>,
    #[serde(skip)]
    nonbinary: bool,
    #[serde(skip)]
    selected: BTreeSet<String>,
    #[serde(skip)]
    evaluated: BTreeSet<String>,
    #[serde(skip)]
    resolved: BTreeSet<String>,
}

impl PopulationCoverage {
    fn observe(
        &mut self,
        task: &str,
        population: usize,
        evaluated: bool,
        resolved: bool,
    ) -> Result<(), Error> {
        if population == 0 || (self.selected_attempts > 0 && self.population_size != population) {
            return Err(Error::Input(
                "inconsistent population size in sweep selection snapshot".into(),
            ));
        }
        self.population_size = population;
        self.selected_attempts += 1;
        self.selected.insert(task.into());
        if evaluated {
            self.evaluated_attempts += 1;
            self.evaluated.insert(task.into());
            if resolved {
                self.resolved_attempts += 1;
                self.resolved.insert(task.into());
            }
        }
        self.selected_tasks = self.selected.len();
        self.evaluated_tasks = self.evaluated.len();
        self.resolved_tasks = self.resolved.len();
        if self.selected_tasks > population {
            return Err(Error::Input(
                "selected task count exceeds the installed population in sweep snapshot".into(),
            ));
        }
        self.selection_coverage = self.selected_tasks as f64 / population as f64;
        self.evaluation_coverage = self.evaluated_attempts as f64 / self.selected_attempts as f64;
        self.selected_resolution_lower_bound =
            self.resolved_attempts as f64 / self.selected_attempts as f64;
        self.complete_population_evaluation =
            self.selected_tasks == population && self.evaluated_attempts == self.selected_attempts;
        // Repeated attempts are not pass@1 or an official one-attempt/task score.
        self.full_population_resolution_rate = (!self.nonbinary
            && self.complete_population_evaluation
            && self.selected_attempts == self.selected_tasks)
            .then_some(self.resolved_tasks as f64 / population as f64);
        Ok(())
    }
}

pub(super) fn populations(
    benchmarks: &[BenchmarkData],
) -> Result<BTreeMap<String, PopulationCoverage>, Error> {
    let mut result: BTreeMap<String, PopulationCoverage> = BTreeMap::new();
    for benchmark in benchmarks {
        let Some(selection) = benchmark.selection.as_ref() else {
            continue;
        };
        let (Some(dataset), Some(population)) = (&selection.dataset, selection.population_size)
        else {
            continue;
        };
        if benchmark.benchmark_outcome.as_ref().is_some_and(|o| {
            o.policy != crate::workloads::outcome::MetricPolicy::default()
                || (o.evaluation.completed() && o.evaluation.resolution().is_none())
        }) {
            continue;
        }
        let key = serde_json::to_string(&(
            &benchmark.agent,
            &benchmark.provider,
            &benchmark.model,
            &selection.workload,
            dataset,
            &benchmark.module,
        ))
        .map_err(|error| Error::Run(error.to_string()))?;
        let group = result.entry(key).or_insert_with(|| PopulationCoverage {
            label: format!(
                "{} / {} / {} / {}",
                benchmark.agent, benchmark.provider, benchmark.model, selection.workload
            ),
            ..Default::default()
        });
        group.nonbinary |= benchmark
            .benchmark_outcome
            .as_ref()
            .is_some_and(|o| o.evaluation.completed() && o.evaluation.resolution().is_none());
        group.observe(
            &selection.task_id,
            population,
            benchmark.evaluation_completed,
            benchmark
                .metrics
                .get("success_rate")
                .is_some_and(|rate| *rate == 1.0),
        )?;
    }
    Ok(result)
}

pub(super) fn html_table(benchmarks: &[BenchmarkData]) -> Result<String, Error> {
    let groups = populations(benchmarks)?;
    if groups.is_empty() {
        return Ok(String::new());
    }
    let mut html = String::from(
        "<section><h2>Population coverage</h2><p>Counts describe the entire saved selection, independent of display filters.</p><div class=\"table-wrap\"><table><thead><tr><th>Agent / provider / model / workload</th><th>Selected tasks / population</th><th>Evaluated / selected attempts</th><th>Resolved unique tasks</th><th>Selected-attempt lower bound</th><th>Complete one-attempt population score</th></tr></thead><tbody>",
    );
    for group in groups.values() {
        let full = group.full_population_resolution_rate.map_or_else(
            || "Not available".into(),
            |rate| format!("{:.1}%", rate * 100.0),
        );
        html.push_str(&format!(
            "<tr><td>{}</td><td>{}/{}</td><td>{}/{}</td><td>{}</td><td>{:.1}%</td><td>{}</td></tr>",
            super::escape(&group.label),
            group.selected_tasks,
            group.population_size,
            group.evaluated_attempts,
            group.selected_attempts,
            group.resolved_tasks,
            group.selected_resolution_lower_bound * 100.0,
            full
        ));
    }
    html.push_str("</tbody></table></div></section>");
    Ok(html)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_populations_and_replicas_never_become_complete_one_attempt_scores() {
        let mut group = PopulationCoverage::default();
        group.observe("one", 3, true, true).unwrap();
        group.observe("two", 3, false, false).unwrap();
        assert_eq!(group.selected_resolution_lower_bound, 0.5);
        assert!(group.full_population_resolution_rate.is_none());
        group.observe("three", 3, true, false).unwrap();
        assert_eq!(group.selected_tasks, 3);
        assert!(!group.complete_population_evaluation);
        let mut repeated = PopulationCoverage::default();
        repeated.observe("one", 1, true, false).unwrap();
        repeated.observe("one", 1, true, true).unwrap();
        assert_eq!(repeated.selected_tasks, 1);
        assert_eq!(repeated.selected_attempts, 2);
        assert_eq!(repeated.resolved_tasks, 1);
        assert!(repeated.complete_population_evaluation);
        assert!(repeated.full_population_resolution_rate.is_none());
    }

    #[test]
    fn complete_original_style_population_uses_official_task_outcomes_not_exit_status() {
        let mut group = PopulationCoverage::default();
        group.observe("one", 2, true, true).unwrap();
        group.observe("two", 2, true, false).unwrap();
        assert_eq!(group.full_population_resolution_rate, Some(0.5));
        assert!(group.observe("third", 2, true, true).is_err());
        assert!(
            PopulationCoverage::default()
                .observe("one", 0, false, false)
                .is_err()
        );
    }
}
