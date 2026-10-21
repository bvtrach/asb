//! Retained log diagnostics do not require an agent, grader or provider.
use super::*;

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
