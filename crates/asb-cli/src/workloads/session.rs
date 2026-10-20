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
    pub on_allocation: Option<&'a super::recovery::AllocationObserver<'a>>,
    pub workload: &'a str,
    pub trial: Option<&'a str>,
    pub trial_set: Option<&'a str>,
    pub workload_directory: &'a Path,
    pub workspace: &'a Path,
    pub project_root: &'a Path,
    pub project_config: &'a crate::project::ProjectConfig,
}

impl PreparationContext<'_> {
    pub(crate) fn tool_root(&self, id: &str) -> PathBuf {
        crate::project::resolve(self.project_root, &self.project_config.tools_dir).join(id)
    }
}

pub(crate) struct TaskInvocation {
    pub identity: TaskIdentity,
    pub prompt: String,
    pub workspace: PathBuf,
}

/// The runner provides negotiated connections, never workload/agent dispatch.
pub(crate) struct ExecutionContext<'a> {
    pub config: &'a crate::benchmark::BenchmarkConfig,
    pub connection: Option<&'a crate::endpoint::Connection>,
    pub project_root: &'a Path,
    pub project_config: &'a crate::project::ProjectConfig,
    pub replay: bool,
    pub measurements: Option<&'a mut crate::measurements::Session>,
    pub show_output: bool,
}

pub(crate) struct ExecutionResult {
    pub output: std::process::Output,
    pub metadata: crate::agents::driver::GuestResult,
    pub elapsed_ms: Option<u128>,
}

/// Baseline provenance is not a boolean: an official clean task definition
/// does not claim that ASB ran its initial failing tests.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub(crate) enum BaselineEvidence {
    Measured {
        passed: Option<bool>,
    },
    UpstreamTaskDefinition {
        base_commit: String,
    },
    UpstreamEnvironmentDefinition {
        source_revision: String,
        definition_sha256: String,
    },
    Unavailable,
}

impl BaselineEvidence {
    pub(crate) fn measured_passed(&self) -> Option<bool> {
        match self {
            Self::Measured { passed } => *passed,
            _ => None,
        }
    }

    pub(crate) fn supports(&self, evaluation_kind: &str) -> bool {
        let hash = |value: &str, length| {
            value.len() == length && value.bytes().all(|byte| byte.is_ascii_hexdigit())
        };
        match self {
            Self::Measured {
                passed: Some(false),
            } => true,
            Self::UpstreamTaskDefinition { base_commit } => {
                evaluation_kind == "upstream-harness"
                    && base_commit.len() == 40
                    && base_commit.bytes().all(|byte| byte.is_ascii_hexdigit())
            }
            Self::UpstreamEnvironmentDefinition {
                source_revision,
                definition_sha256,
            } => {
                evaluation_kind == "upstream-harness"
                    && hash(source_revision, 40)
                    && hash(definition_sha256, 64)
            }
            _ => false,
        }
    }

    /// Legacy records retain their original measured-baseline semantics.
    /// An explicitly malformed new field must never fall back to a legacy flag.
    pub(crate) fn from_record(record: &serde_json::Value) -> Self {
        match record.get("workload_baseline") {
            Some(value) => serde_json::from_value(value.clone()).unwrap_or(Self::Unavailable),
            None => Self::Measured {
                passed: record
                    .get("workload_baseline_passed")
                    .and_then(serde_json::Value::as_bool),
            },
        }
    }
}

