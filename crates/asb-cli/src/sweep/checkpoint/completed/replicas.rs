//! Immutable evidence for a single replica, independent of unfinished siblings.
use super::*;
use crate::runner::replica::{CompletionObserver, Identity};
use std::sync::Mutex;

const REPLICA_SCOPE: &str = "completed-replica-declared-evidence";

pub(in crate::sweep) struct Observer<'a> {
    pub inputs: &'a Inputs,
    pub index: usize,
    pub directory: &'a Path,
    pub entry: &'a PlannedBenchmark,
    pub publication: Mutex<()>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    schema_version: u32,
    scope: String,
    configuration: usize,
    replica: Identity,
    input_digest: String,
    outcome: PathBuf,
    roots: Vec<Root>,
    fingerprint: Fingerprint,
}

impl CompletionObserver for Observer<'_> {
    fn completed(&self, replica: Identity, record: &Path) -> Result<(), Error> {
        // Do not launch a hasher per simultaneously exiting sibling. The
        // execution itself has already ended; only evidence publication waits.
        let _publication = self
            .publication
            .lock()
            .map_err(|_| invalid("replica evidence publication failed"))?;
        let outcome = std::path::absolute(record).map_err(io_error)?;
        let (roots, _) = state(self.directory, self.entry, replica, &outcome)?;
        let fingerprint = self.inputs.snapshotter.fingerprint(
            &roots,
            &context(self.index, replica, self.entry, &self.inputs.initial.digest),
            false,
            &[],
        )?;
        publish(
            &path(self.inputs, self.index, replica.index),
            &Receipt {
                schema_version: 1,
                scope: REPLICA_SCOPE.into(),
                configuration: self.index,
                replica,
                input_digest: self.inputs.initial.digest.clone(),
                outcome,
                roots,
                fingerprint,
            },
        )
    }
}

impl Inputs {
    pub(in crate::sweep) fn has_replica(
        &self,
        index: usize,
        replica: usize,
    ) -> Result<bool, Error> {
        match std::fs::symlink_metadata(path(self, index, replica)) {
            Ok(_) => Ok(true),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(io_error(error)),
        }
    }
    pub(in crate::sweep) fn verify_replica(
        &self,
        index: usize,
        replica: Identity,
        directory: &Path,
        entry: &PlannedBenchmark,
    ) -> Result<Value, Error> {
        let receipt: Receipt = read_private(&path(self, index, replica.index))?;
        let (roots, summary) = state(directory, entry, replica, &receipt.outcome)?;
        if receipt.schema_version != 1
            || receipt.scope != REPLICA_SCOPE
            || receipt.configuration != index
            || receipt.replica != replica
            || receipt.input_digest != self.initial.digest
            || !receipt.fingerprint.valid()
            || receipt.roots != roots
        {
            return Err(invalid(
                "replica receipt does not match its selected slot or evidence",
            ));
        }
        let actual = self.snapshotter.fingerprint(
            &roots,
            &context(index, replica, entry, &self.initial.digest),
            false,
            &[],
        )?;
        if actual != receipt.fingerprint {
            return Err(invalid(
                "completed replica outputs or evaluator evidence changed",
            ));
        }
        Ok(summary)
    }
}

fn path(inputs: &Inputs, index: usize, replica: usize) -> PathBuf {
    inputs
        .directory
        .join("replicas")
        .join(format!("{index:06}"))
        .join(format!("{replica:03}.json"))
}

fn context(index: usize, replica: Identity, entry: &PlannedBenchmark, digest: &str) -> Value {
    json!({"protocol":"asb-replica-evidence-v1","scope":REPLICA_SCOPE,
        "configuration":index,"replica":replica,"entry":entry,"input_digest":digest})
}

fn state(
    directory: &Path,
    entry: &PlannedBenchmark,
    replica: Identity,
    record: &Path,
) -> Result<(Vec<Root>, Value), Error> {
    let directory = std::path::absolute(directory).map_err(io_error)?;
    if !replica.valid()
        || replica.total != entry.configuration.concurrency
        || record.parent() != Some(directory.join(".asb-completions").as_path())
    {
        return Err(invalid(
            "replica outcome is outside the selected configuration",
        ));
    }
    let outcome: Outcome = read_private(record)?;
    validate_outcome(&outcome, entry)?;
    if outcome.schema_version != 2 || outcome.replica != Some(replica) || outcome.outputs.is_empty()
    {
        return Err(invalid(
            "replica outcome has no stable slot or execution outputs",
        ));
    }
    let mut roots = vec![Root::path("replica-outcome", record.to_path_buf())];
    roots.push(Root::path(
        "configuration",
        directory.join("benchmark.toml"),
    ));
    let mut outputs = BTreeSet::new();
    for output in &outcome.outputs {
        if output == &directory
            || !output.starts_with(&directory)
            || output
                .components()
                .any(|part| matches!(part, std::path::Component::ParentDir))
            || !outputs.insert(output.clone())
        {
            return Err(invalid(
                "replica execution outputs are shared, duplicated or outside its configuration",
            ));
        }
    }
    let isolated = crate::sweep::isolated_config(&entry.configuration, &directory);
    let config = crate::runner::replica::configuration(&isolated, replica)?;
    let workspace = PathBuf::from(&config.workspace);
    let runtime = crate::runner::replica::runtime_path(&workspace);
    let mut expected = BTreeSet::from([workspace, PathBuf::from(&config.log_file)]);
    if std::fs::symlink_metadata(&runtime).is_ok() {
        expected.insert(runtime);
    }
    if let Some(settings) = config
        .measurements
        .as_ref()
        .filter(|settings| settings.enabled)
    {
        expected.insert(PathBuf::from(&settings.directory));
    }
    if outputs != expected {
        return Err(invalid(
            "replica outputs do not match its selected workspace, log, runtime and measurements",
        ));
    }
    if entry
        .configuration
        .measurements
        .as_ref()
        .is_some_and(|settings| settings.enabled)
    {
        let name = if replica.total == 1 {
            "measurements".into()
        } else {
            format!("measurements-concurrency-{}", replica.index)
        };
        let measurements = directory.join(name);
        if !outputs.contains(&measurements)
            || !measurement_matches(
                &read_private(&measurements.join("run.json"))?,
                &outcome.summary,
            )
        {
            return Err(invalid(
                "replica measurement does not match its slot's graded outcome",
            ));
        }
    }
    for (index, output) in outputs.into_iter().enumerate() {
        roots.push(Root::path(format!("replica-output-{index}"), output));
    }
    let evidence: BTreeSet<_> = outcome
        .workload_evidence
        .into_iter()
        .chain(outcome.proxy_evidence)
        .collect();
    for (index, output) in evidence.into_iter().enumerate() {
        roots.push(Root::path(format!("replica-evidence-{index}"), output));
    }
    Ok((roots, outcome.summary))
}

