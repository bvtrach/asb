//! Retained log diagnostics do not require an agent, grader or provider.
use super::*;

fn load_sweep(
    directory: &Path,
    configuration: &crate::benchmark::BenchmarkConfig,
) -> Result<BTreeMap<usize, Recorded>, Error> {
    let mut records = load(directory, configuration.concurrency)?;
    add_sweep_logs(&mut records, directory, configuration)?;
    Ok(records)
}

#[test]
fn sweep_logs_use_owned_paths_and_never_mix_a_latest_attempt_with_an_old_outcome() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/outcome-log-fixtures")
        .join(crate::workloads::TaskIdentity::local("sweep-log", None).attempt_id);
    fs::create_dir_all(&root).unwrap();
    let mut config: crate::benchmark::BenchmarkConfig = toml::from_str(
        "agent='pi'\nprovider='fixture'\nmodel='fixture'\nworkload='smoke'\nlog_file='../not-a-sweep-log'\n",
    ).unwrap();
    let summary = json!({"schema_version":2,"task":{"workload":"smoke","task_id":"smoke","attempt_id":"first","dataset":null},
        "agent":"pi","workload":"smoke","provider":"fixture","model":"fixture",
        "replica_index":1,"process_success":true,"workload_success":false,
        "stdout":"PRIVATE TRANSCRIPT","stderr":"PRIVATE STDERR"});
    let log = root.join("run.log");
    fs::write(&log, format!("{summary}\n")).unwrap();
    let loaded = load_sweep(&root, &config).unwrap();
    assert_eq!(loaded[&1].source, "run-log");
    assert!(loaded[&1].summary.get("stdout").is_none());
    let directory = root.join(".asb-completions");
    fs::create_dir(&directory).unwrap();
    let mut clean_summary = summary.clone();
    clean_summary.as_object_mut().unwrap().remove("stdout");
    clean_summary.as_object_mut().unwrap().remove("stderr");
    let outcome = json!({"schema_version":2,"scope":"task-outcome-not-yet-a-resume-receipt",
        "replica":{"index":1,"total":1},"workload_cleanup_confirmed":true,
        "proxy_cleanup_confirmed":true,"summary":clean_summary});
    let path = directory.join("outcome.json");
    let original = outcome.to_string();
    fs::write(&path, &original).unwrap();
    assert_eq!(
        load_sweep(&root, &config).unwrap()[&1].source,
        "task-outcome"
    );
    for field in ["planning", "effective_budgets"] {
        let mut logged = summary.clone();
        logged[field] = json!({"original":true});
        fs::write(&log, format!("{logged}\n")).unwrap();
        assert_eq!(
            load_sweep(&root, &config).unwrap()[&1].summary[field],
            logged[field]
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), original);
        let mut retained = outcome.clone();
        retained["summary"][field] = json!({"different":true});
        let retained_bytes = retained.to_string();
        fs::write(&path, &retained_bytes).unwrap();
        assert!(
            load_sweep(&root, &config)
                .err()
                .unwrap()
                .to_string()
                .contains("disagree on recorded")
        );
        assert_eq!(fs::read_to_string(&path).unwrap(), retained_bytes);
        fs::write(&path, &original).unwrap();
        logged[field] = json!(3);
        fs::write(&log, format!("{logged}\n")).unwrap();
        assert!(
            load_sweep(&root, &config)
                .err()
                .unwrap()
                .to_string()
                .contains("metadata must be an object")
        );
    }
    let mut latest = summary.clone();
    latest["task"]["attempt_id"] = json!("newer-attempt");
    fs::write(&log, format!("{summary}\n{latest}\n")).unwrap();
    assert!(
        load_sweep(&root, &config)
            .err()
            .unwrap()
            .to_string()
            .contains("disagree on `task`")
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), original);
    fs::remove_dir_all(&directory).unwrap();
    // Grade-only recovery/import deliberately ignores partial execution logs.
    for source in ["imported-official-grade", "recovered-official-grade"] {
        fs::write(&log, "truncated interrupted execution\n").unwrap();
        let mut retained = BTreeMap::from([(
            1,
            Recorded {
                path: root.join("retained.json"),
                summary: clean_summary.clone(),
                workload_cleanup: false,
                proxy_cleanup: false,
                source,
            },
        )]);
        add_sweep_logs(&mut retained, &root, &config).unwrap();
        assert_eq!(retained[&1].source, source);
        assert_eq!(retained[&1].summary, clean_summary);
    }
    config.concurrency = 2;
    let replica_log = root.join("run-concurrency-1.log");
    for slot in [Value::Null, json!(2)] {
        let mut wrong = summary.clone();
        wrong["replica_index"] = slot;
        fs::write(&replica_log, format!("{wrong}\n")).unwrap();
        assert!(
            load_sweep(&root, &config)
                .err()
                .unwrap()
                .to_string()
                .contains("replica slot")
        );
    }
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn latest_log_must_be_bounded_well_formed_and_strip_client_transcripts() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/outcome-log-fixtures")
        .join(crate::workloads::TaskIdentity::local("fixture", None).attempt_id);
    fs::create_dir_all(&root).unwrap();
    let path = root.join("benchmark.toml");
    let config: crate::benchmark::BenchmarkConfig = toml::from_str(
        "agent='pi'\nprovider='fixture'\nmodel='fixture'\nworkload='smoke'\nlog_file='run.log'\n",
    )
    .unwrap();
    assert!(load_run(&path, &config).unwrap().is_empty());
    let log = root.join("run.log");
    for (text, reason) in [
        ("\n \n", "run log is empty"),
        ("not JSON\n", "invalid latest run log JSON"),
        ("{}\n", "lacks a supported task summary"),
        (
            "{\"schema_version\":2,\"task\":{},\"process_success\":true,\"workload_success\":\"yes\"}\n",
            "lacks a supported task summary",
        ),
    ] {
        fs::write(&log, text).unwrap();
        let error = load_run(&path, &config).err().unwrap().to_string();
        assert!(
            error.contains(reason) && error.contains("run.log"),
            "{error}"
        );
    }
    let record = json!({"schema_version":2,"task":{},"process_success":false,"workload_success":null,"stdout":"PRIVATE TRANSCRIPT","stderr":"PRIVATE STDERR"});
    fs::write(&log, format!("invalid historical line\n{record}\n\n")).unwrap();
    let records = load_run(&path, &config).unwrap();
    let recorded = records.get(&1).unwrap();
    assert!(recorded.summary.get("stdout").is_none());
    assert!(recorded.summary.get("stderr").is_none());
    assert!(recorded.status().contains("agent process failed"));
    fs::remove_file(&log).unwrap();
    fs::create_dir(&log).unwrap();
    assert!(
        load_run(&path, &config)
            .err()
            .unwrap()
            .to_string()
            .contains("regular file")
    );
    fs::remove_dir(&log).unwrap();
    fs::write(&log, vec![b'x'; super::super::MAX_BYTES as usize + 1]).unwrap();
    assert!(
        load_run(&path, &config)
            .err()
            .unwrap()
            .to_string()
            .contains("exceeds 16 MiB")
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn outcome_directory_and_schema_errors_are_named_without_reading_other_paths() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/outcome-log-fixtures")
        .join(crate::workloads::TaskIdentity::local("fixture", None).attempt_id);
    fs::create_dir_all(&root).unwrap();
    let outcomes = root.join(".asb-completions");
    fs::write(&outcomes, "not a directory").unwrap();
    assert!(
        load(&root, 1)
            .err()
            .unwrap()
            .to_string()
            .contains("real directory")
    );
    fs::remove_file(&outcomes).unwrap();
    fs::create_dir(&outcomes).unwrap();
    fs::write(
        outcomes.join("slot.json"),
        json!({"schema_version":987}).to_string(),
    )
    .unwrap();
    assert!(
        load(&root, 1)
            .err()
            .unwrap()
            .to_string()
            .contains("unsupported")
    );
    fs::write(
        outcomes.join("slot.json"),
        json!({"schema_version":2,"replica":"bad"}).to_string(),
    )
    .unwrap();
    assert!(
        load(&root, 1)
            .err()
            .unwrap()
            .to_string()
            .contains("replica identity is malformed")
    );
    let summary = json!({"replica_index":2});
    let identity = legacy_replica(&root, &outcomes.join("slot.json"), &summary, 3).unwrap();
    assert_eq!(identity.index, 2);
    assert_eq!(identity.total, 3);
    fs::remove_dir_all(root).unwrap();
}
