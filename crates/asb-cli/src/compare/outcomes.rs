//! Report actual outcomes independently of optional runtime measurements.
//!
//! These are recorded results, not verification of a resume seal or historical
//! attestation. Cleanup failures do not erase a separately confirmed task grade.
use super::*;
use crate::runner::replica::Identity;
use std::io::Read;

const LIMIT: u64 = 1024 * 1024;

pub(crate) struct Recorded {
    pub path: PathBuf,
    pub summary: Value,
    workload_cleanup: bool,
    proxy_cleanup: bool,
    pub source: &'static str,
}

impl Recorded {
    pub fn check_measurement(&self, measured: &Value) -> Result<(), Error> {
        if matches!(
            self.source,
            "imported-official-grade" | "recovered-official-grade"
        ) {
            return Err(invalid(
                &self.path,
                "a grade-only result and a measurement claim the same slot; preserve both and inspect the conflict",
            ));
        }
        for field in [
            "schema_version",
            "replica_index",
            "execution_cohort",
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
        ] {
            if self.summary[field] != measured[field] {
                return Err(invalid(
                    &self.path,
                    &format!(
                        "task outcome and measurement disagree on `{field}` ({}); preserve both records and inspect the attempt",
                        field.replace('_', " ")
                    ),
                ));
            }
        }
        Ok(())
    }

    pub fn status(&self) -> String {
        if self.source == "recovered-official-grade" {
            return format!(
                "recovered official task {}; execution timing, tokens, steps and process status unknown; partial execution artifacts preserved, not used as final measurements",
                if self.summary["workload_success"] == true {
                    "resolved"
                } else {
                    "unresolved"
                }
            );
        }
        if self.source == "imported-official-grade" {
            return format!(
                "imported official task {}; execution timing, tokens, steps and process status unknown; recorded cleanup: {}; not execution recovery or resume evidence",
                if self.summary["workload_success"] == true {
                    "resolved"
                } else {
                    "unresolved"
                },
                self.summary["recorded_cleanup"]
                    .as_str()
                    .unwrap_or("unknown")
            );
        }
        let kind = crate::workloads::scored_kind(
            self.summary["workload_evaluation"]
                .as_str()
                .unwrap_or("unknown"),
            &crate::workloads::BaselineEvidence::from_record(&self.summary),
        );
        let grade = if self.summary["workload_evaluation"] == "adapter-smoke" {
            "adapter smoke only; upstream task resolution not measured"
        } else if !crate::workloads::resolution_eligible(kind) {
            "recorded task unscored; grading scope or baseline is not eligible"
        } else {
            match self.summary["workload_success"].as_bool() {
                Some(true) => "recorded task resolved",
                Some(false) => "recorded task unresolved",
                None => "recorded task evaluation unavailable",
            }
        };
        let source = if self.source == "run-log" {
            "latest run log"
        } else {
            "task outcome"
        };
        let mut status = format!("{grade}; optional measurements unavailable (using {source})");
        if let Some(reason) = self.summary["failure_reason"].as_str() {
            status.push_str(&format!("; run failure: {reason}"));
        } else if self.summary["process_success"] == false {
            status.push_str("; agent process failed");
        }
        if let Some(reason) = self.summary["workload_failure"]["reason"].as_str() {
            status.push_str(&format!("; evaluator: {reason}"));
        }
        if !self.workload_cleanup || !self.proxy_cleanup {
            status.push_str("; owned-resource cleanup not confirmed");
        }
        status
    }
}

