//! Explicit grade-only closure of allocated slots whose run record was lost.
//! This never invents execution measurements or authorizes retrying a client.
use super::*;
use crate::runner::replica::Identity;
use crate::workloads::recovery::{AttemptReference, inspect_reference};

const SCOPE: &str = "closed-slot-official-grade-only";

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    schema_version: u32,
    scope: String,
    configuration: usize,
    replica: Identity,
    input_digest: String,
    reference: AttemptReference,
    binding: Value,
    report: Value,
    roots: Vec<Root>,
    fingerprint: Fingerprint,
}

pub(super) struct Pending {
    path: PathBuf,
    context: Value,
    receipt: Receipt,
}

fn path(checkpoint: &Path, index: usize, slot: usize) -> PathBuf {
    checkpoint
        .join("closed-slots")
        .join(format!("{index:06}/{slot:03}.json"))
}

fn binding(
    checkpoint: &Path,
    index: usize,
    replica: Identity,
    directory: &Path,
    entry: &PlannedBenchmark,
) -> Result<(AttemptReference, Value), Error> {
    let reference = allocations::reporting_reference(checkpoint, index, replica, directory, entry)?;
    let relative = format!("{index:06}/{:03}.json", replica.index);
    let start: Value = read_private(&checkpoint.join("started").join(&relative))?;
    let allocation: Value = read_private(&checkpoint.join("allocated").join(relative))?;
    Ok((reference, json!({"intent":start,"allocation":allocation})))
}

fn roots(
    directory: &Path,
    entry: &PlannedBenchmark,
    replica: Identity,
    reference: &AttemptReference,
    checkpoint: &Path,
    index: usize,
) -> Result<Vec<Root>, Error> {
    let config = crate::runner::replica::configuration(
        &crate::sweep::isolated_config(&entry.configuration, directory),
        replica,
    )?;
    let workspace = PathBuf::from(config.workspace);
    let mut roots = vec![
        Root::path("configuration", directory.join("benchmark.toml")),
        Root::path(
            "start",
            checkpoint
                .join("started")
                .join(format!("{index:06}/{:03}.json", replica.index)),
        ),
        Root::path(
            "allocation",
            checkpoint
                .join("allocated")
                .join(format!("{index:06}/{:03}.json", replica.index)),
        ),
        Root::path("workload-evidence", reference.artifacts.clone()),
        Root::path("runtime", crate::runner::replica::runtime_path(&workspace)),
        Root::path("workspace", workspace),
        Root::path("log", PathBuf::from(config.log_file)),
    ];
    if let Some(settings) = config.measurements.filter(|settings| settings.enabled) {
        roots.push(Root::path(
            "partial-measurements-not-execution-evidence",
            PathBuf::from(settings.directory),
        ));
    }
    Ok(roots)
}

fn context(index: usize, replica: Identity, entry: &PlannedBenchmark, digest: &str) -> Value {
    json!({"scope":SCOPE,"configuration":index,"replica":replica,"entry":entry,"input_digest":digest})
}

fn summary(entry: &PlannedBenchmark, receipt: &Receipt) -> Value {
    let mut value = crate::sweep::recovered::reporting_summary(entry, &receipt.report);
    value["reconciled"] = json!(true);
    value["recovered_grade_only"] = json!(true);
    value["replica_index"] = Value::Null;
    value["execution_cohort"] = Value::Null;
    value["closed_slot"] = json!(format!(
        "{}/{}",
        receipt.replica.index, receipt.replica.total
    ));
    value
}

fn claim(attempts: &mut BTreeSet<String>, summary: &Value) -> Result<(), Error> {
    let id = summary["task"]["attempt_id"]
        .as_str()
        .filter(|id| !id.is_empty() && id.len() <= 256)
        .ok_or_else(|| invalid("closed slot has no actual task identity"))?;
    if !attempts.insert(id.to_owned()) {
        return Err(invalid(
            "an actual attempt cannot close multiple selected slots",
        ));
    }
    Ok(())
}

