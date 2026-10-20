//! Adapter-owned state for one workload attempt.
//!
//! The runner consumes this contract, not dataset formats, test commands or
//! benchmark names. Upstream adapters can own containers and evaluator state
//! for exactly one attempt instead of sharing mutable state through a registry.

use super::Evaluation;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static NEXT_ATTEMPT: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DatasetIdentity {
    pub source: String,
    pub revision: String,
    pub split: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct TaskIdentity {
    pub workload: String,
    pub task_id: String,
    pub attempt_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dataset: Option<DatasetIdentity>,
}

impl TaskIdentity {
    pub(crate) fn local(workload: &str, task_id: Option<&str>) -> Self {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        Self {
            workload: workload.into(),
            task_id: task_id.unwrap_or(workload).into(),
            attempt_id: format!(
                "{timestamp:x}-{:x}-{:x}",
                std::process::id(),
                NEXT_ATTEMPT.fetch_add(1, Ordering::Relaxed)
            ),
            dataset: None,
        }
    }
}

pub(crate) struct PreparationContext<'a> {
    pub workload: &'a str,
    pub trial: Option<&'a str>,
    pub trial_set: Option<&'a str>,
    pub workload_directory: &'a Path,
    pub workspace: &'a Path,
}

pub(crate) struct TaskInvocation {
    pub identity: TaskIdentity,
    pub prompt: String,
    pub workspace: PathBuf,
}

pub(crate) trait WorkloadSession {
    fn invocation(&self) -> &TaskInvocation;
    fn evaluation_kind(&self) -> &'static str;
    fn baseline_passed(&self) -> Option<bool>;
    fn evaluate(&mut self, final_response: &str) -> Result<Evaluation, String>;
}

pub(super) type Evaluator = fn(&str, &Path, &str) -> Result<Evaluation, String>;

pub(super) struct FileSession {
    invocation: TaskInvocation,
    evaluator: Evaluator,
    evaluation_kind: &'static str,
    baseline_passed: Option<bool>,
}

impl FileSession {
    pub fn new(
        context: &PreparationContext<'_>,
        prompt: String,
        evaluator: Evaluator,
        evaluation_kind: &'static str,
    ) -> Self {
        let baseline_passed = evaluator(context.workload, context.workspace, "")
            .ok()
            .map(|result| result == Evaluation::Passed);
        Self {
            invocation: TaskInvocation {
                identity: TaskIdentity::local(context.workload, context.trial),
                prompt,
                workspace: context.workspace.to_path_buf(),
            },
            evaluator,
            evaluation_kind,
            baseline_passed,
        }
    }
}

impl WorkloadSession for FileSession {
    fn invocation(&self) -> &TaskInvocation {
        &self.invocation
    }
    fn evaluation_kind(&self) -> &'static str {
        self.evaluation_kind
    }
    fn baseline_passed(&self) -> Option<bool> {
        self.baseline_passed
    }
    fn evaluate(&mut self, final_response: &str) -> Result<Evaluation, String> {
        (self.evaluator)(
            &self.invocation.identity.workload,
            &self.invocation.workspace,
            final_response,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_tasks_have_distinct_attempts_without_changing_task_identity() {
        let first = TaskIdentity::local("suite", Some("owner__repo-123"));
        let second = TaskIdentity::local("suite", Some("owner__repo-123"));
        assert_eq!(first.workload, second.workload);
        assert_eq!(first.task_id, second.task_id);
        assert_ne!(first.attempt_id, second.attempt_id);
        let encoded = serde_json::to_string(&first).unwrap();
        assert_eq!(
            serde_json::from_str::<TaskIdentity>(&encoded).unwrap(),
            first
        );
        assert!(!encoded.contains("dataset"));
    }

    #[test]
    fn dataset_revision_and_split_survive_serialization() {
        let mut task = TaskIdentity::local("suite", Some("case-1"));
        task.dataset = Some(DatasetIdentity {
            source: "official/dataset".into(),
            revision: "pinned-revision".into(),
            split: "test".into(),
        });
        assert_eq!(
            serde_json::from_value::<TaskIdentity>(serde_json::to_value(&task).unwrap()).unwrap(),
            task
        );
    }

    #[test]
    fn session_owns_prepared_prompt_workspace_and_evaluator() {
        let root = std::env::temp_dir().join(format!(
            "asb-session-{}",
            TaskIdentity::local("fixture", None).attempt_id
        ));
        let workloads = root.join("workloads");
        let workspace = root.join("workspace");
        std::fs::create_dir_all(&workloads).unwrap();
        super::super::populate(&workloads).unwrap();
        let mut session = super::super::prepare_task(&PreparationContext {
            workload: "bug-fix",
            trial: None,
            trial_set: None,
            workload_directory: &workloads,
            workspace: &workspace,
        })
        .unwrap();
        assert_eq!(session.invocation().workspace, workspace);
        assert_eq!(session.invocation().identity.task_id, "bug-fix");
        assert!(!session.invocation().prompt.trim().is_empty());
        assert_eq!(session.baseline_passed(), Some(false));
        assert_eq!(session.evaluation_kind(), "bundled-fixture");
        assert!(matches!(
            session.evaluate("done"),
            Ok(Evaluation::Failed(_))
        ));
        std::fs::write(workspace.join("src/parser.py"), "def parse_path(value: str) -> list[str]:\n    return [segment for segment in value.split('/') if segment]\n").unwrap();
        assert_eq!(session.evaluate(""), Ok(Evaluation::Passed));
        // Baseline evidence belongs to this attempt, not the mutated workspace.
        assert_eq!(session.baseline_passed(), Some(false));
        let later = super::super::prepare_task(&PreparationContext {
            workload: "bug-fix",
            trial: None,
            trial_set: None,
            workload_directory: &workloads,
            workspace: &workspace,
        })
        .unwrap();
        assert_eq!(later.baseline_passed(), Some(true));
        assert_ne!(
            later.invocation().identity.attempt_id,
            session.invocation().identity.attempt_id
        );
        let _ = std::fs::remove_dir_all(root);
    }
}