pub(crate) fn load(directory: &Path, total: usize) -> Result<BTreeMap<usize, Recorded>, Error> {
    let directory = directory.join(".asb-completions");
    match fs::symlink_metadata(&directory) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(error) => {
            return Err(invalid(
                &directory,
                &format!("cannot inspect outcomes: {error}"),
            ));
        }
        Ok(metadata) if !metadata.is_dir() => {
            return Err(invalid(&directory, "outcomes must be a real directory"));
        }
        Ok(_) => (),
    }
    let mut outcomes = BTreeMap::new();
    let mut files = Vec::new();
    for entry in fs::read_dir(&directory).map_err(crate::io_error)? {
        if files.len() >= 2 * crate::benchmark::MAX_CONCURRENCY {
            return Err(invalid(
                &directory,
                "outcome file population exceeds its limit",
            ));
        }
        files.push(entry.map_err(crate::io_error)?.path());
    }
    // Deterministic validation diagnostics, never a replica mapping. Actual
    // slots still come from explicit identity or a unique measured attempt.
    files.sort();
    for path in files {
        // A stopped writer can leave an unpublished temporary record. It earns
        // no grade, but must not hide the completed results of other replicas.
        if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with('.') && name.ends_with(".tmp"))
        {
            continue;
        }
        if path.extension().is_none_or(|extension| extension != "json") {
            return Err(invalid(&path, "unexpected file in outcome directory"));
        }
        let raw = bounded_record(&path)?;
        let summary = &raw["summary"];
        // Original schema-1 records predate proxy cleanup evidence. Absence is
        // unknown cleanup, not a fabricated confirmation or an erased grade.
        let proxy_cleanup = match raw.get("proxy_cleanup_confirmed") {
            None if raw["schema_version"] == 1 => Some(false),
            value => value.and_then(Value::as_bool),
        };
        let replica = if raw["schema_version"] == 2 {
            serde_json::from_value::<Identity>(raw["replica"].clone())
                .map_err(|_| invalid(&path, "outcome replica identity is malformed"))?
        } else if raw["schema_version"] == 1 {
            legacy_replica(
                directory.parent().expect("outcomes have a parent"),
                &path,
                summary,
                total,
            )?
        } else {
            return Err(invalid(
                &path,
                "outcome schema or legacy replica mapping is unsupported",
            ));
        };
        if !replica.valid()
            || replica.total != total
            || raw["scope"] != "task-outcome-not-yet-a-resume-receipt"
            || raw["workload_cleanup_confirmed"].as_bool().is_none()
            || proxy_cleanup.is_none()
            || !summary.is_object()
            || summary["schema_version"] != 2
            || summary["process_success"].as_bool().is_none()
            || (!summary["workload_success"].is_null()
                && summary["workload_success"].as_bool().is_none())
            || !summary["task"].is_object()
            || summary["replica_index"]
                .as_u64()
                .is_some_and(|index| index != replica.index as u64)
        {
            return Err(invalid(
                &path,
                "outcome identity or result fields are malformed",
            ));
        }
        if outcomes
            .insert(
                replica.index,
                Recorded {
                    path: path.clone(),
                    summary: summary.clone(),
                    workload_cleanup: raw["workload_cleanup_confirmed"] == true,
                    proxy_cleanup: proxy_cleanup == Some(true),
                    source: "task-outcome",
                },
            )
            .is_some()
        {
            return Err(invalid(
                &path,
                "multiple task outcomes claim the same replica slot",
            ));
        }
    }
    Ok(outcomes)
}

