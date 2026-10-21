//! Public selection identity is available before any task environment starts.
use super::session::{DatasetIdentity, TaskIdentity};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::Path;

pub(crate) struct PlanningCatalog {
    pub dataset: DatasetIdentity,
    pub task_ids: BTreeSet<String>,
    pub evaluation_kind: &'static str,
}

/// A selection is not an execution attempt or evidence of a passing baseline.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TaskSelection {
    pub workload: String,
    pub task_id: String,
    pub dataset: Option<DatasetIdentity>,
    pub evaluation_kind: String,
    pub population_size: Option<usize>,
}

impl TaskSelection {
    pub(crate) fn valid(&self) -> bool {
        let bounded = |value: &str, limit| !value.is_empty() && value.len() <= limit;
        bounded(&self.workload, 256)
            && bounded(&self.task_id, 256)
            && bounded(&self.evaluation_kind, 128)
            && self.dataset.is_some() == self.population_size.is_some()
            && self
                .population_size
                .is_none_or(|count| count > 0 && count <= 1_000_000)
            && self.dataset.as_ref().is_none_or(|dataset| {
                bounded(&dataset.source, 1024)
                    && bounded(&dataset.revision, 256)
                    && bounded(&dataset.split, 256)
            })
    }

    pub(crate) fn matches(&self, actual: &TaskIdentity) -> bool {
        self.workload == actual.workload
            && self.task_id == actual.task_id
            && self.dataset == actual.dataset
    }
}

pub(crate) fn describe(
    workload: &str,
    trial: Option<&str>,
    trial_set: Option<&str>,
    directory: &Path,
    catalog: Option<&PlanningCatalog>,
) -> Result<TaskSelection, String> {
    if let Some(catalog) = catalog {
        let task = trial.ok_or_else(|| {
            format!("select an installed instance ID for `{workload}` before starting the sweep")
        })?;
        if !catalog.task_ids.contains(task) {
            return Err(format!(
                "selected instance `{task}` is absent from the installed `{workload}` catalog"
            ));
        }
    }
    if trial_set.is_some()
        && let Some(trial) = trial
    {
        super::selected_trial(workload, trial, trial_set, directory)?;
    }
    let selection = TaskSelection {
        workload: workload.into(),
        task_id: trial.unwrap_or(workload).into(),
        dataset: catalog.map(|catalog| catalog.dataset.clone()),
        evaluation_kind: catalog.map_or_else(
            || super::evaluation_kind(workload, directory).into(),
            |catalog| catalog.evaluation_kind.into(),
        ),
        population_size: catalog.map(|catalog| catalog.task_ids.len()),
    };
    if !selection.valid() {
        return Err(format!(
            "invalid or oversized selection identity for `{workload}`"
        ));
    }
    Ok(selection)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selected_instances_are_validated_without_inventing_attempts_or_baselines() {
        let catalog = PlanningCatalog {
            dataset: DatasetIdentity {
                source: "official-source".into(),
                revision: "a".repeat(40),
                split: "test".into(),
            },
            task_ids: ["issue-1".into(), "issue-2".into()].into_iter().collect(),
            evaluation_kind: "upstream-harness",
        };
        assert!(describe("suite", None, None, Path::new("."), Some(&catalog)).is_err());
        assert!(
            describe(
                "suite",
                Some("unknown"),
                None,
                Path::new("."),
                Some(&catalog)
            )
            .is_err()
        );
        let selection = describe(
            "suite",
            Some("issue-1"),
            None,
            Path::new("."),
            Some(&catalog),
        )
        .unwrap();
        assert_eq!(selection.population_size, Some(2));
        let mut task = TaskIdentity::local("suite", Some("issue-1"));
        task.dataset = Some(catalog.dataset);
        assert!(selection.matches(&task));
        task.attempt_id = "a different actual attempt".into();
        assert!(selection.matches(&task));
        task.dataset.as_mut().unwrap().revision = "b".repeat(40);
        assert!(!selection.matches(&task));
        let record = serde_json::to_value(selection).unwrap();
        assert!(record.get("attempt_id").is_none());
        assert!(record.get("baseline").is_none());
    }
}
