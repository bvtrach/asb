//! Coverage is separate from measured success and from process exit status.
use super::BenchmarkData;
use crate::Error;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

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
        self.full_population_resolution_rate = (self.complete_population_evaluation
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
        let key = serde_json::to_string(&(
            &benchmark.agent,
            &benchmark.provider,
            &benchmark.model,
            &selection.workload,
            dataset,
        ))
        .map_err(|error| Error::Run(error.to_string()))?;
        result
            .entry(key)
            .or_insert_with(|| PopulationCoverage {
                label: format!(
                    "{} / {} / {} / {}",
                    benchmark.agent, benchmark.provider, benchmark.model, selection.workload
                ),
                ..Default::default()
            })
            .observe(
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