/// Ordinary runs append logs and may share an outcome directory with other
/// benchmarks. Select the last complete record of each configured replica log,
/// never arbitrary outcome filenames or another benchmark's measurements.
pub(crate) fn load_run(
    config_path: &Path,
    config: &crate::benchmark::BenchmarkConfig,
) -> Result<BTreeMap<usize, Recorded>, Error> {
    use std::io::{Seek, SeekFrom};
    let mut records = BTreeMap::new();
    for index in 1..=config.concurrency {
        let selected = crate::runner::replica::configuration(
            config,
            Identity {
                index,
                total: config.concurrency,
            },
        )?;
        let path = crate::runner::resolve_log_path(config_path, &selected.log_file);
        match fs::symlink_metadata(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(invalid(&path, &format!("cannot inspect run log: {error}"))),
            Ok(metadata) if !metadata.is_file() => {
                return Err(invalid(
                    &path,
                    "run log must be a regular file, not a symlink or pipe",
                ));
            }
            Ok(_) => (),
        }
        let mut file = match fs::File::open(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            result => result.map_err(|error| invalid(&path, &error.to_string()))?,
        };
        let size = file.metadata().map_err(crate::io_error)?.len();
        let offset = size.saturating_sub(super::MAX_BYTES);
        file.seek(SeekFrom::Start(offset))
            .map_err(crate::io_error)?;
        let mut tail = Vec::new();
        file.take(super::MAX_BYTES + 1)
            .read_to_end(&mut tail)
            .map_err(crate::io_error)?;
        if tail.len() as u64 > super::MAX_BYTES {
            return Err(invalid(
                &path,
                "log changed while reading; retry after the run finishes",
            ));
        }
        let line = tail
            .split(|byte| *byte == b'\n')
            .rfind(|line| !line.iter().all(u8::is_ascii_whitespace))
            .ok_or_else(|| invalid(&path, "run log is empty"))?;
        if offset > 0 && line.len() == tail.len() {
            return Err(invalid(&path, "latest run log record exceeds 16 MiB"));
        }
        let mut summary: Value = serde_json::from_slice(line)
            .map_err(|error| invalid(&path, &format!("invalid latest run log JSON: {error}")))?;
        if summary["schema_version"] != 2
            || !summary["task"].is_object()
            || summary["process_success"].as_bool().is_none()
            || (!summary["workload_success"].is_null()
                && summary["workload_success"].as_bool().is_none())
        {
            return Err(invalid(
                &path,
                "latest run log lacks a supported task summary",
            ));
        }
        // Logs include client transcripts; reports must never copy them.
        if let Some(object) = summary.as_object_mut() {
            object.remove("stdout");
            object.remove("stderr");
        }
        records.insert(
            index,
            Recorded {
                path,
                summary,
                workload_cleanup: false,
                proxy_cleanup: false,
                source: "run-log",
            },
        );
    }
    Ok(records)
}

pub(super) fn add_imports(
    outcomes: &mut BTreeMap<usize, Recorded>,
    imports: BTreeMap<usize, (PathBuf, Value)>,
) -> Result<(), Error> {
    add_retained(outcomes, imports, "imported-official-grade")
}

pub(super) fn add_closed(
    outcomes: &mut BTreeMap<usize, Recorded>,
    closed: BTreeMap<usize, (PathBuf, Value)>,
) -> Result<(), Error> {
    add_retained(outcomes, closed, "recovered-official-grade")
}

fn add_retained(
    outcomes: &mut BTreeMap<usize, Recorded>,
    imports: BTreeMap<usize, (PathBuf, Value)>,
    source: &'static str,
) -> Result<(), Error> {
    for (slot, (path, summary)) in imports {
        if outcomes.contains_key(&slot) {
            return Err(invalid(
                &path,
                "import conflicts with an existing task outcome; preserve both",
            ));
        }
        outcomes.insert(
            slot,
            Recorded {
                path,
                summary,
                workload_cleanup: false,
                proxy_cleanup: false,
                source,
            },
        );
    }
    Ok(())
}

fn legacy_replica(
    directory: &Path,
    path: &Path,
    summary: &Value,
    total: usize,
) -> Result<Identity, Error> {
    if total == 1 {
        return Ok(Identity::default());
    }
    if let Some(index) = summary["replica_index"]
        .as_u64()
        .and_then(|value| usize::try_from(value).ok())
    {
        return Ok(Identity { index, total });
    }
    // Old multi-replica outcomes did not declare their slot. Retain measured
    // history only when the actual attempt maps to exactly one measured slot;
    // directory iteration order or filenames are never evidence of a slot.
    let mut matching = None;
    for index in 1..=total {
        let measured = directory.join(format!("measurements-concurrency-{index}/run.json"));
        match fs::symlink_metadata(&measured) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => {
                return Err(invalid(
                    &measured,
                    &format!("cannot inspect legacy measurement: {error}"),
                ));
            }
            Ok(_) => (),
        }
        let record = bounded_record(&measured)?;
        if summary["task"].is_object()
            && record["task"] == summary["task"]
            && matching.replace(index).is_some()
        {
            return Err(invalid(
                path,
                "legacy attempt appears in multiple measured replica slots",
            ));
        }
    }
    matching.map(|index|Identity {index,total}).ok_or_else(||invalid(path,
        "legacy outcome has no unambiguous replica mapping; preserve it and its original measurements"))
}