#[cfg(test)]
mod tests {
    use super::super::tests::Fixture;
    use super::*;

    fn record(fixture: &Fixture, replica: Identity) -> PathBuf {
        let path = fixture.directory.join(".asb-completions/0.json");
        let mut record: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        let isolated =
            crate::sweep::isolated_config(&fixture.entry.configuration, &fixture.directory);
        let config = crate::runner::replica::configuration(&isolated, replica).unwrap();
        let workspace = PathBuf::from(config.workspace);
        std::fs::create_dir_all(&workspace).unwrap();
        std::fs::write(&config.log_file, "private log sentinel").unwrap();
        record["schema_version"] = 2.into();
        record["replica"] = serde_json::to_value(replica).unwrap();
        record["outputs"] = json!([workspace, config.log_file]);
        std::fs::write(&path, serde_json::to_vec(&record).unwrap()).unwrap();
        path
    }

    #[test]
    fn a_false_graded_replica_is_sealed_before_its_sibling_and_is_independently_verified() {
        let fixture = Fixture::new(false);
        let identity = Identity { index: 1, total: 2 };
        let record = record(&fixture, identity);
        std::fs::remove_file(fixture.directory.join(".asb-completions/1.json")).unwrap();
        let observer = Observer {
            inputs: &fixture.inputs,
            index: 0,
            directory: &fixture.directory,
            entry: &fixture.entry,
            publication: Mutex::new(()),
        };
        observer.completed(identity, &record).unwrap();
        assert!(!fixture.inputs.has_replica(0, 2).unwrap());
        let verified = fixture
            .inputs
            .verify_replica(0, identity, &fixture.directory, &fixture.entry)
            .unwrap();
        assert_eq!(verified["workload_success"], false);
        assert!(
            fixture
                .inputs
                .completed(0, &fixture.directory, &fixture.entry)
                .is_err(),
            "the sibling group is not complete"
        );
        let receipt = std::fs::read(path(&fixture.inputs, 0, 1)).unwrap();
        let text = String::from_utf8(receipt.clone()).unwrap();
        assert!(!text.contains("private log sentinel"));
        assert!(!text.contains("private evaluator sentinel"));
        // A running sibling can produce its own outputs without invalidating
        // the already graded replica's immutable evidence.
        std::fs::create_dir(fixture.directory.join("workspace-concurrency-2")).unwrap();
        std::fs::write(
            fixture.directory.join("run-concurrency-2.log"),
            "sibling still working",
        )
        .unwrap();
        fixture
            .inputs
            .verify_replica(0, identity, &fixture.directory, &fixture.entry)
            .unwrap();
        assert!(
            observer.completed(identity, &record).is_err(),
            "never replace a prior replica receipt"
        );
        assert_eq!(std::fs::read(path(&fixture.inputs, 0, 1)).unwrap(), receipt);
        std::fs::write(
            fixture.directory.join("run-concurrency-1.log"),
            "edited evidence",
        )
        .unwrap();
        assert!(
            fixture
                .inputs
                .verify_replica(0, identity, &fixture.directory, &fixture.entry)
                .is_err()
        );
    }

    #[test]
    fn replica_identity_output_omissions_and_cleanup_mismatches_cannot_be_sealed() {
        for case in ["slot", "total", "outputs", "cleanup", "outside"] {
            let fixture = Fixture::new(true);
            let identity = Identity { index: 1, total: 2 };
            let path = record(&fixture, identity);
            let mut record: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
            match case {
                "slot" => record["replica"]["index"] = 2.into(),
                "total" => record["replica"]["total"] = 3.into(),
                "outputs" => {
                    record["outputs"].as_array_mut().unwrap().pop();
                }
                "cleanup" => record["workload_cleanup_confirmed"] = false.into(),
                _ => record["outputs"][0] = json!(fixture.root),
            }
            std::fs::write(&path, serde_json::to_vec(&record).unwrap()).unwrap();
            let observer = Observer {
                inputs: &fixture.inputs,
                index: 0,
                directory: &fixture.directory,
                entry: &fixture.entry,
                publication: Mutex::new(()),
            };
            assert!(observer.completed(identity, &path).is_err(), "{case}");
            assert!(!fixture.inputs.has_replica(0, 1).unwrap());
        }
    }
}
