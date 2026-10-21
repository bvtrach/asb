//! Content-bound outcomes, validated separately from declared input stability.
use super::{Fingerprint, Inputs, Root};
use crate::sweep::plan::PlannedBenchmark;
use crate::workloads::{BaselineEvidence, TaskIdentity};
use crate::{Error, io_error};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const LIMIT: u64 = 1024 * 1024;
const SCOPE: &str = "outcome-evidence-only-not-complete-input-closure";
pub(in crate::sweep) mod allocations;
mod reconcile;
pub(super) mod replicas;
pub(in crate::sweep) mod retained;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Outcome {
    schema_version: u32,
    scope: String,
    workload_cleanup_confirmed: bool,
    #[serde(default)]
    proxy_cleanup_confirmed: bool,
    summary: Value,
    workload_evidence: Vec<PathBuf>,
    #[serde(default)]
    proxy_evidence: Vec<PathBuf>,
    #[serde(default)]
    replica: Option<crate::runner::replica::Identity>,
    #[serde(default)]
    outputs: Vec<PathBuf>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in crate::sweep) struct Completion {
    schema_version: u32,
    scope: String,
    index: usize,
    input_digest: String,
    roots: Vec<Root>,
    fingerprint: Fingerprint,
}

impl Inputs {
    /// Validate all existing groups before permitting reuse or new work. An
    /// incomplete replica group cannot be retried as if its graded members
    /// never happened. Recovery/per-replica continuation is handled separately.
    pub(in crate::sweep) fn resume_outcomes(
        &self,
        parent: &Path,
        results: &Path,
        plan: &crate::sweep::plan::SweepPlan,
    ) -> Result<crate::sweep::resume::State, Error> {
        let mut reused = crate::sweep::resume::State::default();
        for (index, entry) in plan.entries.iter().enumerate() {
            let directory = results.join(crate::sweep::execution::result_name(
                index,
                &parent.join(&entry.reference),
            ));
            match std::fs::symlink_metadata(self.completion_path(index)) {
                Ok(_) => {
                    let (_, summaries) = self.verify_completed(index, &directory, entry)?;
                    for replica in 1..=entry.configuration.concurrency {
                        let identity = crate::runner::replica::Identity {
                            index: replica,
                            total: entry.configuration.concurrency,
                        };
                        if self.has_replica(index, replica)? {
                            self.verify_replica(index, identity, &directory, entry)?;
                        }
                    }
                    reused.groups.insert(index, summaries);
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    match std::fs::symlink_metadata(&directory) {
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
                        Err(error) => return Err(io_error(error)),
                        Ok(_) => {
                            let summaries = self
                                .partial_replicas(index, &directory, entry)
                                .map_err(|error| {
                                    invalid(&format!(
                                        "an unsealed or interrupted replica group exists: {error}"
                                    ))
                                })?;
                            if summaries.len() == entry.configuration.concurrency {
                                if !summaries
                                    .values()
                                    .any(|summary| summary["recovered_grade_only"] == true)
                                {
                                    outcome_state(&directory, entry)?;
                                }
                                reused
                                    .groups
                                    .insert(index, summaries.into_values().collect());
                            } else {
                                reused.slots.insert(index, summaries);
                            }
                        }
                    }
                }
                Err(error) => return Err(io_error(error)),
            }
        }
        for (index, summaries) in &mut reused.groups {
            for summary in summaries {
                self.disclose_reconciliation(*index, summary)?;
            }
        }
        for (index, summaries) in &mut reused.slots {
            for summary in summaries.values_mut() {
                self.disclose_reconciliation(*index, summary)?;
            }
        }
        Ok(reused)
    }