pub(crate) trait WorkloadSession {
    fn invocation(&self) -> &TaskInvocation;
    fn evaluation_kind(&self) -> &'static str;
    fn baseline_passed(&self) -> Option<bool>;
    fn baseline_evidence(&self) -> BaselineEvidence {
        BaselineEvidence::Measured {
            passed: self.baseline_passed(),
        }
    }
    fn evaluate(&mut self, final_response: &str) -> Result<Evaluation, String>;
    /// Adapter-owned evaluator artifacts that bind a historical task outcome.
    /// This is not a request to inspect unrelated project or host directories.
    fn checkpoint_evidence(&self) -> Vec<PathBuf> {
        Vec::new()
    }
    /// Confirm owned resource cleanup before reporting the run as completed.
    fn finish(&mut self) -> Result<(), String> {
        Ok(())
    }
    fn execute(
        &mut self,
        _context: &mut ExecutionContext<'_>,
        _output: &mut dyn std::io::Write,
    ) -> Option<Result<ExecutionResult, String>> {
        None
    }
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
    #[test]
    fn external_catalog_location_cannot_select_another_projects_tools() {
        let config = crate::project::ProjectConfig {
            tools_dir: "local-tools".into(),
            ..Default::default()
        };
        let context = super::PreparationContext {
            on_allocation: None,
            workload: "fixture",
            trial: None,
            trial_set: None,
            workload_directory: std::path::Path::new("/shared/catalogs/outside-the-project"),
            workspace: std::path::Path::new("/fixture/project/workspace"),
            project_root: std::path::Path::new("/fixture/project"),
            project_config: &config,
        };
        assert_eq!(
            context.tool_root("fixture-sdk"),
            std::path::Path::new("/fixture/project/local-tools/fixture-sdk")
        );
    }
    use super::*;

    #[test]
    fn official_definition_is_not_a_measured_failing_baseline() {
        let baseline = BaselineEvidence::UpstreamTaskDefinition {
            base_commit: "0123456789abcdef0123456789abcdef01234567".into(),
        };
        assert_eq!(baseline.measured_passed(), None);
        assert!(baseline.supports("upstream-harness"));
        assert!(!baseline.supports("bundled-fixture"));
        assert_eq!(
            super::super::scored_kind("upstream-harness", &baseline),
            "upstream-harness"
        );
        assert_eq!(
            super::super::scored_kind("bundled-fixture", &baseline),
            "invalid-baseline"
        );
        let record = serde_json::json!({"workload_baseline": baseline});
        assert_eq!(BaselineEvidence::from_record(&record), baseline);
    }

    #[test]
    fn baseline_provenance_rejects_malformed_and_explicitly_missing_evidence() {
        for value in [
            serde_json::json!(null),
            serde_json::json!({"kind":"unavailable"}),
            serde_json::json!({"kind":"upstream-task-definition","base_commit":"HEAD"}),
            serde_json::json!({"kind":"upstream-task-definition","base_commit":"g".repeat(40)}),
            serde_json::json!({"kind":"upstream-task-definition","base_commit":"a".repeat(40),"unexpected":true}),
            serde_json::json!({"kind":"measured","passed":true}),
        ] {
            let record =
                serde_json::json!({"workload_baseline":value,"workload_baseline_passed":false});
            let evidence = BaselineEvidence::from_record(&record);
            assert!(!evidence.supports("upstream-harness"));
        }
        let legacy = serde_json::json!({"workload_baseline_passed":false});
        assert!(BaselineEvidence::from_record(&legacy).supports("bundled-fixture"));
        assert!(!BaselineEvidence::from_record(&serde_json::json!({})).supports("bundled-fixture"));
    }

    #[test]
    fn environment_definition_does_not_invent_a_repository_or_measured_test_baseline() {
        let baseline = BaselineEvidence::UpstreamEnvironmentDefinition {
            source_revision: "a".repeat(40),
            definition_sha256: "b".repeat(64),
        };
        assert!(baseline.supports("upstream-harness"));
        assert!(!baseline.supports("bundled-fixture"));
        assert_eq!(baseline.measured_passed(), None);
        assert_eq!(
            BaselineEvidence::from_record(&serde_json::json!({"workload_baseline":baseline})),
            baseline
        );
        let bad = BaselineEvidence::UpstreamEnvironmentDefinition {
            source_revision: "HEAD".into(),
            definition_sha256: "b".repeat(64),
        };
        assert!(!bad.supports("upstream-harness"));
    }

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
            on_allocation: None,
            workload: "bug-fix",
            trial: None,
            trial_set: None,
            workload_directory: &workloads,
            workspace: &workspace,
            project_root: &root,
            project_config: &crate::project::ProjectConfig::default(),
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
            on_allocation: None,
            workload: "bug-fix",
            trial: None,
            trial_set: None,
            workload_directory: &workloads,
            workspace: &workspace,
            project_root: &root,
            project_config: &crate::project::ProjectConfig::default(),
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
