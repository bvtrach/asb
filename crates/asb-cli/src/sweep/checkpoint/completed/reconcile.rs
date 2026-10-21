//! Explicit receipt repair for retained, graded, fully cleaned-up outcomes.
//!
//! This checks current local artifact consistency, not historical attestation.
//! Unfinished or ungraded attempts are never converted into completed grades.
use super::*;
use crate::runner::replica::Identity;
use crate::workloads::recovery::{RecoveryContext, RetainedOutcomeContext};

struct Candidate {
    index: usize,
    replica: Identity,
    outcome: PathBuf,
    roots: Vec<Root>,
    context: Value,
    fingerprint: Fingerprint,
}

impl Inputs {
    pub(in crate::sweep) fn reconcile_completed(
        &self,
        parent: &Path,
        results: &Path,
        plan: &crate::sweep::plan::SweepPlan,
    ) -> Result<(), Error> {
        self.reconcile_completed_with(
            parent,
            results,
            plan,
            crate::workloads::recovery::verify_retained,
        )
    }

    fn reconcile_completed_with(
        &self,
        parent: &Path,
        results: &Path,
        plan: &crate::sweep::plan::SweepPlan,
        verify: impl Fn(&RetainedOutcomeContext<'_>) -> Result<(), Error>,
    ) -> Result<(), Error> {
        let closed = self.prepare_closed_slots(parent, results, plan)?;
        let mut candidates = Vec::new();
        let mut groups = Vec::new();
        for (index, entry) in plan.entries.iter().enumerate() {
            let directory = std::path::absolute(results.join(
                crate::sweep::execution::result_name(index, &parent.join(&entry.reference)),
            ))
            .map_err(io_error)?;
            let records = directory.join(".asb-completions");
            match std::fs::symlink_metadata(&records) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(io_error(error)),
                Ok(metadata) if !metadata.is_dir() => {
                    return Err(invalid("retained outcomes are not a real directory"));
                }
                Ok(_) => (),
            }
            if std::fs::symlink_metadata(self.completion_path(index)).is_ok() {
                self.verify_completed(index, &directory, entry)?;
                for slot in 1..=entry.configuration.concurrency {
                    if self.has_replica(index, slot)? {
                        self.verify_replica(
                            index,
                            Identity {
                                index: slot,
                                total: entry.configuration.concurrency,
                            },
                            &directory,
                            entry,
                        )?;
                    }
                }
                continue;
            }
            let expected = crate::sweep::isolated_config(&entry.configuration, &directory);
            let saved: crate::benchmark::BenchmarkConfig =
                toml::from_str(&crate::read_config(&directory.join("benchmark.toml"))?)
                    .map_err(|_| invalid("retained isolated configuration is malformed"))?;
            if saved != expected {
                return Err(invalid(
                    "retained isolated configuration differs from the original selection",
                ));
            }
            let mut slots = BTreeSet::new();
            for file in std::fs::read_dir(&records).map_err(io_error)? {
                let file = file.map_err(io_error)?;
                if slots.len() >= entry.configuration.concurrency
                    || file.path().extension().is_none_or(|value| value != "json")
                {
                    return Err(invalid(
                        "retained outcome population is excessive or unfinished",
                    ));
                }
                let record = ValidatedOutcome::read(&file.path(), entry)?;
                let replica = record.replica.ok_or_else(|| {
                    invalid("receipt reconciliation requires explicit replica identity")
                })?;
                if record.schema_version != 2 || !slots.insert(replica.index) {
                    return Err(invalid(
                        "retained replica identity is missing or duplicated",
                    ));
                }
                if self.has_replica(index, replica.index)? {
                    self.verify_replica(index, replica, &directory, entry)?;
                    continue;
                }
                let reference = allocations::reference(self, index, replica, &directory, entry)?;
                if !record.workload_evidence.contains(&reference.artifacts) {
                    return Err(invalid(
                        "retained evaluator evidence differs from its original allocation",
                    ));
                }
                let outcome = std::path::absolute(file.path()).map_err(io_error)?;
                let (roots, _) = replicas::state(
                    &directory,
                    entry,
                    replica,
                    &outcome,
                    &replicas::start_path(self, index, replica.index),
                    &allocations::path(self, index, replica.index),
                )?;
                let context = replicas::context(index, replica, entry, &self.initial.digest, true);
                let fingerprint = self.snapshotter.fingerprint(&roots, &context, false, &[])?;
                let (root, project_config) =
                    crate::project::load(&directory.join("benchmark.toml"))?;
                verify(&RetainedOutcomeContext {
                    recovery: RecoveryContext {
                        project_root: &root,
                        project_config: &project_config,
                        reference: &reference,
                    },
                    summary: &record.summary,
                })?;
                if self.snapshotter.fingerprint(&roots, &context, false, &[])? != fingerprint {
                    return Err(invalid("retained artifacts changed during reconciliation"));
                }
                candidates.push(Candidate {
                    index,
                    replica,
                    outcome,
                    roots,
                    context,
                    fingerprint,
                });
            }
            if slots.len() == entry.configuration.concurrency {
                outcome_state(&directory, entry)?;
                groups.push((index, directory, entry));
            }
        }
        // No receipt is published until every candidate and existing receipt
        // passed validation. Publication never replaces historical metadata.
        for candidate in &candidates {
            if self
                .snapshotter
                .fingerprint(&candidate.roots, &candidate.context, false, &[])?
                != candidate.fingerprint
            {
                return Err(invalid(
                    "retained artifacts changed before receipt publication",
                ));
            }
        }
        self.publish_closed_slots(closed)?;
        for candidate in candidates {
            self.publish_reconciled(
                candidate.index,
                candidate.replica,
                candidate.outcome,
                candidate.roots,
                candidate.fingerprint,
            )?;
        }
        for (index, directory, entry) in groups {
            self.completed(index, &directory, entry)?;
        }
        Ok(())
    }