    /// A graded false outcome is completed, not a request for another try.
    /// Errors leave the benchmark result intact and cannot create a reusable
    /// receipt. Reuse additionally requires validating the original input seal.
    pub(in crate::sweep) fn completed(
        &self,
        index: usize,
        directory: &Path,
        entry: &PlannedBenchmark,
    ) -> Result<(), Error> {
        let path = self.completion_path(index);
        if std::fs::symlink_metadata(&path).is_ok() {
            // Never replace a prior receipt with a new interpretation of old
            // results. Changed historical evidence must require a fresh sweep.
            self.verify_completed(index, directory, entry)?;
            return Ok(());
        }
        let (roots, _) = outcome_state(directory, entry)?;
        let context = self.completion_context(index, entry);
        let fingerprint = self.snapshotter.fingerprint(&roots, &context, false, &[])?;
        let receipt = Completion {
            schema_version: 1,
            scope: SCOPE.into(),
            index,
            input_digest: self.initial.digest.clone(),
            roots,
            fingerprint,
        };
        publish(&path, &receipt)
    }

    pub(in crate::sweep) fn verify_completed(
        &self,
        index: usize,
        directory: &Path,
        entry: &PlannedBenchmark,
    ) -> Result<(Completion, Vec<Value>), Error> {
        let receipt: Completion = read_private(&self.completion_path(index))?;
        let (roots, summaries) = outcome_state(directory, entry)?;
        if receipt.schema_version != 1
            || receipt.scope != SCOPE
            || receipt.index != index
            || receipt.input_digest != self.initial.digest
            || !receipt.fingerprint.valid()
            || receipt.roots != roots
        {
            return Err(invalid("receipt identity or evidence roots do not match"));
        }
        let actual = self.snapshotter.fingerprint(
            &roots,
            &self.completion_context(index, entry),
            false,
            &[],
        )?;
        if actual != receipt.fingerprint {
            return Err(invalid("completed result or evaluator evidence changed"));
        }
        Ok((receipt, summaries))
    }

    fn completion_context(&self, index: usize, entry: &PlannedBenchmark) -> Value {
        json!({"protocol":"asb-completion-v1", "scope":SCOPE,
            "index":index,"entry":entry,"input_digest":self.initial.digest})
    }

    pub(in crate::sweep) fn completion_path(&self, index: usize) -> PathBuf {
        self.directory
            .join("completed")
            .join(format!("{index:06}.json"))
    }
}