fn validate_record(
    checkpoint: &Path,
    directory: &Path,
    index: usize,
    entry: &PlannedBenchmark,
    slot: usize,
    inspect: &impl Fn(&AttemptReference) -> Result<Value, Error>,
) -> Result<Receipt, Error> {
    let record: Receipt = read_private(&path(checkpoint, index, slot))?;
    let replica = Identity {
        index: slot,
        total: entry.configuration.concurrency,
    };
    let (reference, binding) = binding(checkpoint, index, replica, directory, entry)?;
    if record.schema_version != 1
        || record.scope != SCOPE
        || record.configuration != index
        || record.replica != replica
        || record.reference != reference
        || record.binding != binding
        || binding["intent"]["input_digest"].as_str() != Some(record.input_digest.as_str())
        || record.report != inspect(&reference)?
    {
        return Err(invalid("closed slot binding or official grade changed"));
    }
    Ok(record)
}

pub(in crate::sweep) fn reporting(
    checkpoint: &Path,
    directory: &Path,
    index: usize,
    entry: &PlannedBenchmark,
) -> Result<BTreeMap<usize, (PathBuf, Value)>, Error> {
    let mut values = BTreeMap::new();
    for slot in 1..=entry.configuration.concurrency {
        let file = path(checkpoint, index, slot);
        match std::fs::symlink_metadata(&file) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(io_error(error)),
            Ok(_) => (),
        }
        let receipt = validate_record(
            checkpoint,
            directory,
            index,
            entry,
            slot,
            &inspect_reference,
        )?;
        values.insert(slot, (file, summary(entry, &receipt)));
    }
    Ok(values)
}

impl Inputs {
    pub(super) fn closed_slot(
        &self,
        index: usize,
        replica: Identity,
        directory: &Path,
        entry: &PlannedBenchmark,
    ) -> Result<Option<Value>, Error> {
        self.closed_slot_with(index, replica, directory, entry, &inspect_reference)
    }