    pub(super) fn disclose_reconciliation(
        &self,
        index: usize,
        summary: &mut Value,
    ) -> Result<(), Error> {
        if let Some(slot) = summary["replica_index"]
            .as_u64()
            .and_then(|slot| usize::try_from(slot).ok())
            && self.has_replica(index, slot)?
            && self.reconciled_replica(index, slot)?
        {
            summary["reconciled"] = json!(true);
        }
        Ok(())
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use crate::runner::replica::CompletionObserver;
    use std::sync::Mutex;

    pub(in crate::sweep::checkpoint::completed) fn retained_fixture() -> (
        super::super::tests::Fixture,
        PathBuf,
        crate::sweep::plan::SweepPlan,
    ) {
        let mut fixture = super::super::tests::Fixture::new(false);
        let results = fixture.root.join("sweep-results");
        std::fs::create_dir(&results).unwrap();
        let directory = results.join("000-benchmark");
        std::fs::rename(&fixture.directory, &directory).unwrap();
        fixture.directory = directory;
        std::fs::create_dir_all(fixture.root.join(".asb/attempts")).unwrap();
        std::fs::write(
            fixture.root.join(".asb/project.toml"),
            toml::to_string(&crate::project::ProjectConfig::default()).unwrap(),
        )
        .unwrap();
        let isolated =
            crate::sweep::isolated_config(&fixture.entry.configuration, &fixture.directory);
        std::fs::write(
            fixture.directory.join("benchmark.toml"),
            toml::to_string(&isolated).unwrap(),
        )
        .unwrap();
        let observer = replicas::Observer {
            inputs: &fixture.inputs,
            index: 0,
            directory: &fixture.directory,
            entry: &fixture.entry,
            publication: Mutex::new(()),
        };
        observer.reserve().unwrap();
        let cohort = crate::measurements::ExecutionCohort {
            id: "abc-123".into(),
            selected_replicas: 2,
            admitted_slots: vec![1, 2],
        };
        for index in 1..=2 {
            let replica = Identity { index, total: 2 };
            let config = crate::runner::replica::configuration(&isolated, replica).unwrap();
            let workspace = PathBuf::from(&config.workspace);
            std::fs::create_dir_all(&workspace).unwrap();
            std::fs::write(&config.log_file, "retained private sentinel").unwrap();
            let artifacts = fixture.root.join(format!(".asb/attempts/abc-{index}"));
            std::fs::create_dir(&artifacts).unwrap();
            let outcome = fixture
                .directory
                .join(format!(".asb-completions/{}.json", index - 1));
            let mut record: Value =
                serde_json::from_slice(&std::fs::read(&outcome).unwrap()).unwrap();
            record["schema_version"] = json!(2);
            record["replica"] = json!(replica);
            record["outputs"] = json!([workspace, config.log_file]);
            record["workload_evidence"] = json!([artifacts]);
            record["summary"]["replica_index"] = json!(index);
            record["summary"]["execution_cohort"] = json!(cohort);
            std::fs::write(outcome, serde_json::to_vec(&record).unwrap()).unwrap();
            observer.starting(replica, Some(&cohort)).unwrap();
            observer
                .allocated(
                    replica,
                    &crate::workloads::recovery::AttemptReference {
                        schema_version: 1,
                        preparation_id: format!("abc-{index}"),
                        workload: fixture.entry.selection.workload.clone(),
                        task_id: fixture.entry.selection.task_id.clone(),
                        dataset: fixture.entry.selection.dataset.clone().unwrap(),
                        artifacts,
                    },
                )
                .unwrap();
        }
        let plan=serde_json::from_value(json!({"schema_version":1,"run_id":"fixture","execution_mode":"live","entries":[&fixture.entry]})).unwrap();
        (fixture, results, plan)
    }

    #[test]
    fn receipt_repair_preserves_false_grades_and_authenticates_its_provenance() {
        let (fixture, results, plan) = retained_fixture();
        fixture
            .inputs
            .reconcile_completed_with(&fixture.root, &results, &plan, |context| {
                assert_eq!(context.summary["workload_success"], false);
                Ok(())
            })
            .unwrap();
        let state = fixture
            .inputs
            .resume_outcomes(&fixture.root, &results, &plan)
            .unwrap();
        assert_eq!(state.groups[&0].len(), 2);
        assert!(
            state.groups[&0].iter().all(
                |summary| summary["workload_success"] == false && summary["reconciled"] == true
            )
        );
        let path = replicas::path(&fixture.inputs, 0, 1);
        let original = std::fs::read(&path).unwrap();
        fixture
            .inputs
            .reconcile_completed_with(&fixture.root, &results, &plan, |_| {
                panic!("already sealed grades must not be validated again")
            })
            .unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), original);
        let mut changed: Value = serde_json::from_slice(&original).unwrap();
        changed["reconciled"] = json!(false);
        std::fs::write(&path, serde_json::to_vec(&changed).unwrap()).unwrap();
        assert!(
            fixture
                .inputs
                .resume_outcomes(&fixture.root, &results, &plan)
                .is_err()
        );
    }

    #[test]
    fn any_invalid_candidate_prevents_all_receipt_publication() {
        let (fixture, results, plan) = retained_fixture();
        let result =
            fixture
                .inputs
                .reconcile_completed_with(&fixture.root, &results, &plan, |context| {
                    if context.summary["replica_index"] == 2 {
                        Err(Error::Input("synthetic inconsistent grader".into()))
                    } else {
                        Ok(())
                    }
                });
        assert!(result.is_err());
        assert!(!fixture.inputs.has_replica(0, 1).unwrap());
        assert!(!fixture.inputs.has_replica(0, 2).unwrap());
        assert!(!fixture.inputs.completion_path(0).exists());
    }

    #[test]
    fn each_outcome_obligation_blocks_sealing_continuation_and_reconciliation() {
        // Each mutation starts from the complete current schema-2 fixture,
        // including original reservations, start intents and allocations.
        let mutations = [
            ("schema", "/schema_version", json!(3)),
            ("scope", "/scope", json!("unknown")),
            ("summary schema", "/summary/schema_version", json!(1)),
            ("missing slot", "/replica", Value::Null),
            ("zero slot", "/replica/index", json!(0)),
            ("large slot", "/replica/index", json!(3)),
            ("population", "/replica/total", json!(3)),
            (
                "workload cleanup",
                "/workload_cleanup_confirmed",
                json!(false),
            ),
            ("proxy cleanup", "/proxy_cleanup_confirmed", json!(false)),
            ("cohort malformed", "/summary/execution_cohort", json!({})),
            ("cohort absent", "/summary/execution_cohort", Value::Null),
            ("cohort slot", "/summary/replica_index", json!(1)),
            (
                "cohort population",
                "/summary/execution_cohort/selected_replicas",
                json!(3),
            ),
            (
                "cohort admission",
                "/summary/execution_cohort/admitted_slots",
                json!([1]),
            ),
            ("task malformed", "/summary/task", Value::Null),
            ("task workload", "/summary/task/workload", json!("other")),
            ("task selection", "/summary/task/task_id", json!("other")),
            ("dataset absent", "/summary/task/dataset", Value::Null),
            (
                "dataset source",
                "/summary/task/dataset/source",
                json!("other"),
            ),
            (
                "dataset revision",
                "/summary/task/dataset/revision",
                json!("other"),
            ),
            (
                "dataset split",
                "/summary/task/dataset/split",
                json!("other"),
            ),
            ("attempt empty", "/summary/task/attempt_id", json!("")),
            (
                "attempt bound",
                "/summary/task/attempt_id",
                json!("a".repeat(257)),
            ),
            ("agent", "/summary/agent", json!("other")),
            ("workload", "/summary/workload", json!("other")),
            ("provider", "/summary/provider", json!("other")),
            ("model", "/summary/model", json!("other")),
            ("unknown grade", "/summary/workload_success", Value::Null),
            ("string grade", "/summary/workload_success", json!("false")),
            ("unknown process", "/summary/process_success", Value::Null),
            ("string process", "/summary/process_success", json!("true")),
            (
                "evaluator",
                "/summary/workload_evaluation",
                json!("unknown"),
            ),
            (
                "baseline malformed",
                "/summary/workload_baseline",
                Value::Null,
            ),
            (
                "baseline unsupported",
                "/summary/workload_baseline",
                json!({"kind":"unavailable"}),
            ),
            (
                "baseline commit",
                "/summary/workload_baseline/base_commit",
                json!("not-a-commit"),
            ),
            (
                "evidence bound",
                "/workload_evidence",
                json!(vec!["/missing"; 65]),
            ),
            (
                "proxy bound",
                "/proxy_evidence",
                json!(vec!["/missing"; 65]),
            ),
            ("output bound", "/outputs", json!(vec!["/missing"; 65])),
            ("output relative", "/outputs/0", json!("relative")),
            ("dataset evidence absent", "/workload_evidence", json!([])),
            (
                "evidence relative",
                "/workload_evidence/0",
                json!("relative"),
            ),
            ("evidence missing", "/workload_evidence/0", Value::Null),
            ("proxy relative", "/proxy_evidence", json!(["relative"])),
        ];
        for (name, pointer, replacement) in mutations {
            let (fixture, results, plan) = retained_fixture();
            // A late invalid member must also prevent publishing an earlier
            // valid reconciliation candidate, regardless of directory order.
            let mut record = fixture.read(1);
            *record.pointer_mut(pointer).unwrap() = if name == "evidence missing" {
                json!(fixture.root.join("absent-evaluator-evidence"))
            } else {
                replacement
            };
            fixture.write(1, &record);
            assert!(fixture.seal().is_err(), "seal: {name}");
            assert!(
                fixture
                    .inputs
                    .resume_outcomes(&fixture.root, &results, &plan)
                    .is_err(),
                "resume: {name}"
            );
            assert!(
                fixture
                    .inputs
                    .reconcile_completed_with(&fixture.root, &results, &plan, |_| Ok(()))
                    .is_err(),
                "reconcile: {name}"
            );
            assert!(!fixture.inputs.completion_path(0).exists(), "group: {name}");
            for slot in 1..=2 {
                assert!(
                    !fixture.inputs.has_replica(0, slot).unwrap(),
                    "slot {slot}: {name}"
                );
            }
            assert_eq!(fixture.read(1), record, "unchanged outcome: {name}");
        }
    }
}