fn outcome_state(
    directory: &Path,
    entry: &PlannedBenchmark,
) -> Result<(Vec<Root>, Vec<Value>), Error> {
    let directory = std::path::absolute(directory).map_err(io_error)?;
    let outcomes = directory.join(".asb-completions");
    let metadata = std::fs::symlink_metadata(&outcomes).map_err(io_error)?;
    if !metadata.is_dir() {
        return Err(invalid("outcome directory is not a real directory"));
    }
    let mut files = Vec::new();
    for item in std::fs::read_dir(&outcomes).map_err(io_error)? {
        let item = item.map_err(io_error)?;
        if files.len() >= crate::benchmark::MAX_CONCURRENCY
            || item.path().extension().is_none_or(|value| value != "json")
        {
            return Err(invalid("outcome population is excessive or incomplete"));
        }
        files.push(item.path());
    }
    files.sort();
    if files.len() != entry.configuration.concurrency {
        return Err(invalid(
            "completed outcome count does not match selected replicas",
        ));
    }
    let config = &entry.configuration;
    let mut attempts = BTreeSet::new();
    let mut summaries = BTreeMap::new();
    let mut evidence = BTreeSet::new();
    let mut proxy_outputs = BTreeSet::new();
    let mut replica_attempts = BTreeMap::new();
    for file in files {
        let record: Outcome = read_private(&file)?;
        let task = validate_outcome(&record, entry)?;
        if !attempts.insert(task.attempt_id.clone()) {
            return Err(invalid("outcome contains duplicate attempt identities"));
        }
        if let Some(replica) = record.replica
            && replica_attempts
                .insert(replica.index, task.attempt_id.clone())
                .is_some()
        {
            return Err(invalid("outcome contains duplicate replica slots"));
        }
        for path in record.outputs {
            if !path.starts_with(&directory) {
                proxy_outputs.insert(path);
            }
        }
        for path in record.workload_evidence {
            evidence.insert(path);
        }
        for path in record.proxy_evidence {
            proxy_outputs.insert(path);
        }
        summaries.insert(task.attempt_id, record.summary);
    }
    if config
        .measurements
        .as_ref()
        .is_some_and(|settings| settings.enabled)
    {
        let mut measured_attempts = BTreeSet::new();
        for replica in 1..=config.concurrency {
            let name = if config.concurrency == 1 {
                "measurements".into()
            } else {
                format!("measurements-concurrency-{replica}")
            };
            let measured: Value = read_private(&directory.join(name).join("run.json"))?;
            let attempt = measured["task"]["attempt_id"]
                .as_str()
                .ok_or_else(|| invalid("measurement attempt identity is missing"))?;
            let summary = summaries
                .get(attempt)
                .ok_or_else(|| invalid("measurement and outcome identify different attempts"))?;
            if !measured_attempts.insert(attempt.to_owned())
                || replica_attempts
                    .get(&replica)
                    .is_some_and(|expected| expected != attempt)
                || !measurement_matches(&measured, summary)
            {
                return Err(invalid("measurement and outcome disagree on task results"));
            }
        }
    }
    let mut roots = vec![Root::path("configuration-results", directory)];
    for (index, path) in evidence.into_iter().enumerate() {
        roots.push(Root::path(format!("workload-evidence-{index}"), path));
    }
    for (index, path) in proxy_outputs.into_iter().enumerate() {
        roots.push(Root::path(format!("proxy-output-{index}"), path));
    }
    Ok((roots, summaries.into_values().collect()))
}

fn validate_outcome(record: &Outcome, entry: &PlannedBenchmark) -> Result<TaskIdentity, Error> {
    let summary = &record.summary;
    let structured = crate::workloads::outcome::BenchmarkOutcome::read(summary)
        .map_err(|_| invalid("malformed structured benchmark outcome"))?;
    let config = &entry.configuration;
    if !summary["execution_cohort"].is_null() {
        let cohort: crate::measurements::ExecutionCohort =
            serde_json::from_value(summary["execution_cohort"].clone())
                .map_err(|_| invalid("outcome execution cohort is malformed"))?;
        let replica = record
            .replica
            .ok_or_else(|| invalid("outcome cohort has no replica identity"))?;
        if summary["replica_index"].as_u64() != Some(replica.index as u64)
            || !cohort.admits(replica.index, replica.total)
        {
            return Err(invalid(
                "outcome cohort does not match its selected replica",
            ));
        }
    } else if !summary["replica_index"].is_null() {
        return Err(invalid("outcome replica has no execution cohort"));
    }
    let task: TaskIdentity = serde_json::from_value(summary["task"].clone())
        .map_err(|_| invalid("outcome task identity is missing or malformed"))?;
    if !summary["module"].is_null() {
        let module: crate::workloads::contracts::ModuleDescriptor =
            serde_json::from_value(summary["module"].clone())
                .map_err(|_| invalid("malformed recorded module"))?;
        if !module.valid()
            || entry.selection.module.as_ref() != Some(&module)
            || structured
                .as_ref()
                .is_some_and(|outcome| !outcome.policy.matches_module(&module))
        {
            return Err(invalid("recorded module differs from selected protocol"));
        }
    }
    let baseline: BaselineEvidence =
        serde_json::from_value(summary["workload_baseline"].clone())
            .map_err(|_| invalid("outcome baseline evidence is missing or malformed"))?;
    if !matches!(record.schema_version, 1 | 2)
        || (record.schema_version == 2
            && record
                .replica
                .is_none_or(|replica| !replica.valid() || replica.total != config.concurrency))
        || record.scope != "task-outcome-not-yet-a-resume-receipt"
        || !record.workload_cleanup_confirmed
        || !record.proxy_cleanup_confirmed
        || summary["schema_version"] != 2
        || !entry.selection.matches(&task)
        || task.attempt_id.is_empty()
        || task.attempt_id.len() > 256
        || summary["agent"] != config.agent
        || summary["workload"] != config.workload
        || summary["provider"] != config.provider
        || summary["model"] != config.model
        || structured.as_ref().map_or_else(
            || summary["workload_success"].as_bool().is_none(),
            |outcome| {
                !outcome.evaluation.completed()
                    || outcome.cleanup_confirmed != Some(true)
                    || outcome.lineage.execution_attempt != task.attempt_id
                    || outcome.execution_success != summary["process_success"].as_bool()
                    || outcome.evaluation.resolution() != summary["workload_success"].as_bool()
            },
        )
        || summary["process_success"].as_bool().is_none()
        || summary["workload_evaluation"] != entry.selection.evaluation_kind
        || !baseline.supports(&entry.selection.evaluation_kind)
        || record.workload_evidence.len() > 64
        || record.proxy_evidence.len() > 64
        || record.outputs.len() > 64
        || record.outputs.iter().any(|path| !path.is_absolute())
        || (entry.selection.dataset.is_some() && record.workload_evidence.is_empty())
    {
        return Err(invalid(
            "outcome identity, grade, baseline or cleanup is incomplete",
        ));
    }
    for path in &record.workload_evidence {
        if !path.is_absolute() || !path.exists() {
            return Err(invalid(
                "declared evaluator evidence is missing or relative",
            ));
        }
    }
    for path in &record.proxy_evidence {
        if !path.is_absolute() {
            return Err(invalid("declared proxy output path is relative"));
        }
        // A failed agent may have made no calls and produced no recording.
        // Bind that absence too, rather than retrying a graded false task.
    }
    Ok(task)
}