    fn closed_slot_with(
        &self,
        index: usize,
        replica: Identity,
        directory: &Path,
        entry: &PlannedBenchmark,
        inspect: &impl Fn(&AttemptReference) -> Result<Value, Error>,
    ) -> Result<Option<Value>, Error> {
        match std::fs::symlink_metadata(path(&self.directory, index, replica.index)) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(io_error(error)),
            Ok(_) => (),
        }
        let record = validate_record(
            &self.directory,
            directory,
            index,
            entry,
            replica.index,
            inspect,
        )?;
        let roots = roots(
            directory,
            entry,
            replica,
            &record.reference,
            &self.directory,
            index,
        )?;
        if record.input_digest != self.initial.digest
            || record.roots != roots
            || self.snapshotter.fingerprint(
                &roots,
                &context(index, replica, entry, &self.initial.digest),
                false,
                &[],
            )? != record.fingerprint
        {
            return Err(invalid(
                "closed slot preserved artifacts or declared inputs changed",
            ));
        }
        Ok(Some(summary(entry, &record)))
    }

    pub(super) fn prepare_closed_slots(
        &self,
        parent: &Path,
        results: &Path,
        plan: &crate::sweep::plan::SweepPlan,
    ) -> Result<Vec<Pending>, Error> {
        self.prepare_closed_slots_with(parent, results, plan, &inspect_reference)
    }

    fn prepare_closed_slots_with(
        &self,
        parent: &Path,
        results: &Path,
        plan: &crate::sweep::plan::SweepPlan,
        inspect: &impl Fn(&AttemptReference) -> Result<Value, Error>,
    ) -> Result<Vec<Pending>, Error> {
        let mut pending = Vec::new();
        let mut attempts = BTreeSet::new();
        for (index, entry) in plan.entries.iter().enumerate() {
            let directory = std::path::absolute(results.join(
                crate::sweep::execution::result_name(index, &parent.join(&entry.reference)),
            ))
            .map_err(io_error)?;
            if !directory.exists() {
                continue;
            }
            if std::fs::symlink_metadata(self.completion_path(index)).is_ok() {
                let (_, summaries) = self.verify_completed(index, &directory, entry)?;
                for summary in &summaries {
                    claim(&mut attempts, summary)?;
                }
                continue;
            }
            let outcomes =
                crate::compare::outcomes::load(&directory, entry.configuration.concurrency)?;
            for outcome in outcomes.values() {
                claim(&mut attempts, &outcome.summary)?;
            }
            // Reporting-only imports never grant continuation, but malformed
            // or changed imports must not be hidden by a new closure record.
            crate::sweep::recovered::load(&self.directory, &directory, index, entry)?;
            for slot in 1..=entry.configuration.concurrency {
                let replica = Identity {
                    index: slot,
                    total: entry.configuration.concurrency,
                };
                if let Some(summary) =
                    self.closed_slot_with(index, replica, &directory, entry, inspect)?
                {
                    if self.has_replica(index, slot)? || outcomes.contains_key(&slot) {
                        return Err(invalid(
                            "closed grade-only slot conflicts with an execution outcome",
                        ));
                    }
                    claim(&mut attempts, &summary)?;
                    continue;
                }
                if self.has_replica(index, slot)? || outcomes.contains_key(&slot) {
                    continue;
                }
                if !self.replica_started(index, slot)? {
                    continue;
                }
                let (reference, binding) =
                    binding(&self.directory, index, replica, &directory, entry)?;
                if binding["intent"]["input_digest"] != self.initial.digest {
                    return Err(invalid("interrupted slot has different declared inputs"));
                }
                let report = inspect(&reference)?;
                claim(&mut attempts, &report)?;
                let roots = roots(
                    &directory,
                    entry,
                    replica,
                    &reference,
                    &self.directory,
                    index,
                )?;
                let context = context(index, replica, entry, &self.initial.digest);
                let fingerprint = self.snapshotter.fingerprint(&roots, &context, false, &[])?;
                if inspect(&reference)? != report {
                    return Err(invalid("official grade changed during slot recovery"));
                }
                pending.push(Pending {
                    path: path(&self.directory, index, slot),
                    context,
                    receipt: Receipt {
                        schema_version: 1,
                        scope: SCOPE.into(),
                        configuration: index,
                        replica,
                        input_digest: self.initial.digest.clone(),
                        reference,
                        binding,
                        report,
                        roots,
                        fingerprint,
                    },
                });
            }
        }
        Ok(pending)
    }

    pub(super) fn publish_closed_slots(&self, pending: Vec<Pending>) -> Result<(), Error> {
        self.publish_closed_slots_with(pending, &inspect_reference)
    }

    fn publish_closed_slots_with(
        &self,
        pending: Vec<Pending>,
        inspect: &impl Fn(&AttemptReference) -> Result<Value, Error>,
    ) -> Result<(), Error> {
        for candidate in &pending {
            if self.snapshotter.fingerprint(
                &candidate.receipt.roots,
                &candidate.context,
                false,
                &[],
            )? != candidate.receipt.fingerprint
                || inspect(&candidate.receipt.reference)? != candidate.receipt.report
            {
                return Err(invalid("closed slot evidence changed before publication"));
            }
        }
        for candidate in pending {
            publish(&candidate.path, &candidate.receipt)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::reconcile::tests::retained_fixture;
    use super::*;

    #[test]
    fn a_closed_slot_allows_only_a_positively_reserved_unstarted_sibling() {
        let (fixture, results, plan, reports) = unsealed();
        let inspect = |reference: &AttemptReference| Ok(reports[&reference.preparation_id].clone());
        let pending = fixture
            .inputs
            .prepare_closed_slots_with(&fixture.root, &results, &plan, &inspect)
            .unwrap();
        fixture
            .inputs
            .publish_closed_slots_with(pending, &inspect)
            .unwrap();
        // Discard only synthetic slot 2's start/output metadata to model an
        // untouched positively reserved sibling. Slot 1's receipt is untouched.
        for target in [
            path(&fixture.inputs.directory, 0, 2),
            replicas::start_path(&fixture.inputs, 0, 2),
            allocations::path(&fixture.inputs, 0, 2),
            fixture.directory.join("run-concurrency-2.log"),
        ] {
            std::fs::remove_file(target).unwrap();
        }
        std::fs::remove_dir(fixture.directory.join("workspace-concurrency-2")).unwrap();
        let observer = replicas::Observer {
            inputs: &fixture.inputs,
            index: 0,
            directory: &fixture.directory,
            entry: &fixture.entry,
            publication: std::sync::Mutex::new(()),
        };
        observer.reserve().unwrap();
        let closed = |replica| {
            fixture.inputs.closed_slot_with(
                0,
                replica,
                &fixture.directory,
                &fixture.entry,
                &inspect,
            )
        };
        let summaries = fixture
            .inputs
            .partial_replicas_with(0, &fixture.directory, &fixture.entry, closed)
            .unwrap();
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[&1]["workload_success"], false);
        // Merely acquiring outputs is enough to prevent silent reuse of slot 2.
        std::fs::write(
            fixture.directory.join("run-concurrency-2.log"),
            "possibly started",
        )
        .unwrap();
        assert!(
            fixture
                .inputs
                .partial_replicas_with(0, &fixture.directory, &fixture.entry, closed)
                .is_err()
        );
    }

    fn unsealed() -> (
        super::super::tests::Fixture,
        PathBuf,
        crate::sweep::plan::SweepPlan,
        BTreeMap<String, Value>,
    ) {
        let (fixture, results, plan) = retained_fixture();
        let mut reports = BTreeMap::new();
        for slot in 1..=2 {
            let outcome = fixture
                .directory
                .join(format!(".asb-completions/{}.json", slot - 1));
            let mut summary: Value = read_private::<Value>(&outcome).unwrap()["summary"].clone();
            summary.as_object_mut().unwrap().remove("replica_index");
            summary.as_object_mut().unwrap().remove("execution_cohort");
            summary["process_success"] = Value::Null;
            summary["elapsed_ms"] = Value::Null;
            summary["steps"] = Value::Null;
            summary["token_usage"] = Value::Null;
            summary["workload_success"] = json!(slot == 2);
            reports.insert(format!("abc-{slot}"), summary);
            // Controlled loss of only these fixture's ordinary run records.
            std::fs::remove_file(outcome).unwrap();
        }
        (fixture, results, plan, reports)
    }

    #[test]
    fn closed_slots_preserve_both_grades_and_unknown_execution_without_replacement() {
        let (fixture, results, plan, reports) = unsealed();
        let inspect = |reference: &AttemptReference| Ok(reports[&reference.preparation_id].clone());
        let pending = fixture
            .inputs
            .prepare_closed_slots_with(&fixture.root, &results, &plan, &inspect)
            .unwrap();
        assert_eq!(pending.len(), 2);
        assert!(!path(&fixture.inputs.directory, 0, 1).exists());
        fixture
            .inputs
            .publish_closed_slots_with(pending, &inspect)
            .unwrap();
        let original = std::fs::read(path(&fixture.inputs.directory, 0, 1)).unwrap();
        for slot in 1..=2 {
            let replica = Identity {
                index: slot,
                total: 2,
            };
            let summary = fixture
                .inputs
                .closed_slot_with(0, replica, &fixture.directory, &fixture.entry, &inspect)
                .unwrap()
                .unwrap();
            assert_eq!(summary["workload_success"], slot == 2);
            assert_eq!(summary["recovered_grade_only"], true);
            assert_eq!(summary["reconciled"], true);
            for field in [
                "process_success",
                "elapsed_ms",
                "steps",
                "token_usage",
                "execution_cohort",
                "replica_index",
            ] {
                assert!(summary[field].is_null(), "{field}");
            }
            assert!(!fixture.inputs.has_replica(0, slot).unwrap());
        }
        assert!(!fixture.inputs.completion_path(0).exists());
        assert!(
            fixture
                .inputs
                .prepare_closed_slots_with(&fixture.root, &results, &plan, &inspect)
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            original,
            std::fs::read(path(&fixture.inputs.directory, 0, 1)).unwrap()
        );
    }

    #[test]
    fn late_unknown_grade_or_duplicate_attempt_publishes_nothing() {
        for duplicate in [false, true] {
            let (fixture, results, plan, mut reports) = unsealed();
            if duplicate {
                reports.get_mut("abc-2").unwrap()["task"] = reports["abc-1"]["task"].clone();
            }
            let inspect = |reference: &AttemptReference| {
                if !duplicate && reference.preparation_id == "abc-2" {
                    Err(invalid("fixture grade is unknown"))
                } else {
                    Ok(reports[&reference.preparation_id].clone())
                }
            };
            assert!(
                fixture
                    .inputs
                    .prepare_closed_slots_with(&fixture.root, &results, &plan, &inspect)
                    .is_err()
            );
            assert!(!fixture.inputs.directory.join("closed-slots").exists());
        }
    }

    #[test]
    fn publication_rechecks_all_candidates_before_writing_any_receipt() {
        let (fixture, results, plan, reports) = unsealed();
        let inspect = |reference: &AttemptReference| Ok(reports[&reference.preparation_id].clone());
        let pending = fixture
            .inputs
            .prepare_closed_slots_with(&fixture.root, &results, &plan, &inspect)
            .unwrap();
        std::fs::write(
            fixture.directory.join("run-concurrency-2.log"),
            "changed after preparation",
        )
        .unwrap();
        assert!(
            fixture
                .inputs
                .publish_closed_slots_with(pending, &inspect)
                .is_err()
        );
        assert!(!fixture.inputs.directory.join("closed-slots").exists());
    }

    #[test]
    fn reporting_keeps_a_grade_but_resume_rejects_changed_partial_outputs() {
        let (fixture, results, plan, reports) = unsealed();
        let inspect = |reference: &AttemptReference| Ok(reports[&reference.preparation_id].clone());
        let pending = fixture
            .inputs
            .prepare_closed_slots_with(&fixture.root, &results, &plan, &inspect)
            .unwrap();
        fixture
            .inputs
            .publish_closed_slots_with(pending, &inspect)
            .unwrap();
        std::fs::write(
            fixture.directory.join("run-concurrency-1.log"),
            "edited partial output",
        )
        .unwrap();
        assert!(
            validate_record(
                &fixture.inputs.directory,
                &fixture.directory,
                0,
                &fixture.entry,
                1,
                &inspect
            )
            .is_ok()
        );
        assert!(
            fixture
                .inputs
                .closed_slot_with(
                    0,
                    Identity { index: 1, total: 2 },
                    &fixture.directory,
                    &fixture.entry,
                    &inspect
                )
                .is_err()
        );
        // Unrelated sibling output is not part of slot 2's preservation seal.
        fixture
            .inputs
            .closed_slot_with(
                0,
                Identity { index: 2, total: 2 },
                &fixture.directory,
                &fixture.entry,
                &inspect,
            )
            .unwrap()
            .unwrap();
    }

    #[test]
    fn changed_grade_and_original_intent_cannot_reuse_a_closure() {
        let (fixture, results, plan, reports) = unsealed();
        let inspect = |reference: &AttemptReference| Ok(reports[&reference.preparation_id].clone());
        let pending = fixture
            .inputs
            .prepare_closed_slots_with(&fixture.root, &results, &plan, &inspect)
            .unwrap();
        fixture
            .inputs
            .publish_closed_slots_with(pending, &inspect)
            .unwrap();
        let changed = |reference: &AttemptReference| {
            let mut report = inspect(reference)?;
            report["workload_success"] = json!(true);
            Ok(report)
        };
        assert!(
            validate_record(
                &fixture.inputs.directory,
                &fixture.directory,
                0,
                &fixture.entry,
                1,
                &changed
            )
            .is_err()
        );
        let intent = replicas::start_path(&fixture.inputs, 0, 1);
        let mut value: Value = read_private(&intent).unwrap();
        value["intent_id"] = json!("changed");
        std::fs::write(intent, serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(
            validate_record(
                &fixture.inputs.directory,
                &fixture.directory,
                0,
                &fixture.entry,
                1,
                &inspect
            )
            .is_err()
        );
    }
}