fn bounded_record(path: &Path) -> Result<Value, Error> {
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    let file = options
        .open(path)
        .map_err(|error| invalid(path, &format!("cannot read outcome: {error}")))?;
    let metadata = file.metadata().map_err(crate::io_error)?;
    if !metadata.is_file() || metadata.len() > LIMIT {
        return Err(invalid(path, "outcome must be a bounded regular file"));
    }
    let mut bytes = Vec::new();
    file.take(LIMIT + 1)
        .read_to_end(&mut bytes)
        .map_err(crate::io_error)?;
    if bytes.len() as u64 > LIMIT {
        return Err(invalid(path, "outcome exceeded its size limit"));
    }
    serde_json::from_slice(&bytes).map_err(|_| invalid(path, "outcome JSON is malformed"))
}

fn invalid(path: &Path, reason: &str) -> Error {
    Error::Input(format!(
        "cannot compare task outcome {}: {reason}",
        path.display()
    ))
}

#[cfg(test)]
#[path = "../../tests/unit/outcome_logs.rs"]
mod log_boundary_tests;

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixture {
        root: PathBuf,
        total: usize,
        workload: String,
        dataset: Value,
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    impl Fixture {
        fn new(workload: &str, total: usize) -> Self {
            let id = crate::workloads::TaskIdentity::local("compare-outcome-test", None).attempt_id;
            let root = std::env::temp_dir().join(format!("asb-outcome-{id}"));
            fs::create_dir_all(root.join("sweep-results/000-one/.asb-completions")).unwrap();
            let text = format!(
                "agent='pi'\nprovider='fixture-provider'\nmodel='fixture-model'\nworkload='{workload}'\ntrial='original-task'\nconcurrency={total}\n"
            );
            fs::write(root.join("one.toml"), &text).unwrap();
            fs::write(root.join("sweep.toml"), "configs=['one.toml']\n").unwrap();
            let config: crate::benchmark::BenchmarkConfig = toml::from_str(&text).unwrap();
            let dataset = json!({"source":"original-fixture-dataset","revision":"a".repeat(40),"split":"test"});
            fs::write(
                root.join("sweep-results/plan.json"),
                json!({
                    "schema_version":1,"run_id":"a123","execution_mode":"replay",
                    "entries":[{"reference":"one.toml","configuration":config,
                    "selection":{"workload":workload,"task_id":"original-task","dataset":dataset,
                    "evaluation_kind":"upstream-harness","population_size":1}}]
                })
                .to_string(),
            )
            .unwrap();
            Self {
                root,
                total,
                workload: workload.into(),
                dataset,
            }
        }

        fn directory(&self) -> PathBuf {
            self.root.join("sweep-results/000-one")
        }

        fn record(&self, slot: usize, grade: Value, process: bool, cleanup: bool) -> Value {
            let baseline = if self.workload.starts_with("terminal-bench") {
                json!({"kind":"upstream-environment-definition","source_revision":"a".repeat(40),"definition_sha256":"b".repeat(64)})
            } else {
                json!({"kind":"upstream-task-definition","base_commit":"a".repeat(40)})
            };
            let summary = json!({
                "schema_version":2,"agent":"pi","provider":"fixture-provider","model":"fixture-model",
                "workload":self.workload,"replica_index":slot,
                "execution_cohort":{"id":"ab123","selected_replicas":self.total,"admitted_slots":(1..=self.total).collect::<Vec<_>>()},
                "task":{"workload":self.workload,"task_id":"original-task","dataset":self.dataset,"attempt_id":format!("ab123-{slot}")},
                "process_success":process,"workload_success":grade,
                "workload_evaluation":"upstream-harness",
                "workload_baseline":baseline,
                "workload_baseline_passed":null,
                "workload_failure":if grade.is_null(){json!({"kind":"evaluation-error","reason":"fixture grader timed out"})}else{Value::Null},
                "failure_kind":if process {Value::Null}else{json!("agent-exit")},
                "failure_reason":if process {Value::Null}else{json!("fixture agent exited nonzero")},
                "success_rate":if grade.is_null(){Value::Null}else{json!(if grade==true {1.0}else{0.0})},
                "elapsed_ms":57,"steps":3,"token_usage":{"total_tokens":23},"adapter_smoke_success":null
            });
            json!({"schema_version":2,"scope":"task-outcome-not-yet-a-resume-receipt",
                "replica":{"index":slot,"total":self.total},
                "workload_cleanup_confirmed":cleanup,"proxy_cleanup_confirmed":cleanup,
                "summary":summary,"workload_evidence":[],"proxy_evidence":[],"outputs":[]})
        }

        fn publish(&self, slot: usize, record: &Value) -> PathBuf {
            let path = self
                .directory()
                .join(format!(".asb-completions/{slot:03}.json"));
            fs::write(&path, record.to_string()).unwrap();
            path
        }

        fn compare(&self) -> Result<Value, Error> {
            super::super::run(
                self.root.join("sweep.toml").to_str().unwrap(),
                &mut Vec::new(),
            )?;
            read_json(&self.root.join("results/sweep-comparison/comparison.json"))
        }
    }

    #[test]
    fn every_original_adapter_keeps_unmeasured_grades_and_incomplete_population_honest() {
        for workload in [
            "swe-bench",
            "swe-bench-lite",
            "swe-bench-verified",
            "terminal-bench-1.0",
            "terminal-bench-2.1",
        ] {
            let fixture = Fixture::new(workload, 4);
            for (slot, grade, process, cleanup) in [
                (1, json!(true), false, false),
                (2, json!(false), true, true),
                (3, Value::Null, true, true),
            ] {
                fixture.publish(slot, &fixture.record(slot, grade, process, cleanup));
            }
            fs::write(
                fixture.directory().join(".asb-completions/.unfinished.tmp"),
                "incomplete record",
            )
            .unwrap();
            let report = fixture.compare().unwrap();
            let rows = report["benchmarks"].as_array().unwrap();
            assert_eq!(rows.len(), 4);
            for (slot, row) in rows.iter().enumerate().take(3) {
                assert_eq!(row["result_source"], "task-outcome");
                assert_eq!(row["replica_index"], slot + 1);
                assert_eq!(row["metrics"]["elapsed_ms"], 57.0);
                assert_eq!(row["metrics"]["token_usage.total_tokens"], 23.0);
                assert!(row["host_metadata"].is_null());
                assert!(row["components"].is_null());
                assert!(row["series"].as_object().unwrap().is_empty());
            }
            assert_eq!(rows[0]["metrics"]["success_rate"], 1.0);
            assert_eq!(rows[1]["metrics"]["success_rate"], 0.0);
            assert_eq!(rows[2]["metrics"]["success_rate"], 0.0);
            assert!(
                rows[0]["status"]
                    .as_str()
                    .unwrap()
                    .contains("cleanup not confirmed")
            );
            assert!(
                rows[0]["status"]
                    .as_str()
                    .unwrap()
                    .contains("fixture agent exited nonzero")
            );
            assert!(
                rows[2]["status"]
                    .as_str()
                    .unwrap()
                    .contains("fixture grader timed out")
            );
            assert_eq!(rows[3]["result_source"], "missing");
            assert!(rows[3]["task"].is_null());
            assert_eq!(report["evaluation_coverage"]["selected_attempts"], 4);
            assert_eq!(report["evaluation_coverage"]["evaluated_attempts"], 2);
            assert_eq!(report["evaluation_coverage"]["unevaluated_attempts"], 2);
            let group = report["resolution_groups"]
                .as_object()
                .unwrap()
                .values()
                .next()
                .unwrap();
            assert_eq!(group["task_count"], 4);
            assert_eq!(group["resolved_count"], 1);
            assert_eq!(group["success_rate"], 0.25);
            let population = report["population_coverage"]
                .as_object()
                .unwrap()
                .values()
                .next()
                .unwrap();
            assert!(
                !population["complete_population_evaluation"]
                    .as_bool()
                    .unwrap()
            );
            assert!(population["full_population_resolution_rate"].is_null());
            let html =
                fs::read_to_string(fixture.root.join("results/sweep-comparison/report.html"))
                    .unwrap();
            assert!(html.contains("optional measurements unavailable"));
            assert!(html.contains("metric-success_rate.svg"));
        }
    }

    #[test]
    fn retained_and_measured_results_must_agree_but_neither_changes_the_other() {
        let fixture = Fixture::new("swe-bench-lite", 1);
        let record = fixture.record(1, json!(true), true, true);
        let outcome = fixture.publish(1, &record);
        let directory = fixture.directory().join("measurements");
        fs::create_dir(&directory).unwrap();
        let measured = directory.join("run.json");
        let mut measurement = record["summary"].clone();
        measurement["status"] = json!("completed");
        measurement["success"] = json!(true);
        fs::write(&measured, measurement.to_string()).unwrap();
        assert_eq!(
            fixture.compare().unwrap()["benchmarks"][0]["result_source"],
            "measurements"
        );
        let before = fs::read(&outcome).unwrap();
        for field in [
            "workload_success",
            "task",
            "workload_baseline",
            "token_usage",
            "execution_cohort",
        ] {
            let mut changed = measurement.clone();
            changed[field] = json!("contradictory fixture");
            fs::write(&measured, changed.to_string()).unwrap();
            let error = fixture.compare().unwrap_err().to_string();
            assert!(error.contains(&format!("disagree on `{field}`")), "{error}");
            assert_eq!(fs::read(&outcome).unwrap(), before);
            assert_eq!(read_json(&measured).unwrap(), changed);
        }
        fs::remove_file(&measured).unwrap();
        assert_eq!(
            fixture.compare().unwrap()["benchmarks"][0]["result_source"],
            "task-outcome"
        );
    }

    #[test]
    fn complete_distinct_task_population_keeps_its_score_without_collectors() {
        for workload in [
            "swe-bench",
            "swe-bench-lite",
            "swe-bench-verified",
            "terminal-bench-1.0",
            "terminal-bench-2.1",
        ] {
            let fixture = Fixture::new(workload, 1);
            fixture.publish(1, &fixture.record(1, json!(false), true, true));
            let text = fs::read_to_string(fixture.root.join("one.toml"))
                .unwrap()
                .replace("original-task", "second-task");
            fs::write(fixture.root.join("two.toml"), &text).unwrap();
            fs::write(
                fixture.root.join("sweep.toml"),
                "configs=['one.toml','two.toml']\n",
            )
            .unwrap();
            let mut plan = read_json(&fixture.root.join("sweep-results/plan.json")).unwrap();
            plan["entries"][0]["selection"]["population_size"] = json!(2);
            let mut second = plan["entries"][0].clone();
            second["reference"] = json!("two.toml");
            second["configuration"] =
                json!(toml::from_str::<crate::benchmark::BenchmarkConfig>(&text).unwrap());
            second["selection"]["task_id"] = json!("second-task");
            plan["entries"].as_array_mut().unwrap().push(second);
            fs::write(
                fixture.root.join("sweep-results/plan.json"),
                plan.to_string(),
            )
            .unwrap();
            let directory = fixture.root.join("sweep-results/001-two/.asb-completions");
            fs::create_dir_all(&directory).unwrap();
            let mut second = fixture.record(1, json!(true), false, false);
            second["summary"]["task"]["task_id"] = json!("second-task");
            second["summary"]["task"]["attempt_id"] = json!("ab124-1");
            fs::write(directory.join("second.json"), second.to_string()).unwrap();
            let report = fixture.compare().unwrap();
            assert_eq!(report["evaluation_coverage"]["evaluated_attempts"], 2);
            let population = report["population_coverage"]
                .as_object()
                .unwrap()
                .values()
                .next()
                .unwrap();
            assert_eq!(population["selected_tasks"], 2);
            assert_eq!(population["resolved_tasks"], 1);
            assert_eq!(population["complete_population_evaluation"], true);
            assert_eq!(population["full_population_resolution_rate"], 0.5);
        }
    }

    #[test]
    fn fallback_rejects_mismatched_task_dataset_replica_and_duplicate_attempts() {
        let fixture = Fixture::new("swe-bench-lite", 2);
        let original = fixture.record(1, json!(true), true, true);
        for field in ["agent", "provider", "model", "workload"] {
            let mut changed = original.clone();
            changed["summary"][field] = json!("another-selection");
            fixture.publish(1, &changed);
            assert!(
                fixture
                    .compare()
                    .unwrap_err()
                    .to_string()
                    .contains("does not match selected")
            );
        }
        for mutate in 0..5 {
            let mut changed = original.clone();
            match mutate {
                0 => changed["summary"]["task"]["dataset"]["revision"] = json!("b".repeat(40)),
                1 => changed["summary"]["task"]["task_id"] = json!("another-task"),
                2 => changed["replica"]["total"] = json!(1),
                3 => changed["summary"]["execution_cohort"]["admitted_slots"] = json!([2]),
                _ => changed["summary"]["replica_index"] = json!(2),
            }
            fixture.publish(1, &changed);
            assert!(fixture.compare().is_err());
        }
        fixture.publish(1, &original);
        let mut second = fixture.record(2, json!(false), true, true);
        second["summary"]["task"]["attempt_id"] = original["summary"]["task"]["attempt_id"].clone();
        fixture.publish(2, &second);
        assert!(
            fixture
                .compare()
                .unwrap_err()
                .to_string()
                .contains("duplicate actual attempt")
        );
    }

    #[test]
    fn outcomes_are_bounded_regular_files_and_legacy_slots_must_be_unambiguous() {
        let fixture = Fixture::new("swe-bench-lite", 1);
        let original = fixture.record(1, json!(true), true, true);
        let path = fixture.publish(1, &original);
        let mut legacy = original.clone();
        legacy["schema_version"] = json!(1);
        legacy.as_object_mut().unwrap().remove("replica");
        legacy
            .as_object_mut()
            .unwrap()
            .remove("proxy_cleanup_confirmed");
        legacy["summary"]["replica_index"] = Value::Null;
        legacy["summary"]["execution_cohort"] = Value::Null;
        fixture.publish(1, &legacy);
        assert_eq!(load(&fixture.directory(), 1).unwrap().len(), 1);
        let report = fixture.compare().unwrap();
        assert_eq!(report["benchmarks"][0]["metrics"]["success_rate"], 1.0);
        assert!(
            report["benchmarks"][0]["status"]
                .as_str()
                .unwrap()
                .contains("cleanup not confirmed")
        );
        assert!(load(&fixture.directory(), 2).is_err());
        fixture.publish(1, &original);
        let duplicate = fixture.directory().join(".asb-completions/duplicate.json");
        fs::copy(&path, &duplicate).unwrap();
        assert!(load(&fixture.directory(), 1).is_err());
        fs::remove_file(&duplicate).unwrap();
        fs::write(&path, vec![b' '; LIMIT as usize + 1]).unwrap();
        assert!(load(&fixture.directory(), 1).is_err());
        fs::write(&path, "{").unwrap();
        assert!(load(&fixture.directory(), 1).is_err());
        fs::remove_file(&path).unwrap();
        fs::create_dir(&path).unwrap();
        assert!(load(&fixture.directory(), 1).is_err());
        fs::remove_dir(&path).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::{FileTypeExt, symlink};
            symlink(fixture.root.join("one.toml"), &path).unwrap();
            assert!(load(&fixture.directory(), 1).is_err());
            fs::remove_file(&path).unwrap();
            assert!(
                std::process::Command::new("mkfifo")
                    .arg(&path)
                    .status()
                    .unwrap()
                    .success()
            );
            assert!(fs::symlink_metadata(&path).unwrap().file_type().is_fifo());
            assert!(load(&fixture.directory(), 1).is_err());
        }
    }

    #[test]
    fn legacy_multi_replica_outcomes_map_only_by_actual_measured_attempt() {
        let fixture = Fixture::new("swe-bench-lite", 2);
        for slot in 1..=2 {
            let mut record = fixture.record(slot, json!(slot == 1), true, true);
            record["schema_version"] = json!(1);
            record.as_object_mut().unwrap().remove("replica");
            record["summary"]["replica_index"] = Value::Null;
            record["summary"]["execution_cohort"] = Value::Null;
            fixture.publish(slot, &record);
            let directory = fixture
                .directory()
                .join(format!("measurements-concurrency-{slot}"));
            fs::create_dir(&directory).unwrap();
            let mut measured = record["summary"].clone();
            measured["status"] = json!("completed");
            fs::write(directory.join("run.json"), measured.to_string()).unwrap();
        }
        let report = fixture.compare().unwrap();
        assert_eq!(report["benchmarks"][0]["metrics"]["success_rate"], 1.0);
        assert_eq!(report["benchmarks"][1]["metrics"]["success_rate"], 0.0);
        let records = fixture.directory().join(".asb-completions");
        fs::rename(
            records.join("001.json"),
            records.join("z-first-attempt.json"),
        )
        .unwrap();
        fs::rename(
            records.join("002.json"),
            records.join("a-second-attempt.json"),
        )
        .unwrap();
        let reordered = fixture.compare().unwrap();
        assert_eq!(
            reordered, report,
            "filenames may order validation but cannot choose replica identity"
        );
        fs::rename(
            records.join("z-first-attempt.json"),
            records.join("001.json"),
        )
        .unwrap();
        fs::rename(
            records.join("a-second-attempt.json"),
            records.join("002.json"),
        )
        .unwrap();
        fs::copy(
            fixture
                .directory()
                .join("measurements-concurrency-1/run.json"),
            fixture
                .directory()
                .join("measurements-concurrency-2/run.json"),
        )
        .unwrap();
        assert!(
            fixture
                .compare()
                .unwrap_err()
                .to_string()
                .contains("multiple measured replica slots")
        );
    }

    #[test]
    fn ineligible_retained_evaluation_never_claims_resolution() {
        let fixture = Fixture::new("swe-bench-lite", 1);
        for kind in ["adapter-smoke", "upstream-harness", "unknown"] {
            let mut record = fixture.record(1, json!(true), true, true);
            record["summary"]["workload_evaluation"] = json!(kind);
            record["summary"]["workload_baseline"] = json!({"kind":"unavailable"});
            fixture.publish(1, &record);
            let report = fixture.compare().unwrap();
            let row = &report["benchmarks"][0];
            assert_eq!(row["evaluation_completed"], false);
            assert!(row["metrics"].get("success_rate").is_none());
            assert!(!row["status"].as_str().unwrap().contains("task resolved"));
        }
    }

    #[test]
    fn legacy_comparison_without_a_plan_still_bounds_replica_population() {
        let fixture = Fixture::new("swe-bench-lite", 1);
        fs::remove_file(fixture.root.join("sweep-results/plan.json")).unwrap();
        let text = fs::read_to_string(fixture.root.join("one.toml")).unwrap();
        for value in ["0", "65", "-1", "'unbounded'"] {
            fs::write(
                fixture.root.join("one.toml"),
                text.replace("concurrency=1", &format!("concurrency={value}")),
            )
            .unwrap();
            assert!(
                fixture
                    .compare()
                    .unwrap_err()
                    .to_string()
                    .contains("concurrency must be an integer")
            );
        }
    }
}