fn measurement_matches(measured: &Value, summary: &Value) -> bool {
    [
        "execution_plan_fingerprint",
        "module",
        "benchmark_outcome",
        "replica_index",
        "execution_cohort",
        "schema_version",
        "task",
        "agent",
        "workload",
        "provider",
        "model",
        "process_success",
        "workload_success",
        "workload_evaluation",
        "workload_baseline",
        "workload_baseline_passed",
        "success_rate",
        "token_usage",
        "elapsed_ms",
        "steps",
        "failure_kind",
        "failure_reason",
        "workload_failure",
        "adapter_smoke_success",
    ]
    .iter()
    .all(|field| measured[*field] == summary[*field])
}

fn invalid(reason: &str) -> Error {
    Error::Input(format!(
        "completed task cannot be reused: {reason}; preserve these results and start a fresh sweep"
    ))
}

pub(in crate::sweep) fn read_private<T: serde::de::DeserializeOwned>(
    path: &Path,
) -> Result<T, Error> {
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = options.open(path).map_err(io_error)?;
    let metadata = file.metadata().map_err(io_error)?;
    if !metadata.is_file() || metadata.len() > LIMIT {
        return Err(invalid("outcome record is not a bounded regular file"));
    }
    let mut bytes = Vec::new();
    file.take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(io_error)?;
    if bytes.len() as u64 > LIMIT {
        return Err(invalid("outcome record exceeds its size limit"));
    }
    serde_json::from_slice(&bytes).map_err(|_| invalid("outcome record is malformed"))
}

pub(in crate::sweep) fn publish(path: &Path, receipt: &impl Serialize) -> Result<(), Error> {
    let directory = path.parent().expect("completion receipt parent");
    if let Ok(metadata) = std::fs::symlink_metadata(directory)
        && !metadata.is_dir()
    {
        return Err(invalid("receipt directory is not a real directory"));
    }
    let mut builder = std::fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(directory).map_err(io_error)?;
    let content = serde_json::to_vec_pretty(receipt)
        .map_err(|_| invalid("cannot encode completion receipt"))?;
    if content.len() as u64 > LIMIT {
        return Err(invalid("completion receipt exceeds its size limit"));
    }
    let temporary = directory.join(format!(
        ".{}.tmp",
        TaskIdentity::local("completion-seal", None).attempt_id
    ));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temporary).map_err(io_error)?;
    let result = file
        .write_all(&content)
        .and_then(|()| file.sync_all())
        // Publish without ever replacing an existing historical receipt.
        .and_then(|()| std::fs::hard_link(&temporary, path));
    let cleanup = std::fs::remove_file(&temporary);
    result.and(cleanup).map_err(io_error)?;
    #[cfg(unix)]
    std::fs::File::open(directory)
        .and_then(|file| file.sync_all())
        .map_err(io_error)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::Snapshotter;
    use super::*;
    use crate::workloads::TaskSelection;

    pub(super) struct Fixture {
        pub(super) root: PathBuf,
        pub(super) directory: PathBuf,
        pub(super) evidence: PathBuf,
        pub(super) inputs: Inputs,
        pub(super) entry: PlannedBenchmark,
    }

    #[test]
    fn continuation_distinguishes_untouched_tasks_from_unsealed_replica_groups() {
        let fixture = Fixture::new(false);
        let plan: crate::sweep::plan::SweepPlan = serde_json::from_value(json!({
            "schema_version":1,"run_id":"abc","execution_mode":"live",
            "entries":[&fixture.entry]
        }))
        .unwrap();
        let results = fixture.root.join("sweep-results");
        std::fs::create_dir(&results).unwrap();
        assert!(
            fixture
                .inputs
                .resume_outcomes(&fixture.root, &results, &plan)
                .unwrap()
                .has_no_saved_outcomes()
        );
        let directory = results.join(crate::sweep::execution::result_name(
            0,
            &fixture.root.join(&fixture.entry.reference),
        ));
        std::fs::create_dir(&directory).unwrap();
        let marker = directory.join("existing-work");
        std::fs::write(&marker, "must be preserved").unwrap();
        let error = fixture
            .inputs
            .resume_outcomes(&fixture.root, &results, &plan)
            .unwrap_err();
        assert!(error.to_string().contains("interrupted replica group"));
        assert_eq!(
            std::fs::read_to_string(marker).unwrap(),
            "must be preserved"
        );
    }

    impl Fixture {
        pub(super) fn new(resolved: bool) -> Self {
            let root = std::env::temp_dir()
                .join(TaskIdentity::local("completion-seal-test", None).attempt_id);
            let directory = root.join("results");
            let evidence = root.join("evaluator");
            std::fs::create_dir_all(directory.join(".asb-completions")).unwrap();
            std::fs::create_dir_all(&evidence).unwrap();
            std::fs::write(evidence.join("grade.json"), "private evaluator sentinel").unwrap();
            std::fs::write(directory.join("run.log"), "private transcript sentinel").unwrap();
            let state = root.join("state");
            let input = root.join("input");
            std::fs::write(&input, "unchanged declared input").unwrap();
            let snapshotter = Snapshotter::open(&state, "python3".into()).unwrap();
            let roots = vec![Root::path("input", input)];
            let context = json!({"scope":"fixture"});
            let initial = snapshotter
                .fingerprint(&roots, &context, false, &[])
                .unwrap();
            let inputs = Inputs {
                directory: state,
                snapshotter,
                roots,
                context,
                initial,
                runtime: None,
            };
            let configuration = toml::from_str("agent='fixture-agent'\nprovider='fixture-provider'\nmodel='fixture-model'\nworkload='fixture-suite'\ntrial='case-1'\nconcurrency=2\n").unwrap();
            let selection = TaskSelection {
                module: None,
                workload: "fixture-suite".into(),
                task_id: "case-1".into(),
                dataset: Some(
                    serde_json::from_value(json!({
                        "source":"fixture/dataset", "revision":"pinned", "split":"test"
                    }))
                    .unwrap(),
                ),
                evaluation_kind: "upstream-harness".into(),
                population_size: Some(2),
            };
            let entry = PlannedBenchmark {
                reference: "benchmark.toml".into(),
                configuration,
                selection,
            };
            let fixture = Self {
                root,
                directory,
                evidence,
                inputs,
                entry,
            };
            for index in 0..2 {
                let mut task = TaskIdentity::local("fixture-suite", Some("case-1"));
                task.dataset = fixture.entry.selection.dataset.clone();
                let outcome = json!({"schema_version":1,"scope":"task-outcome-not-yet-a-resume-receipt",
                    "workload_cleanup_confirmed":true,"workload_evidence":[fixture.evidence],
                    "proxy_cleanup_confirmed":true,"proxy_evidence":[],
                    "summary":{"schema_version":2,"agent":"fixture-agent","provider":"fixture-provider",
                        "model":"fixture-model","workload":"fixture-suite","task":task,
                        "workload_evaluation":"upstream-harness","workload_success":resolved,
                            "process_success":true,
                        "workload_baseline":{"kind":"upstream-task-definition","base_commit":"a".repeat(40)}}});
                fixture.write(index, &outcome);
            }
            fixture
        }
        fn outcome_path(&self, index: usize) -> PathBuf {
            self.directory
                .join(".asb-completions")
                .join(format!("{index}.json"))
        }
        fn read(&self, index: usize) -> Value {
            read_private(&self.outcome_path(index)).unwrap()
        }
        fn write(&self, index: usize, record: &Value) {
            std::fs::write(
                self.outcome_path(index),
                serde_json::to_vec(record).unwrap(),
            )
            .unwrap();
        }
        fn seal(&self) -> Result<(), Error> {
            self.inputs.completed(0, &self.directory, &self.entry)
        }
        fn verify(&self) -> Result<(Completion, Vec<Value>), Error> {
            self.inputs
                .verify_completed(0, &self.directory, &self.entry)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn completed_false_and_true_outcomes_are_sealed_with_private_modes() {
        for resolved in [false, true] {
            let fixture = Fixture::new(resolved);
            fixture.seal().unwrap();
            fixture.verify().unwrap();
            let path = fixture.inputs.completion_path(0);
            let before = std::fs::read(&path).unwrap();
            let text = String::from_utf8(before.clone()).unwrap();
            assert!(!text.contains("private evaluator sentinel"));
            assert!(!text.contains("private transcript sentinel"));
            fixture.seal().unwrap();
            assert_eq!(std::fs::read(&path).unwrap(), before);
            assert_eq!(
                std::fs::read_dir(path.parent().unwrap()).unwrap().count(),
                1
            );
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                assert_eq!(
                    std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                    0o600
                );
                assert_eq!(
                    std::fs::metadata(path.parent().unwrap())
                        .unwrap()
                        .permissions()
                        .mode()
                        & 0o777,
                    0o700
                );
            }
        }
    }

    #[test]
    fn evidence_or_result_changes_cannot_replace_a_historical_completion() {
        for external in [false, true] {
            let fixture = Fixture::new(false);
            fixture.seal().unwrap();
            let path = fixture.inputs.completion_path(0);
            let original = std::fs::read(&path).unwrap();
            // Presentation spools live outside the per-task evidence tree.
            std::fs::write(
                fixture.root.join(".terminal-output-000000.tmp"),
                "temporary output",
            )
            .unwrap();
            fixture.verify().unwrap();
            let changed = if external {
                fixture.evidence.join("grade.json")
            } else {
                fixture.directory.join("run.log")
            };
            std::fs::write(changed, "changed evidence").unwrap();
            assert!(fixture.verify().is_err());
            assert!(fixture.seal().is_err());
            assert_eq!(std::fs::read(path).unwrap(), original);
        }
    }

    #[test]
    fn external_proxy_outputs_and_their_observed_absence_are_bound() {
        for initially_present in [false, true] {
            let fixture = Fixture::new(false);
            let output = fixture.root.join("external-recording.jsonl");
            if initially_present {
                std::fs::write(&output, "private recording sentinel").unwrap();
            }
            for index in 0..2 {
                let mut record = fixture.read(index);
                record["proxy_evidence"] = json!([output]);
                fixture.write(index, &record);
            }
            fixture.seal().unwrap();
            fixture.verify().unwrap();
            let encoded = std::fs::read_to_string(fixture.inputs.completion_path(0)).unwrap();
            assert!(!encoded.contains("private recording sentinel"));
            std::fs::write(&output, "different recording").unwrap();
            assert!(fixture.verify().is_err());
        }
    }

    #[test]
    fn completed_native_evaluation_without_primary_reward_cannot_be_sealed() {
        use crate::workloads::outcome::*;
        for rewards in [json!({}), json!({"secondary":0.5})] {
            let fixture = Fixture::new(false);
            let mut record = fixture.read(0);
            record["summary"]["benchmark_outcome"] = json!({
                "schema_version":1,"execution_success":true,
                "evaluation":{"status":"completed","resolution":null,"rewards":rewards},
                "cleanup_confirmed":true,"evidence":[fixture.evidence],
                "policy":{"id":"accuracy","version":1,"reward":"accuracy","aggregation":"attempt-mean","missing_rewards":"exclude",
                    "rewards":{"accuracy":{"unit":"ratio","range":[0,1]},"secondary":{"unit":"ratio","range":[0,1]}}},
                "lineage":{"execution_attempt":record["summary"]["task"]["attempt_id"],"evaluation_id":"eval-fixture","supersedes":null}
            });
            let parsed: BenchmarkOutcome =
                serde_json::from_value(record["summary"]["benchmark_outcome"].clone()).unwrap();
            assert!(parsed.validate().unwrap_err().contains("primary reward"));
            fixture.write(0, &record);
            assert!(fixture.seal().is_err());
            assert!(!fixture.inputs.completion_path(0).exists());
        }
    }

    #[test]
    fn incomplete_wrong_or_duplicated_outcomes_never_produce_a_receipt() {
        for case in [
            "cleanup",
            "proxy-cleanup",
            "grade",
            "task",
            "dataset",
            "agent",
            "provider",
            "model",
            "baseline",
            "scope",
            "grader",
            "duplicate",
            "duplicate-replica",
            "missing",
            "extra",
            "temporary",
            "relative-evidence",
            "relative-proxy-output",
            "missing-evidence",
            "no-evidence",
        ] {
            let fixture = Fixture::new(false);
            let mut record = fixture.read(0);
            match case {
                "cleanup" => record["workload_cleanup_confirmed"] = json!(false),
                "proxy-cleanup" => record["proxy_cleanup_confirmed"] = json!(false),
                "grade" => record["summary"]["workload_success"] = Value::Null,
                "task" => record["summary"]["task"]["task_id"] = json!("different-task"),
                "dataset" => {
                    record["summary"]["task"]["dataset"]["revision"] = json!("different-revision")
                }
                "agent" | "provider" | "model" => record["summary"][case] = json!("different"),
                "baseline" => {
                    record["summary"]["workload_baseline"] = json!({"kind":"unavailable"})
                }
                "scope" => record["scope"] = json!("not-an-outcome"),
                "grader" => record["summary"]["workload_evaluation"] = json!("adapter-smoke"),
                "duplicate" => {
                    record["summary"]["task"] = fixture.read(1)["summary"]["task"].clone()
                }
                "duplicate-replica" => {
                    record["schema_version"] = json!(2);
                    record["replica"] = json!({"index":1,"total":2});
                    let mut sibling = fixture.read(1);
                    sibling["schema_version"] = json!(2);
                    sibling["replica"] = record["replica"].clone();
                    fixture.write(1, &sibling);
                }
                "missing" => std::fs::remove_file(fixture.outcome_path(1)).unwrap(),
                "extra" => fixture.write(2, &fixture.read(1)),
                "temporary" => std::fs::write(
                    fixture.directory.join(".asb-completions/.unfinished.tmp"),
                    "partial",
                )
                .unwrap(),
                "relative-evidence" => record["workload_evidence"] = json!(["relative"]),
                "relative-proxy-output" => record["proxy_evidence"] = json!(["relative"]),
                "missing-evidence" => {
                    record["workload_evidence"] = json!([fixture.root.join("missing")])
                }
                "no-evidence" => record["workload_evidence"] = json!([]),
                _ => unreachable!(),
            }
            fixture.write(0, &record);
            assert!(fixture.seal().is_err(), "{case}");
            assert!(!fixture.inputs.completion_path(0).exists(), "{case}");
        }
    }

    #[test]
    fn selection_input_or_receipt_changes_prevent_verification() {
        for case in ["model", "replicas", "input", "digest", "index"] {
            let mut fixture = Fixture::new(true);
            fixture.seal().unwrap();
            match case {
                "model" => fixture.entry.configuration.model = "different".into(),
                "replicas" => fixture.entry.configuration.concurrency = 3,
                "input" => fixture.inputs.initial.digest = "b".repeat(64),
                "digest" | "index" => {
                    let path = fixture.inputs.completion_path(0);
                    let mut receipt: Value = read_private(&path).unwrap();
                    if case == "digest" {
                        receipt["fingerprint"]["digest"] = json!("b".repeat(64));
                    } else {
                        receipt["index"] = json!(1);
                    }
                    std::fs::write(path, serde_json::to_vec(&receipt).unwrap()).unwrap();
                }
                _ => unreachable!(),
            }
            assert!(fixture.verify().is_err(), "{case}");
        }
    }

    #[test]
    fn enabled_measurements_must_match_each_unique_outcome() {
        for case in ["matching", "missing", "grade", "duplicate", "tokens"] {
            let mut fixture = Fixture::new(true);
            fixture.entry.configuration.measurements = Some(crate::benchmark::MeasurementConfig {
                enabled: true,
                directory: "original-setting-is-isolated".into(),
                collectors: Vec::new(),
            });
            for replica in 1..=2 {
                let directory = fixture
                    .directory
                    .join(format!("measurements-concurrency-{replica}"));
                std::fs::create_dir(&directory).unwrap();
                let mut summary = fixture.read(replica - 1)["summary"].clone();
                if replica == 2 {
                    match case {
                        "missing" => continue,
                        "grade" => summary["workload_success"] = json!(false),
                        "duplicate" => summary = fixture.read(0)["summary"].clone(),
                        "tokens" => summary["token_usage"] = json!({"total_tokens":999}),
                        _ => (),
                    }
                }
                std::fs::write(
                    directory.join("run.json"),
                    serde_json::to_vec(&summary).unwrap(),
                )
                .unwrap();
            }
            assert_eq!(fixture.seal().is_ok(), case == "matching", "{case}");
            if case == "matching" {
                fixture.verify().unwrap();
            } else {
                assert!(!fixture.inputs.completion_path(0).exists());
            }
        }
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_or_oversized_outcome_records_cannot_be_sealed() {
        for link in [false, true] {
            let fixture = Fixture::new(true);
            let path = fixture.outcome_path(0);
            if link {
                let other = fixture.root.join("other.json");
                std::fs::rename(&path, &other).unwrap();
                std::os::unix::fs::symlink(other, path).unwrap();
            } else {
                std::fs::write(path, vec![b' '; LIMIT as usize + 1]).unwrap();
            }
            assert!(fixture.seal().is_err());
            assert!(!fixture.inputs.completion_path(0).exists());
        }
    }
}
